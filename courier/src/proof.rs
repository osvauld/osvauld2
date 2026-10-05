//! Proven caller: a desktop signs every request it makes to a node, so the `desktop_did` a
//! request names is the key that sent it. `token::verify_chain` binds a token to its `sub`;
//! this binds `sub` to the sender — without it anyone holding a copy of a token could act as
//! its holder.
//!
//! The signature covers the request's exact wire bytes (hashed), the node it is for, a fresh
//! id and a millisecond timestamp. A node admits a timestamp no older than [`WINDOW_MS`] and
//! not in its future, and each id once. [`Replay`] keeps the ids in memory and refuses
//! anything stamped at or before its own start: every request an earlier process admitted was
//! stamped before that process stopped, so a restart re-admits nothing.
//!
//! No future skew is allowed because desktop and node share a clock today. A remote transport
//! needs the node to hand out its time instead.

use std::collections::HashMap;

use identity::{Signer, public_key_from_did, verify as verify_sig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CourierError, Result, dec, enc, nonce};

const DOMAIN: &[u8] = b"osv/request/v1";

/// How old a request's timestamp may be, in milliseconds.
pub const WINDOW_MS: u64 = 60_000;

/// Ids held at once. Anyone can sign as a fresh key of their own, so the set is bounded before
/// any authorization has run.
pub const CAPACITY: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proof {
    pub request_id: String,
    pub ts_ms: u64,
    pub sig: String,
}

/// `body` is the request exactly as it goes on the wire; the verifier hashes the bytes it
/// received, so no canonical encoding is needed.
pub fn prove(signer: &(impl Signer + ?Sized), node_did: &str, body: &[u8], now_ms: u64) -> Proof {
    let request_id = nonce();
    let sig = signer.sign(&message(node_did, &request_id, now_ms, body));
    Proof {
        request_id,
        ts_ms: now_ms,
        sig: enc(sig),
    }
}

/// Signature and window only — replay is [`Replay::admit`]'s, which needs state.
pub fn verify(proof: &Proof, caller: &str, node_did: &str, body: &[u8], now_ms: u64) -> Result<()> {
    let key = public_key_from_did(caller).ok_or(CourierError::Decode)?;
    let sig: [u8; 64] = dec(&proof.sig)?
        .try_into()
        .map_err(|_| CourierError::Decode)?;
    let signed = message(node_did, &proof.request_id, proof.ts_ms, body);
    if !verify_sig(&key, &signed, &sig) {
        return Err(CourierError::BadSignature);
    }
    if proof.ts_ms > now_ms || now_ms - proof.ts_ms > WINDOW_MS {
        return Err(CourierError::StaleRequest);
    }
    Ok(())
}

// Fields are length-prefixed so no two different tuples share an encoding.
fn message(node_did: &str, request_id: &str, ts_ms: u64, body: &[u8]) -> Vec<u8> {
    let mut m = DOMAIN.to_vec();
    for field in [node_did.as_bytes(), request_id.as_bytes()] {
        m.extend((field.len() as u32).to_be_bytes());
        m.extend(field);
    }
    m.extend(ts_ms.to_be_bytes());
    m.extend(Sha256::digest(body));
    m
}

/// Request ids a node has admitted inside the window.
pub struct Replay {
    since_ms: u64,
    seen: HashMap<String, u64>,
}

impl Replay {
    /// `since_ms` is when this set started; anything stamped then or earlier may have been
    /// admitted by a set that no longer exists.
    pub fn new(since_ms: u64) -> Self {
        Self {
            since_ms,
            seen: HashMap::new(),
        }
    }

    /// Call after [`verify`]: `proof.ts_ms` is trusted to be within the window by then.
    pub fn admit(&mut self, proof: &Proof, now_ms: u64) -> Result<()> {
        if proof.ts_ms <= self.since_ms {
            return Err(CourierError::Replayed);
        }
        // `nonce()`'s shape; anything else did not come from `prove`.
        if proof.request_id.len() != 32 {
            return Err(CourierError::Decode);
        }
        self.seen
            .retain(|_, ts| ts.saturating_add(WINDOW_MS) >= now_ms);
        if self.seen.contains_key(&proof.request_id) {
            return Err(CourierError::Replayed);
        }
        if self.seen.len() >= CAPACITY {
            return Err(CourierError::Busy);
        }
        self.seen.insert(proof.request_id.clone(), proof.ts_ms);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests;
