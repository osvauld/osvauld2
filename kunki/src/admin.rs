//! What the node handed out and what it took back.
//!
//! Sealed `vault` entries, not a CRDT: the node is the sole writer here, and a revocation
//! that could merge away is not a revocation. Keys are plaintext by construction — a holder's
//! DID names its index — which adds nothing, since `vault` already names an account's file
//! after its DID.

use std::collections::HashSet;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use courier::token::Token;
use serde::{Deserialize, Serialize};
use vault::Vault;

use crate::NodeError;

const RECORD: &str = "token/";
const HOLDER: &str = "holder/";
const REVOKED: &str = "revoked/";

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
        self.vault.put_entry(&holder_key(&holder, &id), &[])?;
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
        for name in self.vault.list_entries(&holder_scan(holder))? {
            let id = id_in(&name)?;
            // The record is written first, so an index without one is a damaged store, not a
            // race. An audit log that quietly under-reports is the worse failure.
            issues.push(self.issue(&id)?.ok_or(NodeError::Damaged(name))?);
        }
        Ok(issues)
    }

    /// Unknown ids are accepted: delegations are minted between holders and the node never
    /// sees one until it is presented, so revocation cannot require a record.
    pub fn revoke(&self, id: &[u8; 32], at: u64) -> Result<(), NodeError> {
        self.vault
            .put_entry(&revoked_key(id), &serde_json::to_vec(&Revocation { at })?)?;
        Ok(())
    }

    /// The set the chain check consumes. Read whole, because a check that asked per link
    /// would be a decrypt per link on every request. An unreadable name fails the whole
    /// read rather than returning a set that is quietly missing a revocation.
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

fn holder_scan(holder: &str) -> String {
    format!("{HOLDER}{holder}/")
}

fn holder_key(holder: &str, id: &[u8; 32]) -> String {
    format!("{}{}", holder_scan(holder), name_of(id))
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
