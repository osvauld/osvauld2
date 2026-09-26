//! What a role is allowed to do. Tokens carry role names and nothing else; this is the table
//! that reads them. Platform capabilities are a closed set fixed here — an app manifest may
//! declare its own capabilities but can never declare one of these.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use workspace::ResourceScope;

use crate::token::{Claims, Scope, Token, verify_chain};
use crate::{CourierError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    WorkspaceCreate,
    WorkspaceDelete,
    AppInstall,
    AppRemove,
    NamespaceDeclare,
    PolicyPublish,
    MemberInvite,
    RoleAssign,
}

use Capability::*;

/// Platform roles are read against the level they are held at: `owner` at node level runs the
/// node, `owner` at workspace level runs one workspace. An unrecognised role — including every
/// app role — grants no platform capability, so a manifest cannot reach these by naming one.
pub fn platform_capabilities(role: &str, scope: &Scope) -> &'static [Capability] {
    match (scope, role) {
        (Scope::Node, "owner") => &[
            WorkspaceCreate,
            WorkspaceDelete,
            AppInstall,
            AppRemove,
            NamespaceDeclare,
            PolicyPublish,
            MemberInvite,
            RoleAssign,
        ],
        (Scope::Node, "admin") => &[WorkspaceCreate, RoleAssign],
        (Scope::Workspace(_), "owner") => &[
            WorkspaceDelete,
            AppInstall,
            AppRemove,
            NamespaceDeclare,
            PolicyPublish,
            MemberInvite,
            RoleAssign,
        ],
        // A maintainer assigns roles so app roles can be handed out. Ranking an assignment
        // against the assigner's own role belongs to `role.assign`, not to this table.
        (Scope::Workspace(_), "maintainer") => &[
            AppInstall,
            AppRemove,
            NamespaceDeclare,
            PolicyPublish,
            MemberInvite,
            RoleAssign,
        ],
        // member, guest, every app role, and every typo land here.
        _ => &[],
    }
}

/// A target names something the caller supplies, so its syntax is checked before it is
/// compared against anything. `Scope::Node` contains every other variant, so without this a
/// node-scoped role would act on an address that no narrower role would even parse.
fn well_formed(scope: &Scope) -> bool {
    match scope {
        Scope::Node => true,
        Scope::Workspace(ws) => workspace::valid_id(ws),
        Scope::App { ws, app } => workspace::valid_id(ws) && workspace::valid_id(app),
        Scope::Resource(address) => ResourceScope::parse(address).is_ok(),
    }
}

/// The chain verifies and the role's scope covers what is being acted on — the platform half
/// of a decision minus the capability lookup, for callers gated by workspace membership alone
/// rather than a specific platform action. Sync is the first of these: every role in a
/// workspace may read and write its content, and no manifest/rule layer exists yet to say
/// otherwise, so there is nothing for the capability table to check.
pub fn membership(
    leaf: &Token,
    node_did: &str,
    holder: &str,
    target: &Scope,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<Claims> {
    if !well_formed(target) {
        return Err(CourierError::BadScope);
    }
    let claims = verify_chain(leaf, node_did, holder, now, revoked)?;
    if !claims.scope.contains(target) {
        return Err(CourierError::OutOfScope);
    }
    Ok(claims)
}

/// [`membership`] plus the capability lookup. Manifest rules run after this, over facts it
/// has already proven.
pub fn authorize(
    leaf: &Token,
    node_did: &str,
    holder: &str,
    capability: Capability,
    target: &Scope,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<Claims> {
    let claims = membership(leaf, node_did, holder, target, now, revoked)?;
    if !platform_capabilities(&claims.role, &claims.scope).contains(&capability) {
        return Err(CourierError::NotPermitted);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests;
