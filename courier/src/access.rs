//! Who may read and write one doc: the manifest's declarative rules
//! (`docs/design/group-chat-sync.md` §4, `app-permissions.md` §3).
//!
//! A rule names a role, a DID path variable, or `members(<doc>)`. Roles come from the caller's
//! grants on the node's records, never the presented token; `members(...)` reads the node's
//! own copy of that doc. Pure like the rest of courier: the node supplies both.

use std::collections::BTreeSet;

use loro::{LoroDoc, LoroValue};
use manifest::{Manifest, OWNER, Who};
use workspace::ResourceAddress;

use crate::policy::{self, Capability};
use crate::role::Grant;
use crate::sync::item_scope;
use crate::token::Scope;
use crate::{CourierError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Access {
    pub read: bool,
    pub write: bool,
}

impl Access {
    pub const OPEN: Self = Self {
        read: true,
        write: true,
    };
}

/// A doc's resource address, `ws/<ws>/<item>/<doc>`. `None` for anything that isn't a doc name
/// (`workspace::valid_doc_name`) under real ids — storage refuses those too.
pub fn doc_address(ws_id: &str, item_id: &str, doc: &str) -> Option<String> {
    if !workspace::valid_doc_name(doc) {
        return None;
    }
    let address = format!("ws/{ws_id}/{item_id}/{doc}");
    ResourceAddress::parse(&address).ok().map(|_| address)
}

/// The app roles `grants` hold at one doc of an item:
/// - an app grant on this item: its role;
/// - a node or workspace grant over it: `owner` if it may assign roles there (owner,
///   maintainer, node admin), `member` for a platform member, nothing for a guest;
/// - a resource grant covering the doc's address: its role, at that doc only.
pub fn roles_at(grants: &[Grant], ws_id: &str, item_id: &str, doc: &str) -> BTreeSet<String> {
    let item = item_scope(ws_id, item_id);
    let address = doc_address(ws_id, item_id, doc).map(Scope::Resource);
    let mut roles = BTreeSet::new();
    for g in grants {
        match &g.scope {
            Scope::App { .. } if g.scope.contains(&item) => {
                roles.insert(g.role.clone());
            }
            Scope::Node | Scope::Workspace(_) if g.scope.contains(&item) => {
                if policy::platform_capabilities(&g.role, &g.scope).contains(&Capability::RoleAssign)
                {
                    roles.insert(OWNER.to_string());
                } else if g.role == "member" {
                    roles.insert(g.role.clone());
                }
            }
            Scope::Resource(_) if address.as_ref().is_some_and(|a| g.scope.contains(a)) => {
                roles.insert(g.role.clone());
            }
            _ => {}
        }
    }
    roles
}

/// `caller`'s access to `doc`. An app that declares no docs leaves every doc to membership
/// alone; one that declares any refuses the rest. `members` returns who the node's copy of a
/// doc lists, for `members(...)` rules.
pub fn doc_access<E: From<CourierError>>(
    manifest: &Manifest,
    doc: &str,
    caller: &str,
    roles: &BTreeSet<String>,
    members: &mut dyn FnMut(&str) -> std::result::Result<BTreeSet<String>, E>,
) -> std::result::Result<Access, E> {
    if manifest.docs.is_empty() {
        return Ok(Access::OPEN);
    }
    let (decl, vars) = manifest.resolve(doc).ok_or(CourierError::Undeclared)?;
    let mut admits = |rule: &[Who]| -> std::result::Result<bool, E> {
        for who in rule {
            let yes = match who {
                Who::Role(r) => roles.iter().any(|held| manifest.satisfies(held, r)),
                Who::Var(v) => vars.get(v).is_some_and(|did| did == caller),
                Who::Node => false,
                Who::Members(pattern) => {
                    let name = pattern.fill(&vars).ok_or(CourierError::Decode)?;
                    members(&name)?.contains(caller)
                }
            };
            if yes {
                return Ok(true);
            }
        }
        Ok(false)
    };
    Ok(Access {
        read: admits(&decl.read)?,
        write: admits(&decl.write)?,
    })
}

/// Who a membership doc lists: the DIDs its root `members` map sets to `true`. Any other
/// shape lists nobody, so a malformed doc admits no one rather than everyone.
pub fn members_in(snapshot: Option<&[u8]>) -> Result<BTreeSet<String>> {
    let Some(bytes) = snapshot else {
        return Ok(BTreeSet::new());
    };
    let doc = LoroDoc::new();
    doc.import(bytes).map_err(|_| CourierError::Decode)?;
    let LoroValue::Map(root) = doc.get_deep_value() else {
        return Ok(BTreeSet::new());
    };
    let Some(LoroValue::Map(members)) = root.get("members") else {
        return Ok(BTreeSet::new());
    };
    Ok(members
        .iter()
        .filter(|(_, v)| matches!(v, LoroValue::Bool(true)))
        .map(|(did, _)| did.to_string())
        .collect())
}

#[cfg(test)]
mod tests;
