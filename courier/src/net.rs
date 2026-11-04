//! A message on an app channel (`jetpack-mp.md` §6 step 2): ephemeral, never stored, signed by
//! the sending device's key (its iroh key) so a host relays what it cannot forge. The host reads
//! the routing fields to check membership and rate; the body is the app's, never parsed here.
//!
//! Who a device is comes from elsewhere: its `courier::attest` attestation ties `from_device` to
//! a DID, and attestations are DID-signed, so a host cannot invent that either. Transport-free
//! like the rest of the crate: [`seal`] takes the signing as a closure, which keeps iroh out.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{CourierError, Result};

const DOMAIN: &[u8] = b"osvauld/courier/net/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub ws: String,
    pub item: String,
    pub channel: String,
    /// Rises per `(from_device, item, channel)`; gaps are drops, not errors ([`Seen`]).
    pub seq: u64,
    pub from_device: [u8; 32],
    pub body: Vec<u8>,
}

/// An envelope as it travels: its bincode and the device's signature over it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub head: Vec<u8>,
    pub sig: Vec<u8>,
}

/// Sign `env` with the key whose public half is `env.from_device`.
pub fn seal(env: &Envelope, sign: impl FnOnce(&[u8]) -> [u8; 64]) -> Result<Sealed> {
    too_large(env)?;
    let head = bincode::serialize(env).map_err(|_| CourierError::Decode)?;
    let sig = sign(&[DOMAIN, &head].concat()).to_vec();
    Ok(Sealed { head, sig })
}

/// Open `sealed` against the device it names: any touched byte, or another key, is
/// `BadSignature`. Whether that device may send on that channel is the caller's question.
pub fn open(sealed: &Sealed) -> Result<Envelope> {
    let env: Envelope = bincode::deserialize(&sealed.head).map_err(|_| CourierError::Decode)?;
    too_large(&env)?;
    let sig: [u8; 64] = sealed.sig.as_slice().try_into().map_err(|_| CourierError::Decode)?;
    if !identity::verify(&env.from_device, &[DOMAIN, &sealed.head].concat(), &sig) {
        return Err(CourierError::BadSignature);
    }
    Ok(env)
}

// The ceiling only; a channel's own `size` is checked where its manifest is known.
fn too_large(env: &Envelope) -> Result<()> {
    if env.body.len() > manifest::CHANNEL_SIZE_MAX as usize {
        return Err(CourierError::TooLarge);
    }
    Ok(())
}

/// The last `seq` seen per sender, item and channel. A signature proves who said it, not that
/// it is new; this refuses a replay, which a relay could otherwise inject at will.
#[derive(Debug, Default)]
pub struct Seen(HashMap<([u8; 32], String, String), u64>);

impl Seen {
    pub fn fresh(&mut self, env: &Envelope) -> bool {
        let key = (env.from_device, env.item.clone(), env.channel.clone());
        match self.0.get(&key) {
            Some(&last) if env.seq <= last => false,
            _ => {
                self.0.insert(key, env.seq);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests;
