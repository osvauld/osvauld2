//! The share link: `osvauld://s/<base58>` — one host's address and a 12-byte code the host
//! turns into an invite (`Frame::Fetch`). Short enough to read out or type on a phone when
//! the host is reachable through a relay (~75 characters); longer on a LAN with no relay,
//! where the host's direct addresses ride along (iroh 1.3 has no local discovery). Pure
//! encoding: who may mint one and what the code is worth are the host's business
//! (`kunki::admin`).
//!
//! Packed by hand rather than through serde: a length prefix per field would double a link.
//!
//! ```text
//! version u8 = 1 | device [32] | code [12] | relay u8: 0 none, 1..=4 a known n0 relay,
//! 255 custom (u8 len, url) | addrs u8 count, each: 4 | 6 then the ip bytes, port u16 BE
//! ```

use std::net::{IpAddr, SocketAddr};

use serde::{Deserialize, Serialize};

use crate::{CourierError, Result};

pub const PREFIX: &str = "osvauld://s/";
pub const CODE_LEN: usize = 12;
const VERSION: u8 = 1;

/// n0's public relays, by the index a link carries. The URL form is what iroh reports.
pub const KNOWN_RELAYS: [&str; 4] = [
    "https://use1-1.relay.n0.iroh.link./",
    "https://usw1-1.relay.n0.iroh.link./",
    "https://euc1-1.relay.n0.iroh.link./",
    "https://aps1-1.relay.n0.iroh.link./",
];

/// Where a host can be dialled: its device key (the iroh endpoint id), its relay if it has
/// one, and its direct socket addresses when there is no relay to find it through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostAddr {
    pub device: [u8; 32],
    pub relay: Option<String>,
    pub addrs: Vec<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub host: HostAddr,
    pub code: [u8; CODE_LEN],
}

impl Link {
    pub fn to_text(&self) -> Result<String> {
        let mut out = vec![VERSION];
        out.extend_from_slice(&self.host.device);
        out.extend_from_slice(&self.code);
        match &self.host.relay {
            None => out.push(0),
            Some(url) => match KNOWN_RELAYS.iter().position(|k| k == url) {
                Some(i) => out.push(i as u8 + 1),
                None => {
                    let bytes = url.as_bytes();
                    if bytes.len() > 255 {
                        return Err(CourierError::Decode);
                    }
                    out.push(255);
                    out.push(bytes.len() as u8);
                    out.extend_from_slice(bytes);
                }
            },
        }
        if self.host.addrs.len() > 255 {
            return Err(CourierError::Decode);
        }
        out.push(self.host.addrs.len() as u8);
        for addr in &self.host.addrs {
            match addr.ip() {
                IpAddr::V4(ip) => {
                    out.push(4);
                    out.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    out.push(6);
                    out.extend_from_slice(&ip.octets());
                }
            }
            out.extend_from_slice(&addr.port().to_be_bytes());
        }
        Ok(format!("{PREFIX}{}", bs58::encode(out).into_string()))
    }

    /// Trims whitespace; refuses anything without the prefix so a ticket or a name pasted
    /// here fails by name rather than decoding into nonsense.
    pub fn from_text(text: &str) -> Result<Link> {
        let body = text
            .trim()
            .strip_prefix(PREFIX)
            .ok_or(CourierError::Decode)?;
        let bytes = bs58::decode(body)
            .into_vec()
            .map_err(|_| CourierError::Decode)?;
        let mut r = Reader(&bytes);
        if r.u8()? != VERSION {
            return Err(CourierError::Decode);
        }
        let device: [u8; 32] = r.take(32)?.try_into().map_err(|_| CourierError::Decode)?;
        let code: [u8; CODE_LEN] = r
            .take(CODE_LEN)?
            .try_into()
            .map_err(|_| CourierError::Decode)?;
        let relay = match r.u8()? {
            0 => None,
            i @ 1..=4 => Some(KNOWN_RELAYS[i as usize - 1].to_string()),
            255 => {
                let len = r.u8()? as usize;
                Some(
                    std::str::from_utf8(r.take(len)?)
                        .map_err(|_| CourierError::Decode)?
                        .to_string(),
                )
            }
            _ => return Err(CourierError::Decode),
        };
        let n = r.u8()? as usize;
        let mut addrs = Vec::with_capacity(n);
        for _ in 0..n {
            let ip = match r.u8()? {
                4 => {
                    let o: [u8; 4] = r.take(4)?.try_into().map_err(|_| CourierError::Decode)?;
                    IpAddr::from(o)
                }
                6 => {
                    let o: [u8; 16] = r.take(16)?.try_into().map_err(|_| CourierError::Decode)?;
                    IpAddr::from(o)
                }
                _ => return Err(CourierError::Decode),
            };
            let port = u16::from_be_bytes(r.take(2)?.try_into().map_err(|_| CourierError::Decode)?);
            addrs.push(SocketAddr::new(ip, port));
        }
        if !r.0.is_empty() {
            return Err(CourierError::Decode);
        }
        Ok(Link {
            host: HostAddr {
                device,
                relay,
                addrs,
            },
            code,
        })
    }

    pub fn is_link(text: &str) -> bool {
        text.trim().starts_with(PREFIX)
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(CourierError::Decode);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
}

/// Twelve random bytes: not guessable in a lifetime even without the host's guard.
pub fn new_code() -> [u8; CODE_LEN] {
    use rand::RngCore;
    let mut code = [0u8; CODE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut code);
    code
}

#[cfg(test)]
mod tests;
