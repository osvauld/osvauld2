//! What the node handed out and what it took back.
//!
//! Sealed `vault` entries, not a CRDT: the node is the sole writer here, and a grant or a
//! revocation that a peer could merge away is neither. Keys are plaintext by construction — a
//! holder's DID names its index — which adds nothing, since `vault` already names an account's
//! file after its DID.
//!
//! Everything here is **this node's own authority**: `token/<id>` is the issue,
//! `users/<did>/tokens/<id>` indexes it by holder, `users/<did>/relationship` is the claim that
//! made them known at all, and `users/<did>/meta` is left for profiles. `revoked/<id>` is what
//! *this* node revoked, never what another node announced — merged, one node's revocation
//! could shadow another's ids. The mirror image,
//! tokens this node holds from another node, is reserved for `nodes/<node-did>/`, and is the
//! same shape a desktop needs, since a user holds tokens from several nodes.

use std::collections::HashSet;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use courier::invite::{InviteClaimHello, InviteRequest, InviteTicket, InviteWelcome};
use courier::publish::{PublishAck, PublishHello};
use courier::subscribe::SubscribeHello;
use courier::sync::{SyncAck, SyncHello, SyncLayer};
use courier::token::Token;
use courier::{AdminRecord, ClaimHello, ClaimWelcome, ReconnectHello};
use serde::{Deserialize, Serialize};
use vault::{ItemKind, Vault, WorkspaceItem, WorkspaceMeta};

use crate::push::{Push, Pusher};
use crate::{NodeError, node};

const RECORD: &str = "token/";
const USERS: &str = "users/";
const REVOKED: &str = "revoked/";
const RELATIONSHIP: &str = "/relationship";
/// One entry per redeemed invite nonce, empty-valued like `revoked/<id>` — its presence is the
/// whole fact. Its own namespace, not under `token/`, because a spent nonce is not a token: an
/// invite ticket dies unredeemed as often as not, and this only ever holds the ones that were.
const INVITES: &str = "invites/";
/// `subscriptions/<ws_id>/<item_id-b64>/<layer-b64>/<did>`, empty-valued like `revoked/<id>` —
/// presence is the whole fact. `item_id` and the layer are base64'd before becoming key
/// segments: unlike `ws_id` (already shape-checked by `node_accept_subscription`'s membership
/// check before this is reached) and `did` (a DID carries no `/`, same trust `relationship_key`
/// already places in one), neither is validated the way a minted id is — a crafted `item_id`
/// containing `/` must not be able to forge a different subscription's key.
const SUBSCRIPTIONS: &str = "subscriptions/";

/// Why a token exists. Lineage, not history: the node reissues flat tokens to keep chains
/// short, and a flat token no longer carries its issuer in `prf` — without this, revoking
/// that issuer would stop cascading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cause {
    /// The node's own decision — the first owner, or an operator at the console.
    Node,
    /// Minted under another token's authority, so it dies when that one does.
    Under([u8; 32]),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub token: Token,
    pub holder: String,
    pub cause: Cause,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revocation {
    pub at: u64,
}

/// The node's own records, over an unlocked account. Cloning is cheap and shares the account.
#[derive(Clone)]
pub struct Admin {
    vault: Vault,
}

impl Admin {
    pub fn new(vault: Vault) -> Self {
        Self { vault }
    }

    /// Holder and id come from the token itself: a record that disagrees with the token it
    /// describes is worse than no record at all.
    pub fn record(&self, token: &Token, cause: Cause, at: u64) -> Result<[u8; 32], NodeError> {
        let id = token.id();
        let holder = token.claims()?.aud;
        let issue = Issue {
            token: token.clone(),
            holder: holder.clone(),
            cause,
            at,
        };
        // Record first, index second: a crash between them hides a token rather than
        // promising one that isn't there.
        self.vault
            .put_entry(&record_key(&id), &serde_json::to_vec(&issue)?)?;
        self.vault.put_entry(&tokens_key(&holder, &id), &[])?;
        Ok(id)
    }

