//! Publish: a desktop announcing a workspace to its node. Headers only — content follows over
//! sync (`docs/design/osvauld1-prior-art.md` §1). The workspace and its items travel as a flat
//! list projected for the wire, not `vault`'s local `WorkspaceMeta`/`WorkspaceItem` records —
//! independent of how either side stores them, same reasoning as `ConnectionTicket`'s own text
//! form. `vault::ItemKind::as_str` is the caller's projection from the local enum to `kind`.
//!
//! Creating a workspace on the node is `Capability::WorkspaceCreate`, held by `owner` and
//! `admin` at `Scope::Node` — the same node-scope grant claim already hands the first admin,
//! and `role.assign` will hand later ones. So the token presented here is the desktop's own
//! claim/reconnect token, unnarrowed: narrowing it to the workspace being created would drop
//! it out of `Scope::Node`, the only level `WorkspaceCreate` is ever granted at.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::policy::{self, Capability};
use crate::token::{Scope, Token};
use crate::{Result, nonce};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedWorkspace {
    pub id: String,
    pub name: String,
    pub created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedItem {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub created: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishHello {
    pub request_id: String,
    /// Who `token` is held by — `verify_chain` needs this named up front rather than assumed,
    /// the same reason `ReconnectHello` carries it.
    pub desktop_did: String,
    pub workspace: PublishedWorkspace,
    pub items: Vec<PublishedItem>,
    pub token: Token,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishAck {
    pub request_id: String,
    /// Item ids the node already held before this publish, so a caller can send only the
    /// difference on the next one (`docs/design/osvauld1-prior-art.md` §1).
    pub known_items: Vec<String>,
}

/// `token` is the desktop's own node-rooted grant (role `owner` or `admin`, `Scope::Node`) from
/// its claim or latest reconnect, presented as-is — building this message signs nothing.
pub fn desktop_publish(
    desktop_did: &str,
    token: Token,
    workspace: PublishedWorkspace,
    items: Vec<PublishedItem>,
) -> PublishHello {
    PublishHello {
        request_id: nonce(),
        desktop_did: desktop_did.to_string(),
        workspace,
        items,
        token,
    }
}

/// `known_items` and `revoked` come from the caller — courier never touches storage — the
/// former so the ack can name what the node already has, the latter so a claim revoked since
/// it was issued cannot still publish.
pub fn node_accept_publish(
    hello: &PublishHello,
    node_did: &str,
    known_items: Vec<String>,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<PublishAck> {
    policy::authorize(
        &hello.token,
        node_did,
        &hello.desktop_did,
        Capability::WorkspaceCreate,
        &Scope::Workspace(hello.workspace.id.clone()),
        now,
        revoked,
    )?;
    Ok(PublishAck {
        request_id: hello.request_id.clone(),
        known_items,
    })
}

#[cfg(test)]
mod tests;
