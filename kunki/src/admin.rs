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
use courier::token::Token;
use courier::{AdminRecord, ClaimHello, ClaimWelcome, ReconnectHello};
use serde::{Deserialize, Serialize};
use vault::Vault;

use crate::NodeError;

const RECORD: &str = "token/";
const USERS: &str = "users/";
const REVOKED: &str = "revoked/";
const RELATIONSHIP: &str = "/relationship";

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

fn name_of(id: &[u8; 32]) -> String {
    URL_SAFE_NO_PAD.encode(id)
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
