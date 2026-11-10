//! A message on an app channel (`jetpack-mp.md` §6 step 2): ephemeral, never stored, signed by
//! the sending device's key (its iroh key) so a host relays what it cannot forge. The host reads
//! the routing fields to check membership and rate; the body is the app's, never parsed here.
//!
//! Who a device is comes from elsewhere: its `courier::attest` attestation ties `from_device` to
//! a DID, and attestations are DID-signed, so a host cannot invent that either. Transport-free
//! like the rest of the crate: [`seal`] takes the signing as a closure, which keeps iroh out.

use std::collections::{BTreeSet, HashMap};

use manifest::{Channel, Manifest, Who};
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

/// Whether the session that proved `device` on space `ws`, holding `roles` on the item, may send
/// `env`: the envelope names that device and space, the channel is declared, `send` admits one
/// of the roles, and the body fits the channel's `size`. A channel with no `send` admits every
/// member — getting this far proved membership. `members(...)` and DID variables admit nobody
/// here: a channel has no path to bind them, and the manifest parser refuses the variable form.
pub fn may_send<'m>(
    env: &Envelope,
    device: [u8; 32],
    ws: &str,
    manifest: &'m Manifest,
    roles: &BTreeSet<String>,
) -> Result<&'m Channel> {
    if env.from_device != device || env.ws != ws {
        return Err(CourierError::WrongDevice);
    }
    let ch = manifest
        .channels
        .iter()
        .find(|c| c.name == env.channel)
        .ok_or(CourierError::UndeclaredChannel)?;
    let admitted = ch.send.is_empty()
        || ch.send.iter().any(|who| match who {
            Who::Role(r) => roles.iter().any(|held| manifest.satisfies(held, r)),
            Who::Var(_) | Who::Node | Who::Members(_) => false,
        });
    if !admitted {
        return Err(CourierError::NoSend);
    }
    if ch.size_bytes.is_some_and(|max| env.body.len() > max as usize) {
        return Err(CourierError::TooLarge);
    }
    Ok(ch)
}

pub use manifest::DEFAULT_CHANNEL_RATE as DEFAULT_RATE;

/// A token bucket per sender, item and channel: up to a second's worth in a burst, refilled
/// continuously at the channel's rate.
#[derive(Debug, Default)]
pub struct Rate(HashMap<([u8; 32], String, String), (f64, u64)>);

impl Rate {
    pub fn allow(&mut self, env: &Envelope, per_sec: Option<u32>, now_ms: u64) -> bool {
        let cap = f64::from(per_sec.unwrap_or(DEFAULT_RATE));
        let key = (env.from_device, env.item.clone(), env.channel.clone());
        let (tokens, last) = self.0.entry(key).or_insert((cap, now_ms));
        let refill = now_ms.saturating_sub(*last) as f64 * cap / 1000.0;
        *tokens = (*tokens + refill).min(cap);
        *last = now_ms;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;
