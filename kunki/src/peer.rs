//! What this account holds *from* other nodes.
//!
//! The mirror of [`admin`](crate::admin), and deliberately not the same code. There this
//! account is the authority: an issue is appended and never replaced, because losing one
//! loses the audit log. Here it is the subject — a record is only whatever the issuer last
//! said, so a reclaim or a reissued permit overwrites it, and losing one costs a reconnect
//! rather than a fact.
//!
//! `nodes/<node-did>/relationship` is exactly what `courier::desktop_finish_claim` returned
//! and nothing more: no capability is decided from it, it is the ticket stub that lets this
//! account reconnect and present its permit. A user with no node of their own joins several
//! nodes directly and keeps one per node; a node keeps one per node it federates with. Both
//! read the same, which is why this is not a desktop-only module.

use courier::DesktopNodeRecord;
use vault::Vault;

use crate::NodeError;

const NODES: &str = "nodes/";
// The same suffix `admin` uses under `users/`, for the same reason: it leaves room for
// siblings under `nodes/<did>/` that the listing scan must not pick up.
const RELATIONSHIP: &str = "/relationship";

/// The relationships this account holds, over an unlocked account. Cloning is cheap and
/// shares the account.
#[derive(Clone)]
pub struct Peer {
    vault: Vault,
}

impl Peer {
    pub fn new(vault: Vault) -> Self {
        Self { vault }
    }

    /// Keep what a finished claim returned. Overwrites: claiming the same node again is a
    /// fresh permit for the same relationship, not a second one.
    pub fn record_node(&self, record: &DesktopNodeRecord) -> Result<(), NodeError> {
        self.vault.put_entry(
            &relationship_key(&record.node_did),
            &serde_json::to_vec(record)?,
        )?;
        Ok(())
    }

    /// The relationship with one node, or `None` if this account never claimed it. Absent is
    /// not an error — asking about a stranger is how you find out they are one.
    pub fn node(&self, node_did: &str) -> Result<Option<DesktopNodeRecord>, NodeError> {
        let Some(bytes) = self.vault.get_entry(&relationship_key(node_did))? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    /// Every node this account has claimed. Unordered: nothing here ranks them, and the one
    /// a caller wants is named by DID.
    pub fn nodes(&self) -> Result<Vec<DesktopNodeRecord>, NodeError> {
        let mut nodes = Vec::new();
        for name in self.vault.list_entries(NODES)? {
            if !name.ends_with(RELATIONSHIP) {
                continue;
            }
            let bytes = self
                .vault
                .get_entry(&name)?
                .ok_or_else(|| NodeError::Damaged(name.clone()))?;
            nodes.push(serde_json::from_slice(&bytes)?);
        }
        Ok(nodes)
    }
}

/// `nodes/<node-did>/relationship` — one per node claimed, found by scanning `nodes/` for
/// the suffix. A DID carries no `/`, so the node's own segment cannot forge the shape.
fn relationship_key(node_did: &str) -> String {
    format!("{NODES}{node_did}{RELATIONSHIP}")
}

#[cfg(test)]
mod tests;
