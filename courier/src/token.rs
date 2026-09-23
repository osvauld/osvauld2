//! Role tokens: a node-rooted delegation chain. Each link carries its full parent inside the
//! signed payload, so a holder presents one value and the node checks it without lookups.

use std::collections::HashSet;

use identity::{Signer, public_key_from_did, verify};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use workspace::ResourceScope;

use crate::{CourierError, Result, nonce};

const TOKEN_DOMAIN: &[u8] = b"osvauld/courier/token/v1\0";
/// Chains stay short because the node reissues flattened tokens.
pub const MAX_CHAIN: usize = 8;

/// The level a role name is read against: platform roles at node and workspace level, manifest
/// roles at app level, and one-off shares of specific data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Node,
    Workspace(String),
    App { ws: String, app: String },
    Resource(String),
}

impl Scope {
    /// Whether a role held at this scope reaches `other`. Levels nest downward and never
    /// sideways; an app scope stops there, because its namespaces are bound at install.
    pub fn contains(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Node, _) => true,
            (Self::Workspace(ws), Self::Workspace(other)) => ws == other,
            (Self::Workspace(ws), Self::App { ws: other, .. }) => ws == other,
            (Self::Workspace(ws), Self::Resource(scope)) => {
                ResourceScope::parse(scope).is_ok_and(|scope| scope.workspace_id() == ws)
            }
            (
                Self::App { ws, app },
                Self::App {
                    ws: o_ws,
                    app: o_app,
                },
            ) => ws == o_ws && app == o_app,
            // An app token does not reach into addresses: its namespaces are bound at install.
            (Self::Resource(scope), Self::Resource(other)) => {
                match (ResourceScope::parse(scope), ResourceScope::parse(other)) {
                    (Ok(scope), Ok(other)) => scope.contains(&other),
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    payload: Vec<u8>,
    sig: Vec<u8>,
}

/// Public key material a token is *about*, as distinct from anything it permits. It sits
/// inside the signed payload because a binding carried beside a signature proves nothing,
/// and [`verify_chain`] ignores it: only the acceptor knows what it should equal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub encryption: String,
    pub device: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub role: String,
    pub scope: Scope,
    pub delegable: bool,
    pub nonce: String,
    pub iat: u64,
    pub exp: u64,
    pub prf: Option<Token>,
    /// Set only by [`attest`]; `None` on every token that grants rather than describes.
    pub binds: Option<KeyBinding>,
}

/// The role an attestation carries. Deliberately absent from
/// [`platform_capabilities`](crate::policy::platform_capabilities), so a token that only
/// describes key material cannot be read as authority by any table lookup. When a holder's
/// node is genuinely allowed to act for them, that is a delegation with a real role, not this.
const ATTEST_ROLE: &str = "relationship";

impl Token {
    /// Hash of the signed payload, not the signature: revocation names the grant, whatever
    /// bytes its signature arrived as.
    pub fn id(&self) -> [u8; 32] {
        Sha256::digest(&self.payload).into()
    }