    pub fn issue(&self, id: &[u8; 32]) -> Result<Option<Issue>, NodeError> {
        match self.vault.get_entry(&record_key(id))? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Every token the node issued to `holder`, revoked ones included: the log keeps what it
    /// handed out, the revoked set says what still counts.
    pub fn issued_to(&self, holder: &str) -> Result<Vec<Issue>, NodeError> {
        let mut issues = Vec::new();
        for name in self.vault.list_entries(&tokens_scan(holder))? {
            let id = id_in(&name)?;
            // The record is written first, so an index without one is a damaged store, not a
            // race. An audit log that quietly under-reports is the worse failure.
            issues.push(self.issue(&id)?.ok_or(NodeError::Damaged(name))?);
        }
        Ok(issues)
    }

    /// Every desktop that has claimed this node. courier decides the first-admin question
    /// against this list, so a node that could not rebuild it would let the next caller claim
    /// an already-claimed node.
    pub fn admins(&self) -> Result<Vec<AdminRecord>, NodeError> {
        let mut admins = Vec::new();
        for name in self.vault.list_entries(USERS)? {
            if !name.ends_with(RELATIONSHIP) {
                continue;
            }
            let bytes = self
                .vault
                .get_entry(&name)?
                .ok_or_else(|| NodeError::Damaged(name.clone()))?;
            admins.push(serde_json::from_slice(&bytes)?);
        }
        Ok(admins)
    }

    /// Claim handling with the list on disk. courier decides; this supplies what it decides
    /// against and keeps what it added. Loaded before signing and written after, because
    /// `with_signer` holds the account for its closure.
    pub fn accept_claim(&self, hello: ClaimHello, now: u64) -> Result<ClaimWelcome, NodeError> {
        let mut admins = self.admins()?;
        let welcome = self
            .vault
            .with_signer(|node| courier::node_accept_claim(hello, node, &mut admins, now))
            .ok_or(NodeError::Locked)??;
        // The whole list, not the tail: courier only appends today, and this does not have to
        // know that to stay correct.
        for admin in &admins {
            self.vault
                .put_entry(&relationship_key(&admin.did), &serde_json::to_vec(admin)?)?;
        }
        // The node's own decision, with nothing above it — which is what `Cause::Node` is for.
        // Recorded here and not inside courier, because courier never touches storage.
        self.record(&welcome.token, Cause::Node, now)?;
        Ok(welcome)
    }

    /// Verify a publish and adopt what it announced. `known_items` is read before the check,
    /// so the ack reports what the node held *before* this call, and nothing is written until
    /// the token is proven.
    pub fn accept_publish(&self, hello: PublishHello, now: u64) -> Result<PublishAck, NodeError> {
        let node_did = node::did(&self.vault)?;
        let known_items = self
            .vault
            .items(&hello.workspace.id)?
            .into_iter()
            .map(|item| item.id)
            .collect();
        let ack = courier::publish::node_accept_publish(
            &hello,
            &node_did,
            known_items,
            now,
            &self.revoked()?,
        )?;

        let ws_id = hello.workspace.id.clone();
        self.vault.adopt_workspace(&WorkspaceMeta {
            id: hello.workspace.id,
            name: hello.workspace.name,
            created: hello.workspace.created,
        })?;
        for item in hello.items {
            let kind = ItemKind::parse(&item.kind).ok_or(NodeError::BadItemKind(item.kind))?;
            self.vault.adopt_item(&WorkspaceItem {
                id: item.id,
                ws_id: ws_id.clone(),
                name: item.name,
                kind,
                created: item.created,
            })?;
        }
        Ok(ack)
    }

    /// Verify a sync push and merge it into the node's stored snapshot for that item's layer.
    /// The current snapshot is read before the check, same reasoning as `accept_publish`'s
    /// `known_items`: `node_accept_sync` needs it to compute the merge either way, and reading
    /// it first means a rejected push never touches storage. `None` the first time this layer
    /// is synced — courier treats that as an empty document, not an error.
    ///
    /// Fans the new snapshot out to every other subscriber on this layer once it's stored.
    /// `pusher` is generic, not `Admin`'s own field: `Admin` is the node's records, and which
    /// transport (or none, via `NoopPusher`) delivers a push is a different axis entirely, the
    /// same way `accept_publish` doesn't own how a publish reached it over the wire.
    pub fn accept_sync(
        &self,
        hello: SyncHello,
        now: u64,
        pusher: &impl Pusher,
    ) -> Result<SyncAck, NodeError> {
        let node_did = node::did(&self.vault)?;
        let current = self.load_layer(&hello.ws_id, &hello.item_id, &hello.layer)?;
        let (ack, snapshot) = courier::sync::node_accept_sync(
            &hello,
            &node_did,
            current.as_deref(),
            now,
            &self.revoked()?,
        )?;
        self.store_layer(&hello.ws_id, &hello.item_id, &hello.layer, &snapshot)?;
        self.fan_out(&hello, &snapshot, pusher)?;
        Ok(ack)
    }

    /// Full snapshot, not a diff against each subscriber's own progress: no subscription tracks
    /// a version vector (see `SUBSCRIPTIONS`'s doc comment), and building one now would reopen
    /// the exact hazard `sync.rs`'s module doc already names — a vector advanced on send rather
    /// than confirmed delivery. Loro's `import` merges a full snapshot the same as a diff, so a
    /// subscriber who already has all of it simply merges a no-op.
    fn fan_out(
        &self,
        hello: &SyncHello,
        snapshot: &[u8],
        pusher: &impl Pusher,
    ) -> Result<(), NodeError> {
        let update = Push {
            ws_id: hello.ws_id.clone(),
            item_id: hello.item_id.clone(),
            layer: hello.layer.clone(),
            snapshot: snapshot.to_vec(),
        };
        for subscriber in self.subscribers_for(&hello.ws_id, &hello.item_id, &hello.layer)? {
            if subscriber != hello.desktop_did {
                pusher.push(&subscriber, &update);
            }
        }
        Ok(())
    }

    fn load_layer(
        &self,
        ws_id: &str,
        item_id: &str,
        layer: &SyncLayer,
    ) -> Result<Option<Vec<u8>>, NodeError> {
        Ok(match layer {
            SyncLayer::Src => self.vault.get_src(ws_id, item_id)?,
            SyncLayer::Doc(name) => self.vault.get_doc(ws_id, item_id, name)?,
        })
    }

    fn store_layer(
        &self,
        ws_id: &str,
        item_id: &str,
        layer: &SyncLayer,
        snapshot: &[u8],
    ) -> Result<(), NodeError> {
        match layer {
            SyncLayer::Src => self.vault.put_src(ws_id, item_id, snapshot)?,
            SyncLayer::Doc(name) => self.vault.put_doc(ws_id, item_id, snapshot, name)?,
        }
        Ok(())
    }

    /// Record that `hello`'s holder wants future pushes for this layer. Authorization first,
    /// same shape as every other accept method here — nothing is written until the request is
    /// proven.
    pub fn subscribe(&self, hello: &SubscribeHello, now: u64) -> Result<(), NodeError> {
        let node_did = node::did(&self.vault)?;
        courier::subscribe::node_accept_subscription(hello, &node_did, now, &self.revoked()?)?;
        self.vault.put_entry(&subscription_key(hello), &[])?;
        Ok(())
    }

    /// Drop a subscription. Same authorization as `subscribe` — a holder unsubscribes with the
    /// same grant that would let them subscribe, nothing further to prove to remove themselves.
    pub fn unsubscribe(&self, hello: &SubscribeHello, now: u64) -> Result<(), NodeError> {
        let node_did = node::did(&self.vault)?;
        courier::subscribe::node_accept_subscription(hello, &node_did, now, &self.revoked()?)?;
        self.vault.delete_entry(&subscription_key(hello))?;
        Ok(())
    }

    /// Prove a `Listen` connection before the bridge holds it open. Node-wide, not per-item —
    /// see `courier::subscribe::node_accept_listen`'s own doc for why.
    pub fn authorize_listen(
        &self,
        desktop_did: &str,
        token: &Token,
        now: u64,
    ) -> Result<(), NodeError> {
        let node_did = node::did(&self.vault)?;
        courier::subscribe::node_accept_listen(
            token,
            &node_did,
            desktop_did,
            now,
            &self.revoked()?,
        )?;
        Ok(())
    }

    /// Every desktop DID subscribed to this item's layer — what a future push fans out to.
    pub fn subscribers_for(
        &self,
        ws_id: &str,
        item_id: &str,
        layer: &SyncLayer,
    ) -> Result<Vec<String>, NodeError> {
        Ok(self
            .vault
            .list_entries(&subscription_prefix(ws_id, item_id, layer))?
            .iter()
            .filter_map(|name| name.rsplit('/').next())
            .map(str::to_string)
            .collect())
    }

    /// Mint an invite. `revoked` is read for the same reason `accept_publish` reads it: the
    /// inviter's own token must still be good, not merely well-formed, at the moment it is spent
    /// asking for someone else's.
    pub fn issue_invite(
        &self,
        request: &InviteRequest,
        name: &str,
        now: u64,
    ) -> Result<InviteTicket, NodeError> {
        let revoked = self.revoked()?;
        let ticket = self
            .vault
            .with_signer(|node| {
                courier::invite::issue_invite_ticket(node, request, name, now, &revoked)
            })
            .ok_or(NodeError::Locked)??;
        Ok(ticket)
    }

    /// Every nonce this node has already redeemed an invite for. Read whole, same reasoning as
    /// `revoked()`: a per-nonce check would be a decrypt per nonce on every redemption attempt.
    pub fn redeemed_invites(&self) -> Result<HashSet<String>, NodeError> {
        Ok(self
            .vault
            .list_entries(INVITES)?
            .iter()
            .map(|name| name.rsplit('/').next().unwrap_or_default().to_string())
            .collect())
    }

    /// Invite redemption with the spent-nonce set on disk instead of courier's in-memory
    /// `revoked`/`redeemed` inputs. Loaded before signing, because `with_signer` holds the
    /// account for its closure. The nonce is marked spent *before* the new token is recorded: a
    /// crash between the two leaves the invite unredeemable rather than redeemable twice.
    pub fn accept_invite(
        &self,
        hello: InviteClaimHello,
        now: u64,
    ) -> Result<InviteWelcome, NodeError> {
        let redeemed = self.redeemed_invites()?;
        let welcome = self
            .vault
            .with_signer(|node| courier::invite::node_accept_invite(hello, node, now, &redeemed))
            .ok_or(NodeError::Locked)??;
        self.vault
            .put_entry(&invite_key(&welcome.redeemed_nonce), &[])?;
        self.record(&welcome.token, Cause::Node, now)?;
        Ok(welcome)
    }

    /// Reconnect reads the admin list and the revoked set, and hands back a fresh token.
    /// Challenges stay in memory on purpose — a restart should invalidate every nonce it
    /// handed out. Both sets are loaded before signing, because `with_signer` holds the
    /// account for its closure.
    pub fn accept_reconnect(
        &self,
        hello: ReconnectHello,
        challenges: &mut Vec<String>,
        now: u64,
    ) -> Result<Token, NodeError> {
        let admins = self.admins()?;
        let revoked = self.revoked()?;
        let token = self
            .vault
            .with_signer(|node| {
                courier::node_accept_reconnect(hello, node, &admins, challenges, now, &revoked)
            })
            .ok_or(NodeError::Locked)??;
        // A reissue is an issuance, so it joins the log. That makes the log grow by one per
        // reconnect and leaves the superseded token listed as well; superseding is in the
        // backlog, and under-reporting what is live would be the worse of the two.
        self.record(&token, Cause::Node, now)?;
        Ok(token)
    }

    /// Unknown ids are accepted: delegations are minted between holders and the node never
    /// sees one until it is presented, so revocation cannot require a record.
    pub fn revoke(&self, id: &[u8; 32], at: u64) -> Result<(), NodeError> {
        self.vault
            .put_entry(&revoked_key(id), &serde_json::to_vec(&Revocation { at })?)?;
        Ok(())
    }

    /// What *this* node revoked, which is the set its own chain check consumes — a chain
    /// rooted elsewhere is answered by that node's set, under `nodes/`, not by this one.
    /// Read whole, because a check that asked per link would be a decrypt per link on every
    /// request. An unreadable name fails the whole read rather than returning a set that is
    /// quietly missing a revocation.
    pub fn revoked(&self) -> Result<HashSet<[u8; 32]>, NodeError> {
        self.vault
            .list_entries(REVOKED)?
            .iter()
            .map(|name| id_in(name.as_str()))
            .collect()
    }
}

fn record_key(id: &[u8; 32]) -> String {
    format!("{RECORD}{}", name_of(id))
}

/// `users/<did>/tokens/` — a level down from `users/<did>/`, so a profile can sit beside the
/// tokens without the listing scan picking it up.
fn tokens_scan(holder: &str) -> String {
    format!("{USERS}{holder}/tokens/")
}

fn tokens_key(holder: &str, id: &[u8; 32]) -> String {
    format!("{}{}", tokens_scan(holder), name_of(id))
}

/// `users/<did>/relationship` — one per claimant, found by scanning `users/` for the suffix.
fn relationship_key(did: &str) -> String {
    format!("{USERS}{did}{RELATIONSHIP}")
}

fn revoked_key(id: &[u8; 32]) -> String {
    format!("{REVOKED}{}", name_of(id))
}

/// The nonce is already URL-safe (courier's `nonce()` base64url-encodes it), so it is used
/// directly as the key's last segment rather than re-encoded.
fn invite_key(nonce: &str) -> String {
    format!("{INVITES}{nonce}")
}

fn name_of(id: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(id)
}

fn subscription_key(hello: &SubscribeHello) -> String {
    format!(
        "{}{}",
        subscription_prefix(&hello.ws_id, &hello.item_id, &hello.layer),
        hello.desktop_did
    )
}

fn subscription_prefix(ws_id: &str, item_id: &str, layer: &SyncLayer) -> String {
    let item = URL_SAFE_NO_PAD.encode(item_id);
    let layer = URL_SAFE_NO_PAD.encode(layer_tag(layer));
    format!("{SUBSCRIPTIONS}{ws_id}/{item}/{layer}/")
}

/// `Src` and `Doc(name)` as distinct strings before either is encoded — so `Doc("src")` and
/// `Src` itself can never collide regardless of what `name` contains.
fn layer_tag(layer: &SyncLayer) -> String {
    match layer {
        SyncLayer::Src => "src".to_string(),
        SyncLayer::Doc(name) => format!("doc:{name}"),
    }
}

/// The id is always the last segment, whichever index the name came from.
fn id_in(name: &str) -> Result<[u8; 32], NodeError> {
    let last = name.rsplit('/').next().unwrap_or_default();
    URL_SAFE_NO_PAD
        .decode(last)
        .ok()
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| NodeError::Damaged(name.to_string()))
}

#[cfg(test)]
mod tests;
