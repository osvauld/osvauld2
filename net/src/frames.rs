//! u32-big-endian length + bincode body on a QUIC stream. The receiver checks the length
//! against the caller's cap before reading a byte of the body, so a stranger claiming a
//! 4 GiB frame costs four bytes, not an allocation.

use iroh::endpoint::{RecvStream, SendStream};
use serde::{Serialize, de::DeserializeOwned};

use crate::{Error, Result};

/// The most a session's first frame may claim before the peer has proved anything. A
/// `Hello` — an attestation, a token, a few version vectors — is a few hundred bytes.
pub const FIRST_FRAME: usize = 64 * 1024;
/// The most any later frame may claim: a doc update.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

/// The receiving half: buffers stream bytes and yields whole frames.
pub struct FrameBuf {
    recv: RecvStream,
    buf: Vec<u8>,
}

impl FrameBuf {
    pub fn new(recv: RecvStream) -> Self {
        Self {
            recv,
            buf: Vec::new(),
        }
    }

    /// `None` when the peer finished its side cleanly. Cancel-safe: bytes already read stay
    /// in the buffer for the next call.
    pub async fn recv<T: DeserializeOwned>(&mut self, limit: usize) -> Result<Option<T>> {
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            if let Some(head) = self.buf.first_chunk::<4>() {
                let len = u32::from_be_bytes(*head) as usize;
                if len > limit {
                    return Err(Error::Protocol(format!("frame of {len} bytes over the cap")));
                }
                if self.buf.len() >= 4 + len {
                    let msg = bincode::deserialize(&self.buf[4..4 + len])
                        .map_err(|_| Error::Protocol("frame does not decode".into()))?;
                    self.buf.drain(..4 + len);
                    return Ok(Some(msg));
                }
            }
            match self.recv.read(&mut chunk).await.map_err(Error::net)? {
                Some(n) => self.buf.extend_from_slice(&chunk[..n]),
                None if self.buf.is_empty() => return Ok(None),
                None => return Err(Error::Protocol("stream ended mid-frame".into())),
            }
        }
    }
}

/// Writes one frame: the length prefix and the body in a single `write_all`.
pub async fn send_frame<T: Serialize>(stream: &mut SendStream, msg: &T, limit: usize) -> Result<()> {
    let body = bincode::serialize(msg).map_err(|_| Error::Protocol("frame does not encode".into()))?;
    if body.len() > limit {
        return Err(Error::Protocol(format!("frame of {} bytes over the cap", body.len())));
    }
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&body);
    stream.write_all(&out).await.map_err(Error::net)
}