    pub fn claims(&self) -> Result<Claims> {
        bincode::deserialize(&self.payload).map_err(|_| CourierError::Decode)
    }
}

pub fn issue_root(
    node: &(impl Signer + ?Sized),
    aud: &str,
    role: &str,
    scope: Scope,
    delegable: bool,
    now: u64,
    exp: u64,
) -> Result<Token> {
    sign(
        node,
        Claims {
            iss: node.did().to_string(),
            aud: aud.to_string(),
            sub: node.did().to_string(),
            role: role.to_string(),
            scope,
            delegable,
            nonce: nonce(),
            iat: now,
            exp,
            prf: None,
            binds: None,
        },
    )
}

/// A root token whose point is the key material it carries, not a capability. The issuer is
/// its own subject, so it verifies against itself as the root; `aud` is who it is shown to.
///
/// Not delegable: nothing should chain off a statement about keys.
pub fn attest(
    holder: &(impl Signer + ?Sized),
    aud: &str,
    binds: KeyBinding,
    now: u64,
    exp: u64,
) -> Result<Token> {
    sign(
        holder,
        Claims {
            iss: holder.did().to_string(),
            aud: aud.to_string(),
            sub: holder.did().to_string(),
            role: ATTEST_ROLE.to_string(),
            scope: Scope::Node,
            delegable: false,
            nonce: nonce(),
            iat: now,
            exp,
            prf: None,
            binds: Some(binds),
        },
    )
}

/// Unchecked on purpose: the node's chain check is the only authority, so an invalid
/// delegation is minted here and refused there.
pub fn delegate(
    parent: &Token,
    holder: &(impl Signer + ?Sized),
    aud: &str,
    scope: Scope,
    delegable: bool,
    now: u64,
    exp: u64,
) -> Result<Token> {
    let parent_claims = parent.claims()?;
    sign(
        holder,
        Claims {
            iss: holder.did().to_string(),
            aud: aud.to_string(),
            sub: parent_claims.sub,
            role: parent_claims.role,
            scope,
            delegable,
            nonce: nonce(),
            iat: now,
            exp,
            prf: Some(parent.clone()),
            binds: None,
        },
    )
}

/// Walks leaf to root: every link signed by its issuer, rooted at `root_did`, current, and
/// never wider than its parent. Returns the leaf's claims, whose `prf` the walk consumed.
///
/// `root_did` is whichever authority the caller expects this chain to trace back to, not the
/// sovereign node by definition — a claimant's own attestation roots at the claimant, and
/// under federation a token from another node roots at that node.
pub fn verify_chain(
    leaf: &Token,
    root_did: &str,
    holder: &str,
    now: u64,
    revoked: &HashSet<[u8; 32]>,
) -> Result<Claims> {
    let mut links = Vec::new();
    let mut next = Some(leaf.clone());
    while let Some(token) = next {
        // Depth first, before any signature work: an arbitrarily long chain is free to send.
        if links.len() == MAX_CHAIN {
            return Err(CourierError::ChainTooLong);
        }
        let mut claims = token.claims()?;
        next = claims.prf.take();
        links.push((token, claims));
    }

    for (token, claims) in &links {
        let sig: [u8; 64] = token
            .sig
            .as_slice()
            .try_into()
            .map_err(|_| CourierError::Decode)?;
        let issuer = public_key_from_did(&claims.iss).ok_or(CourierError::Decode)?;
        if !verify(&issuer, &[TOKEN_DOMAIN, &token.payload].concat(), &sig) {
            return Err(CourierError::BadSignature);
        }
        if claims.sub != root_did {
            return Err(CourierError::NodeMismatch);
        }
        if now >= claims.exp {
            return Err(CourierError::Expired);
        }
        if revoked.contains(&token.id()) {
            return Err(CourierError::Revoked);
        }
    }

    for pair in links.windows(2) {
        let (child, parent) = (&pair[0].1, &pair[1].1);
        if child.iss != parent.aud {
            return Err(CourierError::BrokenChain);
        }
        if !parent.delegable {
            return Err(CourierError::NotDelegable);
        }
        if child.role != parent.role || !parent.scope.contains(&child.scope) {
            return Err(CourierError::Escalation);
        }
    }

    let root = &links.last().expect("the leaf is always pushed").1;
    if root.iss != root.sub {
        return Err(CourierError::BrokenChain);
    }
    let leaf = &links[0].1;
    if leaf.aud != holder {
        return Err(CourierError::WrongHolder);
    }
    Ok(leaf.clone())
}

fn sign(issuer: &(impl Signer + ?Sized), claims: Claims) -> Result<Token> {
    let payload = bincode::serialize(&claims).map_err(|_| CourierError::Decode)?;
    Ok(Token {
        sig: issuer.sign(&[TOKEN_DOMAIN, &payload].concat()).to_vec(),
        payload,
    })
}

#[cfg(test)]
mod tests;
