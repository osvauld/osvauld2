//! `role.assign`: handing out an app role (`docs/design/app-permissions.md` §2).
//!
//! Authority is the caller's **grants** — every live token the node issued them, read from the
//! node's records — not the one token a request happens to present. That token only proves the
//! caller is a member who reaches the target. A grant may assign:
//! - any declared app role, if its platform role carries `RoleAssign` over the target
//!   (node owner/admin, workspace owner/maintainer) — the manifest's implicit `owner`;
//! - otherwise only roles in its own app role's grant cone, at that same app.
//!
//! The node mints a flat, non-delegable token at `Scope::App` and the caller records which
//! grant allowed it, so revoking that grant takes this one with it.

use std::collections::HashSet;

use identity::{Signer, public_key_from_did};
use manifest::{Manifest, OWNER};
use serde::{Deserialize, Serialize};

use crate::policy::{self, Capability};
use crate::token::{Scope, Token, issue_root};
use crate::{CLAIM_TTL, CourierError, Result};

/// One live token the node issued, as authority: what it is and where it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: [u8; 32],
    pub role: String,
    pub scope: Scope,
}

impl Grant {
    pub fn of(token: &Token) -> Result<Self> {
        let claims = token.claims()?;
        Ok(Self {
            id: token.id(),
            role: claims.role,
            scope: claims.scope,
        })
    }
}

/// Assign `role` to `to` at `scope`, or (to [`node_accept_revoke`]) take it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleRequest {
    pub desktop_did: String,
    pub token: Token,
    pub to: String,
    pub role: String,
    pub scope: Scope,
}

/// The first of `grants` that may give `role` at `target`.
pub fn assigning_grant<'a>(
    grants: &'a [Grant],
    role: &str,
    target: &Scope,
    manifest: &Manifest,
) -> Option<&'a Grant> {
    grants.iter().find(|g| {
        if !g.scope.contains(target) {
            return false;
        }
        let platform = policy::platform_capabilities(&g.role, &g.scope);
        if platform.contains(&Capability::RoleAssign) {
            return manifest.can_grant(OWNER, role);
        }
        // An app role reaches only its own app: `contains` already said so.
        matches!(g.scope, Scope::App { .. }) && manifest.can_grant(&g.role, role)
    })
}

/// The checks both directions share. Returns the grant that authorizes the change.
fn authorize<'a>(
    req: &RoleRequest,
    node_did: &str,
    grants: &'a [Grant],
    manifest: &Manifest,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<&'a Grant> {
    if !matches!(req.scope, Scope::App { .. }) {
        return Err(CourierError::BadScope);
    }
    public_key_from_did(&req.to).ok_or(CourierError::Decode)?;
    policy::membership(
        &req.token,
        node_did,
        &req.desktop_did,
        &req.scope,
        now,
        revoked,
    )?;
    assigning_grant(grants, &req.role, &req.scope, manifest).ok_or(CourierError::NotPermitted)
}

/// Mint the assignee's token. Returns it with the id of the grant that allowed it, for the
/// caller to record as its cause.
pub fn node_accept_assign(
    req: &RoleRequest,
    node: &(impl Signer + ?Sized),
    grants: &[Grant],
    manifest: &Manifest,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<(Token, [u8; 32])> {
    let cause = authorize(req, node.did(), grants, manifest, now, revoked)?.id;
    let token = issue_root(
        node,
        &req.to,
        &req.role,
        req.scope.clone(),
        false,
        now,
        now + CLAIM_TTL,
    )?;
    Ok((token, cause))
}

/// Whoever could assign a role may take it back. Which tokens to revoke is the caller's
/// lookup: courier holds no records.
pub fn node_accept_revoke(
    req: &RoleRequest,
    node_did: &str,
    grants: &[Grant],
    manifest: &Manifest,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<()> {
    authorize(req, node_did, grants, manifest, now, revoked).map(|_| ())
}

#[cfg(test)]
mod tests;
