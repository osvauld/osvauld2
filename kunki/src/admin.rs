//! What the node handed out and what it took back.
//!
//! Sealed `vault` entries, not a CRDT: the node is the sole writer here, and a grant or a
//! revocation that a peer could merge away is neither. Keys are plaintext by construction — a
//! holder's DID names its index — which adds nothing, since `vault` already names an account's
//! file after its DID.
//!
//! Everything here is **one root's authority**: `token/<id>` is the issue,
//! `users/<did>/tokens/<id>` indexes it by holder, `users/<did>/relationship` is the claim that
//! made them known at all, and `users/<did>/meta` is left for profiles. `revoked/<id>` is what
//! *this* root revoked, never what another node announced — merged, one node's revocation
//! could shadow another's ids. The mirror image,
//! tokens this node holds from another node, is reserved for `nodes/<node-did>/`, and is the
//! same shape a desktop needs, since a user holds tokens from several nodes.
//!
//! The root is this account ([`Admin::new`]) or, for a space this account hosts on an owner's
//! behalf, that owner ([`Admin::hosted`], laptop-host §2 "Appointed hosts"). A hosted admin's
//! records sit under `hosted/<owner-did>/` so two owners' `Scope::Node` tokens never meet; it
//! holds the owner's signed ledger replayed ([`Admin::replay`]) and mints nothing — the
//! owner's key is not here to mint with.

use std::collections::{BTreeSet, HashSet};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use courier::CourierError;
use courier::access::{Access, doc_access, members_in, roles_at};
use courier::invite::{InviteClaimHello, InviteRequest, InviteTicket, InviteWelcome};
use courier::ledger::{self, Entry, Signed};
use courier::link::HostAddr;
use courier::publish::{PublishAck, PublishHello};
use courier::role::{Grant, HOST, RoleRequest};
use courier::subscribe::SubscribeHello;
use courier::sync::{Author, SyncHello, SyncLayer, SyncReply, Writer, item_scope, reaches};
use courier::token::{Scope, Token, issue_root};
use courier::{AdminRecord, ClaimHello, ClaimWelcome, ReconnectHello};
use identity::{Signer, public_key_from_did};
use manifest::{Manifest, OWNER};
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
/// `invite-causes/<nonce>` → the id of the grant whose holder asked for that invite, so the
/// token it is redeemed for records `Cause::Under` that grant. Node-local, keyed by nonce: the
/// signed ticket stays unchanged.
const INVITE_CAUSES: &str = "invite-causes/";
/// `subscriptions/<ws_id>/<item_id-b64>/<layer-b64>/<did>`, empty-valued like `revoked/<id>` —
/// presence is the whole fact. `item_id` and the layer are base64'd before becoming key
/// segments: unlike `ws_id` (already shape-checked by `node_accept_subscription`'s membership
/// check before this is reached) and `did` (a DID carries no `/`, same trust `relationship_key`
/// already places in one), neither is validated the way a minted id is — a crafted `item_id`
/// containing `/` must not be able to forge a different subscription's key.
const SUBSCRIPTIONS: &str = "subscriptions/";
/// `codes/<code>` — a share link's code and the public invite it stands for, until it
/// expires (laptop-host step 5). A code is a lookup key for an invite, not an authority of
/// its own: revoking the invite's nonce kills the code with it.
const CODES: &str = "codes/";
/// `hosted/<owner-did>/` — the prefix every record of a hosted admin lives under.
const HOSTED: &str = "hosted/";
/// `hostaddr/<ws_id>/<did>` — where an appointed host said it can be dialled
/// (`Frame::HostHere`). A record, not an authority: [`Admin::hosts_of`] says whether the DID
/// is still a host, and only then is the address listed.
const HOST_ADDRS: &str = "hostaddr/";
/// How long a share link works. Tinline's number; a link is handed around, not kept.
pub const CODE_TTL: u64 = 7 * 24 * 60 * 60;
/// How long an appointment lives. An appointment is standing — revocation, not expiry, ends
/// it — so this is only a ceiling on a host the owner forgot, not a renewal cadence.
pub const HOST_TTL: u64 = 365 * 24 * 60 * 60;

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

/// What a share link's code stands for while it lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Code {
    ticket: InviteTicket,
    ws_id: String,
    exp: u64,
}

/// What a sync was judged on — see [`Admin::judge`].
#[derive(Debug, Clone)]
pub struct Judged {
    pub node_did: String,
    pub revoked: HashSet<[u8; 32]>,
    pub access: Access,
    pub undeclared: bool,
    pub rules: Option<Manifest>,
    /// Whose changes the push may carry (`owned-entries.md` §2.1).
    pub author: Author,
    /// The caller's roles on the doc, for the manifest's rules (`actor has`).
    pub roles: BTreeSet<String>,
}

