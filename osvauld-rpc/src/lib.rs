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
    /// Checked state — present only on `todo` blocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    /// Language tag — present only on `code` blocks that have one set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Inline formatting spans over the block's text (empty when unformatted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<MarkSpan>,
}

/// One inline-mark span over `[start, end)` code-point offsets of a block's text. `value` carries
/// a valued mark's payload (a `link`'s URL); it is `None` for flag marks (bold/italic/strike/code).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkSpan {
    pub start: usize,
    pub end: usize,
    pub mark: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Request {
    ListWorkspaces,
    ListItems { ws_id: String },
    ReadDoc { ws_id: String, item_id: String },
    SetBlockText { ws_id: String, item_id: String, block: String, text: String },
    /// Insert a new block (`kind` is a block tag: `paragraph`, `h1`..`h3`, `li`, `ol`, `todo`,
    /// `quote`, `code`, `divider`). It lands right after `after`; when `after` is `None` it is
    /// appended at the end. Returns the new block's id.
    InsertBlock { ws_id: String, item_id: String, after: Option<String>, kind: String, text: String },
    /// Change an existing block's kind (e.g. turn a paragraph into a heading or list item).
    SetBlockKind { ws_id: String, item_id: String, block: String, kind: String },
    /// Delete a block. Its children (if any) are promoted into its slot, not deleted with it.
    DeleteBlock { ws_id: String, item_id: String, block: String },
    /// Indent a block one level (nest it under its previous sibling). Returns whether it moved.
    IndentBlock { ws_id: String, item_id: String, block: String },
    /// Outdent a block one level (promote it out of its parent). Returns whether it moved.
    OutdentBlock { ws_id: String, item_id: String, block: String },
    /// Move a block relative to `target`. `position` is `before`, `after`, or `into` (as the
    /// last child of `target`).
    MoveBlock { ws_id: String, item_id: String, block: String, position: String, target: String },
    /// Set a `todo` block's checked state.
    SetTodoDone { ws_id: String, item_id: String, block: String, done: bool },
    /// Set a `code` block's language tag (e.g. `rust`, `python`).
    SetCodeLang { ws_id: String, item_id: String, block: String, lang: String },
    /// Apply a flag mark (`bold`, `italic`, `strike`, `code`) over `[start, end)` code points.
    ApplyMark { ws_id: String, item_id: String, block: String, start: usize, end: usize, mark: String },
    /// Apply a `link` mark carrying `url` over `[start, end)` code points.
    ApplyLink { ws_id: String, item_id: String, block: String, start: usize, end: usize, url: String },
    /// Remove a mark (`bold`/`italic`/`strike`/`code`/`link`) over `[start, end)` code points.
    ClearMark { ws_id: String, item_id: String, block: String, start: usize, end: usize, mark: String },
    /// Create a new item (`kind` is the lower-case tag: `doc`, `app`, `table`, `canvas`).
    CreateItem { ws_id: String, name: String, kind: String },
    /// List the source-file paths of an item's folder tree (an .app's `main.lua`, etc.).
    ListFiles { ws_id: String, item_id: String },
    /// Read one source file's text content.
    ReadFile { ws_id: String, item_id: String, path: String },
    /// Write (create or overwrite) one source file's text content.
    WriteFile { ws_id: String, item_id: String, path: String, content: String },
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
