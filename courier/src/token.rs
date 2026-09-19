//! Role tokens: a node-rooted delegation chain. Each link carries its full parent inside the
//! signed payload, so a holder presents one value and the node checks it without lookups.

use identity::Identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CourierError, Result, nonce};

const TOKEN_DOMAIN: &[u8] = b"osvauld/courier/token/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    payload: Vec<u8>,
    sig: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub role: String,
    pub scope: String,
    pub delegable: bool,
    pub nonce: String,
    pub iat: u64,
    pub exp: u64,
    pub prf: Option<Token>,
}

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
    node: &Identity,
    aud: &str,
    role: &str,
    scope: &str,
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
            scope: scope.to_string(),
            delegable,
            nonce: nonce(),
            iat: now,
            exp,
            prf: None,
        },
    )
}

/// Unchecked on purpose: the node's chain check is the only authority, so an invalid
/// delegation is minted here and refused there.
pub fn delegate(
    parent: &Token,
    holder: &Identity,
    aud: &str,
    scope: &str,
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
            scope: scope.to_string(),
            delegable,
            nonce: nonce(),
            iat: now,
            exp,
            prf: Some(parent.clone()),
        },
    )
}

fn sign(issuer: &Identity, claims: Claims) -> Result<Token> {
    let payload = bincode::serialize(&claims).map_err(|_| CourierError::Decode)?;
    Ok(Token {
        sig: issuer.sign(&[TOKEN_DOMAIN, &payload].concat()).to_vec(),
        payload,
    })
}

#[cfg(test)]
mod tests;