impl Judged {
    /// The writer as `courier::sync` judges a push: the rules are the manifest's declaration
    /// for this doc, none for the source layer or an app without a manifest.
    pub fn writer(&self, layer: &SyncLayer) -> Writer<'_> {
        let rules = match (&self.rules, layer) {
            (Some(m), SyncLayer::Doc(name)) => m.resolve(name).map(|(decl, _)| (m, decl)),
            _ => None,
        };
        Writer {
            author: self.author,
            roles: self.roles.clone(),
            rules,
        }
    }
}

/// One root's records, over an unlocked account. Cloning is cheap and shares the account.
#[derive(Clone)]
pub struct Admin {
    vault: Vault,
    /// `None`: this account is the root and signs. `Some(did)`: the owner whose records these
    /// are — every chain roots there, and this admin mints nothing.
    owner: Option<String>,
    /// `""` for the account's own records, `hosted/<owner-did>/` for an owner's.
    prefix: String,
}

impl Admin {
    /// This account's own authority.
    pub fn new(vault: Vault) -> Self {
        Self {
            vault,
            owner: None,
            prefix: String::new(),
        }
    }

    /// `owner_did`'s authority, held here: its ledger replayed under `hosted/<owner_did>/`,
    /// its DID as the root every presented chain must reach.
    pub fn hosted(vault: Vault, owner_did: &str) -> Self {
        Self {
            vault,
            owner: Some(owner_did.to_string()),
            prefix: format!("{HOSTED}{owner_did}/"),
        }
    }

    /// The DID every token here is rooted at: this account's, or the hosted owner's.
    pub fn root(&self) -> Result<String, NodeError> {
        match &self.owner {
            Some(did) => Ok(did.clone()),
            None => node::did(&self.vault),
        }
    }

    /// Sign as the root. A hosted admin has no key for its root — the owner's — so it mints
    /// nothing: a token signed here would be recorded as the owner's and fail every chain
    /// check against the owner's DID, which is worse than a refusal.
    fn with_root_signer<R>(&self, f: impl FnOnce(&dyn Signer) -> R) -> Result<R, NodeError> {
        if let Some(owner) = &self.owner {
            return Err(NodeError::NotOwner(owner.clone()));
        }
        self.vault.with_signer(f).ok_or(NodeError::Locked)
    }

    // Every record goes through these four, so the prefix is applied in one place and the
    // rest of this file speaks names relative to it (`token/<id>`, never `hosted/…/token/<id>`).

    fn key(&self, name: &str) -> String {
        format!("{}{name}", self.prefix)
    }

    fn put_entry(&self, name: &str, bytes: &[u8]) -> Result<(), NodeError> {
        Ok(self.vault.put_entry(&self.key(name), bytes)?)
    }

    fn get_entry(&self, name: &str) -> Result<Option<Vec<u8>>, NodeError> {
        Ok(self.vault.get_entry(&self.key(name))?)
    }

    fn delete_entry(&self, name: &str) -> Result<(), NodeError> {
        Ok(self.vault.delete_entry(&self.key(name))?)
    }

    /// Names under `under`, relative to this admin's prefix.
    fn list_entries(&self, under: &str) -> Result<Vec<String>, NodeError> {
        Ok(self
            .vault
            .list_entries(&self.key(under))?
            .iter()
            .filter_map(|name| name.strip_prefix(&self.prefix))
            .map(str::to_string)
            .collect())
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
        self.put_entry(&record_key(&id), &serde_json::to_vec(&issue)?)?;
        self.put_entry(&tokens_key(&holder, &id), &[])?;
        Ok(id)
    }

