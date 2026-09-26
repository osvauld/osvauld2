//! Subscribe: a desktop declaring interest in an item's layer, so the node knows who to push
//! future changes to once real fanout exists. Authorization only — courier stores nothing;
//! `kunki::admin` is what persists or drops the record. One `SubscribeHello` shape serves both
//! directions (subscribe and unsubscribe carry the same fields), because the only thing that
//! differs between them is which action the caller takes with an already-proven request — not
//! anything about what needs proving.
//!
//! Same authorization as sync: [`policy::membership`], workspace membership alone, no platform
//! capability. Explicit by design (not implicit from sync history) — decided so a subscription
//! is a fact someone chose to create, not a side effect of an unrelated call.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::policy;
use crate::sync::SyncLayer;
use crate::token::{Scope, Token};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeHello {
    pub desktop_did: String,
    pub token: Token,
    pub ws_id: String,
    pub item_id: String,
    pub layer: SyncLayer,
}

/// `token`/`desktop_did` are the desktop's own claim/reconnect grant, presented as-is — same
/// reasoning as `desktop_publish`: building this message signs nothing.
pub fn desktop_start_subscribe(
    desktop_did: &str,
    token: Token,
    ws_id: &str,
    item_id: &str,
    layer: SyncLayer,
) -> SubscribeHello {
    SubscribeHello {
        desktop_did: desktop_did.to_string(),
        token,
        ws_id: ws_id.to_string(),
        item_id: item_id.to_string(),
        layer,
    }
}

/// Prove `hello` is a legitimate request to (un)subscribe. Which action it is is the caller's
/// business, not this function's — a subscribe and an unsubscribe from the same holder for the
/// same layer are equally authorized, so there is nothing here for the two to differ on.
pub fn node_accept_subscription(
    hello: &SubscribeHello,
    node_did: &str,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<()> {
    policy::membership(
        &hello.token,
        node_did,
        &hello.desktop_did,
        &Scope::Workspace(hello.ws_id.clone()),
        now,
        revoked,
    )?;
    Ok(())
}

/// Prove a `Listen` connection is legitimate: the same membership check as above, but at
/// `Scope::Node` rather than one workspace — a listen connection is not scoped to a single
/// item's layer, since a desktop may hold subscriptions across several workspaces on the same
/// node and all of them arrive over the one connection.
pub fn node_accept_listen(
    token: &Token,
    node_did: &str,
    desktop_did: &str,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<()> {
    policy::membership(token, node_did, desktop_did, &Scope::Node, now, revoked)?;
    Ok(())
}

#[cfg(test)]
mod tests;
