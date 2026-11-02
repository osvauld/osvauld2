//! The owner's admin records as signed facts (`laptop-host.md` §2 "Appointed hosts"): one
//! entry per token issued and per revocation, each signed by the owner's DID key, so an
//! appointed host judges members on the owner's records and never on its own say-so. Pure
//! encoding and signature; what an entry is replayed into is `kunki::admin`'s business.

use identity::{Signer, public_key_from_did};
use serde::{Deserialize, Serialize};

use crate::token::Token;
use crate::{CourierError, Result};

const DOMAIN: &[u8] = b"osvauld/courier/ledger/v1\0";

/// One fact of the owner's records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entry {
    Issued(Token),
    Revoked { id: [u8; 32], at: u64 },
}

/// An entry as it travels: the bincode body and the owner's signature over it. The body does
/// not name its signer — [`verify`] takes the owner DID the reader already trusts, so an entry
/// cannot vouch for itself the way an attestation does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    pub body: Vec<u8>,
    pub sig: Vec<u8>,
}

/// Sign `entry` as the owner.
pub fn sign(signer: &(impl Signer + ?Sized), entry: &Entry) -> Result<Signed> {
    let body = bincode::serialize(entry).map_err(|_| CourierError::Decode)?;
    let sig = signer.sign(&[DOMAIN, &body].concat()).to_vec();
    Ok(Signed { body, sig })
}

/// Open `signed` as an entry of `owner_did`'s ledger: anyone else's signature or a touched
/// body is `BadSignature`, a signature or body that does not decode is `Decode`.
pub fn verify(signed: &Signed, owner_did: &str) -> Result<Entry> {
    let public = public_key_from_did(owner_did).ok_or(CourierError::Decode)?;
    let sig: [u8; 64] = signed
        .sig
        .as_slice()
        .try_into()
        .map_err(|_| CourierError::Decode)?;
    if !identity::verify(&public, &[DOMAIN, &signed.body].concat(), &sig) {
        return Err(CourierError::BadSignature);
    }
    bincode::deserialize(&signed.body).map_err(|_| CourierError::Decode)
}

#[cfg(test)]
mod tests;