    pub fn issue(&self, id: &[u8; 32]) -> Result<Option<Issue>, NodeError> {
        match self.get_entry(&record_key(id))? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Every token the root issued to `holder`, revoked ones included: the log keeps what it
    /// handed out, the revoked set says what still counts.
    pub fn issued_to(&self, holder: &str) -> Result<Vec<Issue>, NodeError> {
        let mut issues = Vec::new();
        for name in self.list_entries(&tokens_scan(holder))? {
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
        for name in self.list_entries(USERS)? {
            if !name.ends_with(RELATIONSHIP) {
                continue;
            }
            let bytes = self
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
            .with_root_signer(|node| courier::node_accept_claim(hello, node, &mut admins, now))??;
        // The whole list, not the tail: courier only appends today, and this does not have to
        // know that to stay correct.
        for admin in &admins {
            self.put_entry(&relationship_key(&admin.did), &serde_json::to_vec(admin)?)?;
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
        let node_did = self.root()?;
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
    /// A doc sync is judged by the app's manifest first (`courier::access`): the caller's roles
    /// from its grants on record, `members(...)` from the node's own copies.
    ///
    /// A write the rules refuse is answered with `SyncReply::Rejected`, not an error: nothing is
    /// stored or pushed, and the writer rolls back (`group-chat-sync.md` §5).
    ///
    /// Fans the new snapshot out to every other subscriber on this layer once it's stored —
    /// only when the merge moved the node's copy (`SyncAck::changed`); a push of what the node
    /// already held is acked and goes no further, which is what ends a relay loop between
    /// hosts (laptop-host §2 "Relay only what was new").
    /// `pusher` is generic, not `Admin`'s own field: `Admin` is the node's records, and which
    /// transport (or none, via `NoopPusher`) delivers a push is a different axis entirely, the
    /// same way `accept_publish` doesn't own how a publish reached it over the wire.
    /// `device` is as for [`judge`].
    pub fn accept_sync(
        &self,
        hello: SyncHello,
        device: Option<[u8; 32]>,
        now: u64,
        pusher: &impl Pusher,
    ) -> Result<SyncReply, NodeError> {
        let judged = self.judge(&hello, device, now)?;
        let current = self.load_layer(&hello.ws_id, &hello.item_id, &hello.layer)?;
        let why = match courier::sync::node_accept_sync(
            &hello,
            &judged.node_did,
            current.as_deref(),
            judged.access,
            &judged.writer(&hello.layer),
            now,
            &judged.revoked,
        ) {
            Ok(_) if judged.undeclared => CourierError::Undeclared,
            Ok((ack, snapshot)) => {
                self.store_layer(&hello.ws_id, &hello.item_id, &hello.layer, &snapshot)?;
                // Relay only what was new: a merge that moved nothing is acked and stops
                // here, which is what ends a relay loop between hosts.
                if ack.changed {
                    self.fan_out(&hello, &snapshot, judged.rules.as_ref(), now, pusher)?;
                }
                return Ok(SyncReply::Ack(ack));
            }
            Err(CourierError::NoRead | CourierError::NoWrite) if judged.undeclared => {
                CourierError::Undeclared
            }
            Err(why) => why,
        };
        // A refused write is answered, not failed: the writer rolls back to the node's copy.
        match courier::sync::node_reject(&hello, &why, current.as_deref(), judged.access)? {
            Some(rejected) => Ok(SyncReply::Rejected(rejected)),
            None => Err(why.into()),
        }
    }

    /// What a sync is judged on, read from this host's records before any doc is touched:
    /// the caller's grants against the manifest's rules. Split from [`accept_sync`] so a host
    /// with the doc *open* can judge here and merge on the app's own thread
    /// ([`accept_live`]) — the vault is read, never written, on this path.
    ///
    /// `device` is the key the transport proved the pusher holds, which binds the push to
    /// that device's peer (`owned-entries.md` §2.1). `None` is a caller the node trusts as
    /// itself: its own machine over the local socket, or its own fan-out.
    pub fn judge(
        &self,
        hello: &SyncHello,
        device: Option<[u8; 32]>,
        now: u64,
    ) -> Result<Judged, NodeError> {
        let node_did = self.root()?;
        let revoked = self.revoked()?;
        let rules = self.rules(&hello.ws_id, &hello.item_id, &hello.layer)?;
        let grants = self.grants_given(&hello.desktop_did, now, &revoked)?;
        let (ws, item, layer) = (&hello.ws_id, &hello.item_id, &hello.layer);
        // An appointed host relays what another host judged already: open on every doc of
        // its space, declared or not. The rules are still read, for the fan-out's re-check of
        // each subscriber. The root itself needs no case here — its own `owner` grant
        // satisfies every role rule through the manifest's `owner`.
        let host = grants.iter().any(|g| appoints(&g.role, &g.scope, ws));
        let roles = match layer {
            SyncLayer::Doc(name) => roles_at(&grants, ws, item, name),
            SyncLayer::Src => BTreeSet::new(),
        };
        // A host carries others' changes by design; so does the node itself.
        let author = match device {
            Some(device) if !host => Author::Device(identity::peer_of(&device)),
            _ => Author::Host,
        };
        // An undeclared doc is judged as one nobody may read or write, so membership still
        // runs first and a stranger is refused as a stranger.
        let (access, undeclared) = if host {
            (Access::OPEN, false)
        } else {
            match self.access(rules.as_ref(), ws, item, layer, &hello.desktop_did, &grants) {
                Ok(access) => (access, false),
                Err(NodeError::Courier(CourierError::Undeclared)) => (
                    Access {
                        read: false,
                        write: false,
                    },
                    true,
                ),
                Err(e) => return Err(e),
            }
        };
        Ok(Judged {
            node_did,
            revoked,
            access,
            undeclared,
            rules,
            roles,
            author,
        })
    }

    /// [`accept_sync`] over the live doc of an app this host has open (laptop-host §2, "One
    /// copy per doc"): the merge lands in `doc` and nothing is stored here — the app's own
    /// flush persists it, so the vault keeps one writer per doc. Fan-out still runs from
    /// here, off the merged doc's snapshot.
    pub fn accept_live(
        &self,
        doc: &loro::LoroDoc,
        hello: &SyncHello,
        judged: &Judged,
        now: u64,
        pusher: &impl Pusher,
    ) -> Result<SyncReply, NodeError> {
        let snapshot = |doc: &loro::LoroDoc| {
            doc.export(loro::ExportMode::Snapshot)
                .map_err(|_| NodeError::Courier(CourierError::Decode))
        };
        let why = match courier::sync::accept_into(
            doc,
            hello,
            &judged.node_did,
            judged.access,
            &judged.writer(&hello.layer),
            now,
            &judged.revoked,
        ) {
            Ok(_) if judged.undeclared => CourierError::Undeclared,
            Ok(ack) => {
                if ack.changed {
                    let merged = snapshot(doc)?;
                    self.fan_out(hello, &merged, judged.rules.as_ref(), now, pusher)?;
                }
                return Ok(SyncReply::Ack(ack));
            }
            Err(CourierError::NoRead | CourierError::NoWrite) if judged.undeclared => {
                CourierError::Undeclared
            }
            Err(why) => why,
        };
        let current = snapshot(doc)?;
        match courier::sync::node_reject(hello, &why, Some(&current), judged.access)? {
            Some(rejected) => Ok(SyncReply::Rejected(rejected)),
            None => Err(why.into()),
        }
    }

    /// The manifest a doc layer is judged by; `None` for the source layer, which has its own
    /// gate. An item whose source the node doesn't hold has declared nothing yet.
    fn rules(
        &self,
        ws_id: &str,
        item_id: &str,
        layer: &SyncLayer,
    ) -> Result<Option<Manifest>, NodeError> {
        if *layer == SyncLayer::Src {
            return Ok(None);
        }
        match self.manifest(&item_scope(ws_id, item_id)) {
            Err(NodeError::NoManifest(_)) => Ok(Some(Manifest::default())),
            other => other.map(Some),
        }
    }

    /// `did`'s access to `layer` of `item` under `rules`, holding `grants`.
    fn access(
        &self,
        rules: Option<&Manifest>,
        ws: &str,
        item: &str,
        layer: &SyncLayer,
        did: &str,
        grants: &[Grant],
    ) -> Result<Access, NodeError> {
        let (Some(rules), SyncLayer::Doc(name)) = (rules, layer) else {
            return Ok(Access::OPEN);
        };
        let roles = roles_at(grants, ws, item, name);
        doc_access(rules, name, did, &roles, &mut |doc: &str| {
            Ok::<_, NodeError>(members_in(self.vault.get_doc(ws, item, doc)?.as_deref())?)
        })
    }

    /// Full snapshot, not a diff against each subscriber's own progress: no subscription tracks
    /// a version vector (see `SUBSCRIPTIONS`'s doc comment), and building one now would reopen
    /// the exact hazard `sync.rs`'s module doc already names — a vector advanced on send rather
    /// than confirmed delivery. Loro's `import` merges a full snapshot the same as a diff, so a
    /// subscriber who already has all of it simply merges a no-op.
    ///
    /// Each subscriber is re-checked against its live grants: a subscription outlives the
    /// grant that allowed it, and a revoked member must stop receiving. So is the manifest's read rule, whose inputs (a
    /// group's member list) change after the subscription was made.
    ///
    /// Ends with one [`Pusher::broadcast`], for the host sessions, whether or not any
    /// subscriber was pushed to.
    pub fn fan_out(
        &self,
        hello: &SyncHello,
        snapshot: &[u8],
        rules: Option<&Manifest>,
        now: u64,
        pusher: &impl Pusher,
    ) -> Result<(), NodeError> {
        let update = Push {
            ws_id: hello.ws_id.clone(),
            item_id: hello.item_id.clone(),
            layer: hello.layer.clone(),
            snapshot: snapshot.to_vec(),
            author: hello.desktop_did.clone(),
            origin: hello.request_id.clone(),
        };
        self.relay(&update, rules, now, pusher)
    }

    /// [`fan_out`] over the push itself — [`accept_trusted`] has no hello to build one from.
    fn relay(
        &self,
        update: &Push,
        rules: Option<&Manifest>,
        now: u64,
        pusher: &impl Pusher,
    ) -> Result<(), NodeError> {
        let (ws, item, layer) = (&update.ws_id, &update.item_id, &update.layer);
        let revoked = self.revoked()?;
        for subscriber in self.subscribers_for(ws, item, layer)? {
            // The author's other sessions still hear, where the pusher can tell them apart.
            if subscriber == update.author && pusher.excludes_author() {
                continue;
            }
            // Cost: one index scan per subscriber per push. Fine at chat scale; a live
            // grant index is the fix when it is not.
            let grants = self.grants_given(&subscriber, now, &revoked)?;
            let reached = grants.iter().any(|g| reaches(&g.scope, ws, item, layer));
            if reached
                && self
                    .access(rules, ws, item, layer, &subscriber, &grants)?
                    .read
            {
                pusher.push(&subscriber, update);
            }
        }
        // Hosts hear everything, subscribed or not; the pusher knows which sessions those are.
        pusher.broadcast(update);
        Ok(())
    }

    /// An update from the owner's own device over a host's uplink (laptop-host §2 "Appointed
    /// hosts"): the link named that device, so there is no token to judge — it is merged and
    /// relayed as the root's own write. Returns whether the stored copy moved; when it did
    /// not, nothing is stored or relayed, which with [`accept_sync`]'s same rule is what ends
    /// owner → host → owner loops. `origin` is the uplink session's id, for
    /// [`Pusher::broadcast`] to leave that session out.
    pub fn accept_trusted(
        &self,
        ws_id: &str,
        item_id: &str,
        layer: &SyncLayer,
        update: &[u8],
        origin: &str,
        now: u64,
        pusher: &impl Pusher,
    ) -> Result<bool, NodeError> {
        if update.is_empty() {
            return Ok(false);
        }
        let doc = loro::LoroDoc::new();
        if let Some(bytes) = self.load_layer(ws_id, item_id, layer)? {
            doc.import(&bytes).map_err(undecodable)?;
        }
        let before = doc.oplog_vv();
        doc.import(update).map_err(undecodable)?;
        if doc.oplog_vv() == before {
            return Ok(false);
        }
        let snapshot = doc
            .export(loro::ExportMode::Snapshot)
            .map_err(undecodable)?;
        self.store_layer(ws_id, item_id, layer, &snapshot)?;
        let rules = self.rules(ws_id, item_id, layer)?;
        let push = Push {
            ws_id: ws_id.to_string(),
            item_id: item_id.to_string(),
            layer: layer.clone(),
            snapshot,
            author: self.root()?,
            origin: origin.to_string(),
        };
        self.relay(&push, rules.as_ref(), now, pusher)?;
        Ok(true)
    }

    /// Every issue on record, in id order — one decrypt per record, for the scans that need
    /// the whole log ([`ledger`], [`hosts_of`]) rather than one holder's slice.
    fn issues(&self) -> Result<Vec<Issue>, NodeError> {
        self.list_entries(RECORD)?
            .into_iter()
            .map(|name| {
                let id = id_in(&name)?;
                self.issue(&id)?.ok_or(NodeError::Damaged(name))
            })
            .collect()
    }

    /// The root's records for one space as the facts a host replays (laptop-host §2
    /// "Appointed hosts"): every issue at `Scope::Node` or inside `Scope::Workspace(ws_id)`,
    /// then every revocation. Whole, never since a cursor: [`replay`] is idempotent, so a
    /// resend costs a few KiB. Revocations are not filtered by space — the owner's cascade
    /// already ran, so each cascaded id is its own entry, and an id whose issue is not here
    /// revokes nothing.
    pub fn ledger(&self, ws_id: &str) -> Result<Vec<Entry>, NodeError> {
        let space = Scope::Workspace(ws_id.to_string());
        let mut entries = Vec::new();
        for issue in self.issues()? {
            let scope = issue.token.claims()?.scope;
            if scope == Scope::Node || space.contains(&scope) {
                entries.push(Entry::Issued(issue.token));
            }
        }
        for name in self.list_entries(REVOKED)? {
            let id = id_in(&name)?;
            let bytes = self
                .get_entry(&name)?
                .ok_or_else(|| NodeError::Damaged(name.clone()))?;
            let Revocation { at } = serde_json::from_slice(&bytes)?;
            entries.push(Entry::Revoked { id, at });
        }
        Ok(entries)
    }

    /// [`ledger`] signed as the root, entry by entry, for a `Frame::Ledger`. Built before
    /// signing, because `with_signer` holds the account for its closure. A hosted admin has
    /// no key to sign with ([`NodeError::NotOwner`]): the ledger flows down from the owner.
    pub fn signed_ledger(&self, ws_id: &str) -> Result<Vec<Signed>, NodeError> {
        let entries = self.ledger(ws_id)?;
        Ok(self.with_root_signer(|me| {
            entries
                .iter()
                .map(|entry| ledger::sign(me, entry))
                .collect::<Result<Vec<_>, _>>()
        })??)
    }

    /// One entry of the root's ledger into these records, once its signature is the root's.
    /// A bad signature is an error, never a skip: a ledger with one forged entry is not a
    /// ledger. Idempotent, and an issue already here is left as it is: replayed into the
    /// owner's own admin, that keeps the `Cause::Under` lineage a re-record would flatten to
    /// `Cause::Node`.
    pub fn replay(&self, signed: &Signed) -> Result<(), NodeError> {
        self.apply(ledger::verify(signed, &self.root()?)?)
    }

    /// A whole `Frame::Ledger`: every entry verified before any is applied, so one forged
    /// entry leaves nothing of the frame behind — not the grants before it with the
    /// revocations after it unapplied.
    pub fn replay_all(&self, entries: &[Signed]) -> Result<(), NodeError> {
        let root = self.root()?;
        let entries = entries
            .iter()
            .map(|signed| ledger::verify(signed, &root))
            .collect::<Result<Vec<_>, _>>()?;
        entries.into_iter().try_for_each(|entry| self.apply(entry))
    }

    fn apply(&self, entry: Entry) -> Result<(), NodeError> {
        match entry {
            Entry::Issued(token) => {
                if self.issue(&token.id())?.is_none() {
                    // Lineage is the owner's business: its cascade ran before any `Revoked`
                    // was signed, so this copy never needs to walk one.
                    self.record(&token, Cause::Node, token.claims()?.iat)?;
                }
            }
            Entry::Revoked { id, at } => self.mark_revoked(&id, at)?,
        }
        Ok(())
    }

    /// Appoint `did` a host of `ws_id`: a `host` token at exactly that space, the root's own
    /// decision, living [`HOST_TTL`]. Not delegable — a host serves, it does not appoint. The
    /// member learns of it by finding this token in the replayed ledger.
    pub fn appoint_host(&self, ws_id: &str, did: &str, now: u64) -> Result<Token, NodeError> {
        public_key_from_did(did).ok_or(CourierError::Decode)?;
        let space = Scope::Workspace(ws_id.to_string());
        let token = self.with_root_signer(|me| {
            issue_root(me, did, HOST, space, false, now, now + HOST_TTL)
        })??;
        self.record(&token, Cause::Node, now)?;
        Ok(token)
    }

    /// Whether `did` holds a live `host` grant at exactly `Scope::Workspace(ws_id)`.
    pub fn is_host(&self, did: &str, ws_id: &str, now: u64) -> Result<bool, NodeError> {
        Ok(self
            .grants(did, now)?
            .iter()
            .any(|g| appoints(&g.role, &g.scope, ws_id)))
    }

    /// Every DID holding a live `host` grant at exactly `Scope::Workspace(ws_id)`, sorted.
    /// Appointed hosts only — the owner is the root, not a holder. One pass over `token/`,
    /// the scan [`revoke`] already makes, rather than a per-space index nothing else reads:
    /// a space has a handful of hosts and this is asked once per session or change.
    pub fn hosts_of(&self, ws_id: &str, now: u64) -> Result<Vec<String>, NodeError> {
        let revoked = self.revoked()?;
        let mut hosts = BTreeSet::new();
        for issue in self.issues()? {
            let claims = issue.token.claims()?;
            if appoints(&claims.role, &claims.scope, ws_id)
                && !revoked.contains(&issue.token.id())
                && claims.exp > now
            {
                hosts.insert(claims.aud);
            }
        }
        Ok(hosts.into_iter().collect())
    }

    /// Keep where the appointed host `did` can be dialled for `ws_id`; the last word wins.
    pub fn set_host_addr(&self, ws_id: &str, did: &str, addr: &HostAddr) -> Result<(), NodeError> {
        self.put_entry(&host_addr_key(ws_id, did), &serde_json::to_vec(addr)?)
    }

    /// The address `did` last announced for `ws_id`, whether or not it is still a host.
    pub fn host_addr(&self, ws_id: &str, did: &str) -> Result<Option<HostAddr>, NodeError> {
        match self.get_entry(&host_addr_key(ws_id, did))? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Whether a token this node recorded is neither revoked nor expired. An unrecorded id is
    /// not live: nothing here can vouch for it.
    pub fn is_live(&self, id: &[u8; 32], now: u64) -> Result<bool, NodeError> {
        let Some(issue) = self.issue(id)? else {
            return Ok(false);
        };
        Ok(!self.revoked()?.contains(id) && issue.token.claims()?.exp > now)
    }

    /// Every token the node issued `did` that still counts: not revoked, not expired. This is
    /// the caller's authority for anything decided by role rather than by the token presented.
    pub fn grants(&self, did: &str, now: u64) -> Result<Vec<Grant>, NodeError> {
        self.grants_given(did, now, &self.revoked()?)
    }

    fn grants_given(
        &self,
        did: &str,
        now: u64,
        revoked: &HashSet<[u8; 32]>,
    ) -> Result<Vec<Grant>, NodeError> {
        let mut out = Vec::new();
        for issue in self.issued_to(did)? {
            let id = issue.token.id();
            if revoked.contains(&id) || issue.token.claims()?.exp <= now {
                continue;
            }
            out.push(Grant::of(&issue.token)?);
        }
        Ok(out)
    }

    /// The app's manifest, read from the source the node holds for `scope`'s item. A source
    /// without `manifest.osv` is bare: it declares no roles.
    pub fn manifest(&self, scope: &Scope) -> Result<Manifest, NodeError> {
        let Scope::App { ws, app } = scope else {
            return Err(CourierError::BadScope.into());
        };
        let name = format!("{ws}/{app}");
        let src = self
            .vault
            .get_src(ws, app)?
            .ok_or_else(|| NodeError::NoManifest(name.clone()))?;
        let doc = loro::LoroDoc::new();
        doc.import(&src)
            .map_err(|_| NodeError::BadSource(name.clone()))?;
        match doc.get_map("files").get("manifest.osv") {
            None => Ok(Manifest::default()),
            Some(loro::ValueOrContainer::Container(loro::Container::Text(text))) => {
                Ok(Manifest::parse(&text.to_string())?)
            }
            Some(_) => Err(NodeError::BadSource(name)),
        }
    }

    /// Hand out an app role (`courier::role`), recorded under the grant that allowed it.
    pub fn assign_role(&self, req: RoleRequest, now: u64) -> Result<Token, NodeError> {
        let manifest = self.manifest(&req.scope)?;
        let grants = self.grants(&req.desktop_did, now)?;
        let revoked = self.revoked()?;
        let (token, cause) = self.with_root_signer(|node| {
            courier::role::node_accept_assign(&req, node, &grants, &manifest, now, &revoked)
        })??;
        self.record(&token, Cause::Under(cause), now)?;
        Ok(token)
    }

    /// Take back every live grant of `req.role` at `req.scope` the node issued `req.to`, and
    /// everything issued under them. Returns how many direct grants were revoked.
    pub fn revoke_role(&self, req: RoleRequest, now: u64) -> Result<usize, NodeError> {
        let manifest = self.manifest(&req.scope)?;
        let grants = self.grants(&req.desktop_did, now)?;
        let node_did = self.root()?;
        let revoked = self.revoked()?;
        courier::role::node_accept_revoke(&req, &node_did, &grants, &manifest, now, &revoked)?;
        let mut n = 0;
        for g in self.grants(&req.to, now)? {
            if g.role == req.role && g.scope == req.scope {
                self.revoke(&g.id, now)?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Revoke `id` and, transitively, every token recorded `Cause::Under` it. Unknown ids are
    /// accepted: delegations are minted between holders and the node never sees one until it
    /// is presented, so revocation cannot require a record. Scans every
    /// issue record once per call; a sealed parent-to-children index is the fix if that grows.
    pub fn revoke(&self, id: &[u8; 32], at: u64) -> Result<(), NodeError> {
        let mut children: std::collections::HashMap<[u8; 32], Vec<[u8; 32]>> =
            std::collections::HashMap::new();
        for name in self.list_entries(RECORD)? {
            let child = id_in(&name)?;
            let issue = self.issue(&child)?.ok_or(NodeError::Damaged(name))?;
            if let Cause::Under(parent) = issue.cause {
                children.entry(parent).or_default().push(child);
            }
        }
        let mut seen = HashSet::new();
        let mut stack = vec![*id];
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            self.mark_revoked(&cur, at)?;
            stack.extend(children.get(&cur).into_iter().flatten());
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
        let node_did = self.root()?;
        courier::subscribe::node_accept_subscription(hello, &node_did, now, &self.revoked()?)?;
        self.put_entry(&subscription_key(hello), &[])?;
        Ok(())
    }

    /// Drop a subscription. Same authorization as `subscribe` — a holder unsubscribes with the
    /// same grant that would let them subscribe, nothing further to prove to remove themselves.
    pub fn unsubscribe(&self, hello: &SubscribeHello, now: u64) -> Result<(), NodeError> {
        let node_did = self.root()?;
        courier::subscribe::node_accept_subscription(hello, &node_did, now, &self.revoked()?)?;
        self.delete_entry(&subscription_key(hello))?;
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
        let node_did = self.root()?;
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
        // An app-scoped invite hands out an app role, so the role must be one the app declares.
        if matches!(request.scope, Scope::App { .. })
            && !self
                .manifest(&request.scope)?
                .can_grant(OWNER, &request.role)
        {
            return Err(CourierError::NotPermitted.into());
        }
        let revoked = self.revoked()?;
        let ticket = self.with_root_signer(|node| {
            courier::invite::issue_invite_ticket(node, request, name, now, &revoked)
        })??;
        let nonce = courier::invite::ticket_nonce(&ticket)?;
        let cause = request.token.root()?.id();
        self.put_entry(&invite_cause_key(&nonce), &cause)?;
        Ok(ticket)
    }

    /// A share link's code: a fresh public invite at `request`'s scope, kept under a random
    /// 12-byte code for [`CODE_TTL`] so a `Fetch { code }` can be answered with it. Public,
    /// because a link is redeemed by everyone it was shown to.
    pub fn issue_code(
        &self,
        request: &InviteRequest,
        name: &str,
        now: u64,
    ) -> Result<[u8; courier::link::CODE_LEN], NodeError> {
        let Scope::Workspace(ws_id) = &request.scope else {
            return Err(CourierError::NotPermitted.into());
        };
        let request = InviteRequest {
            public: true,
            ..request.clone()
        };
        let ticket = self.issue_invite(&request, name, now)?;
        let code = courier::link::new_code();
        let kept = Code {
            ticket,
            ws_id: ws_id.clone(),
            exp: now + CODE_TTL,
        };
        self.put_entry(&code_key(&code), &serde_json::to_vec(&kept)?)?;
        Ok(code)
    }

    /// The invite a code stands for and the space it opens; `None` for a code never issued
    /// or past its time. An expired code is forgotten on sight.
    pub fn fetch_code(
        &self,
        code: &[u8; courier::link::CODE_LEN],
        now: u64,
    ) -> Result<Option<(InviteTicket, String)>, NodeError> {
        let key = code_key(code);
        let Some(bytes) = self.get_entry(&key)? else {
            return Ok(None);
        };
        let kept: Code = serde_json::from_slice(&bytes)?;
        if now >= kept.exp {
            self.delete_entry(&key)?;
            return Ok(None);
        }
        Ok(Some((kept.ticket, kept.ws_id)))
    }

    /// Every nonce this node has already redeemed an invite for. Read whole, same reasoning as
    /// `revoked()`: a per-nonce check would be a decrypt per nonce on every redemption attempt.
    pub fn redeemed_invites(&self) -> Result<HashSet<String>, NodeError> {
        Ok(self
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
        let cause = self.invite_cause(&hello)?;
        // The cascade already ran for a revoked inviter; a child minted now would escape it.
        if let Cause::Under(parent) = cause {
            if !self.is_live(&parent, now)? {
                return Err(CourierError::Revoked.into());
            }
        }
        let welcome = self.with_root_signer(|node| {
            courier::invite::node_accept_invite(hello, node, now, &redeemed)
        })??;
        if let Some(spent) = &welcome.redeemed_nonce {
            self.put_entry(&invite_key(spent), &[])?;
        }
        self.record(&welcome.token, cause, now)?;
        Ok(welcome)
    }

    /// Under the grant of whoever asked for the invite. A ticket with no recorded inviter
    /// predates that record and is refused: redeeming it would mint a grant no revocation of
    /// its inviter could reach.
    fn invite_cause(&self, hello: &InviteClaimHello) -> Result<Cause, NodeError> {
        let nonce = courier::invite::ticket_nonce(&hello.ticket)?;
        let key = invite_cause_key(&nonce);
        let bytes = self.get_entry(&key)?.ok_or(NodeError::UntrackedInvite)?;
        Ok(Cause::Under(
            bytes.try_into().map_err(|_| NodeError::Damaged(key))?,
        ))
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
        let presented = hello.token.root()?.id();
        let token = self.with_root_signer(|node| {
            courier::node_accept_reconnect(hello, node, &admins, challenges, now, &revoked)
        })??;
        // A reissue is an issuance, so it joins the log. That makes the log grow by one per
        // reconnect and leaves the superseded token listed as well; superseding is in the
        // backlog, and under-reporting what is live would be the worse of the two. It is
        // recorded under the token it replaces, so revoking that one takes the reissue too.
        self.record(&token, Cause::Under(presented), now)?;
        Ok(token)
    }

    /// One id into the revoked set, no cascade. Two callers: [`Admin::revoke`], which walks the
    /// lineage first, and [`Admin::replay`], for which the owner's walk already ran and shipped
    /// each id — so no revocation of this root's own making skips the walk.
    fn mark_revoked(&self, id: &[u8; 32], at: u64) -> Result<(), NodeError> {
        self.put_entry(&revoked_key(id), &serde_json::to_vec(&Revocation { at })?)?;
        Ok(())
    }

    /// What *this* node revoked, which is the set its own chain check consumes — a chain
    /// rooted elsewhere is answered by that node's set, under `nodes/`, not by this one.
    /// Read whole, because a check that asked per link would be a decrypt per link on every
    /// request. An unreadable name fails the whole read rather than returning a set that is
    /// quietly missing a revocation.
    pub fn revoked(&self) -> Result<HashSet<[u8; 32]>, NodeError> {
        self.list_entries(REVOKED)?
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
fn code_key(code: &[u8; courier::link::CODE_LEN]) -> String {
    format!("{CODES}{}", URL_SAFE_NO_PAD.encode(code))
}

fn invite_key(nonce: &str) -> String {
    format!("{INVITES}{nonce}")
}

fn invite_cause_key(nonce: &str) -> String {
    format!("{INVITE_CAUSES}{nonce}")
}

/// `ws_id` is one this admin hosts (the acceptor admitted the session on it) and a DID
/// carries no `/` — the trust `relationship_key` already places in one.
fn host_addr_key(ws_id: &str, did: &str) -> String {
    format!("{HOST_ADDRS}{ws_id}/{did}")
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

/// A `host` grant at exactly `Scope::Workspace(ws_id)` — not a node-wide one, which no path
/// mints, and not one on another space.
fn appoints(role: &str, scope: &Scope, ws_id: &str) -> bool {
    role == HOST && matches!(scope, Scope::Workspace(ws) if ws == ws_id)
}

/// Bytes Loro would not take, whatever it said about them.
fn undecodable<E>(_: E) -> NodeError {
    NodeError::Courier(CourierError::Decode)
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

#[cfg(test)]
mod access_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod roles_tests;
