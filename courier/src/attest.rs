//! "This device is mine": the DID's signature over the install's device key, which is the
//! iroh endpoint id. The transport proves a peer holds the device key; the attestation is
//! what ties that key to a person. No expiry — it states key custody, not a grant — and a
//! device is retired by the person's own records, not by this statement lapsing.

use serde::{Deserialize, Serialize};

use crate::{CourierError, Result, Signer, dec, public_key_from_did, sign_blob, verify};

const DOMAIN: &[u8] = b"osvauld/courier/attest/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    pub did: String,
    pub device: [u8; 32],
    /// Informational; a reader may age one out, nothing here does.
    pub iat: u64,
}

/// The text form a session's `Hello` carries. `device` is the install's own key, not
/// `Signer::device_public_key` (that one is derived from the phrase, so every install of one
/// person would share it).
pub fn attest(me: &(impl Signer + ?Sized), device: [u8; 32], now: u64) -> Result<String> {
    let claim = Attestation {
        did: me.did().to_string(),
        device,
        iat: now,
    };
    sign_blob(me, DOMAIN, &claim)
}

/// Opens an attestation against the DID it names. The caller then compares `device` with
/// the key the transport authenticated — that comparison is the whole point, and it is the
/// caller's because only the caller has the connection.
pub fn verify_attestation(token: &str) -> Result<Attestation> {
    let signed: crate::SignedBlob =
        serde_json::from_slice(&dec(token)?).map_err(|_| CourierError::Decode)?;
    let payload = dec(&signed.payload)?;
    let sig: [u8; 64] = dec(&signed.signature)?
        .try_into()
        .map_err(|_| CourierError::Decode)?;
    let claim: Attestation = bincode::deserialize(&payload).map_err(|_| CourierError::Decode)?;
    let public = public_key_from_did(&claim.did).ok_or(CourierError::Decode)?;
    if !verify(&public, &[DOMAIN, &payload].concat(), &sig) {
        return Err(CourierError::BadSignature);
    }
    Ok(claim)
}

#[cfg(test)]
mod tests;
