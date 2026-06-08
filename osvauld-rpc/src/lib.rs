use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSummary {
    pub id: String,
    pub ws_id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSummary {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub depth: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Request {
    ListWorkspaces,
    ListItems { ws_id: String },
    ReadDoc { ws_id: String, item_id: String },
    SetBlockText { ws_id: String, item_id: String, block: String, text: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    #[serde(rename = "ok")]
    Ok { result: serde_json::Value },
    #[serde(rename = "err")]
    Err { message: String },
}

impl Response {
    pub fn ok(result: impl Serialize) -> Self {
        Response::Ok { result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null) }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Response::Err { message: message.into() }
    }
}

// 4-byte big-endian length prefix + payload
pub fn write_msg<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "message too large"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

pub fn read_msg<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests;
