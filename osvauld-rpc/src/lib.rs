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
    ListItems {
        ws_id: String,
    },
    ReadDoc {
        ws_id: String,
        item_id: String,
    },
    SetBlockText {
        ws_id: String,
        item_id: String,
        block: String,
        text: String,
    },
    /// Insert a new block (`kind` is a block tag: `paragraph`, `h1`..`h3`, `li`, `ol`, `todo`,
    /// `quote`, `code`, `divider`). It lands right after `after`; when `after` is `None` it is
    /// appended at the end. Returns the new block's id.
    InsertBlock {
        ws_id: String,
        item_id: String,
        after: Option<String>,
        kind: String,
        text: String,
    },
    /// Change an existing block's kind (e.g. turn a paragraph into a heading or list item).
    SetBlockKind {
        ws_id: String,
        item_id: String,
        block: String,
        kind: String,
    },
    /// Delete a block. Its children (if any) are promoted into its slot, not deleted with it.
    DeleteBlock {
        ws_id: String,
        item_id: String,
        block: String,
    },
    /// Indent a block one level (nest it under its previous sibling). Returns whether it moved.
    IndentBlock {
        ws_id: String,
        item_id: String,
        block: String,
    },
    /// Outdent a block one level (promote it out of its parent). Returns whether it moved.
    OutdentBlock {
        ws_id: String,
        item_id: String,
        block: String,
    },
    /// Move a block relative to `target`. `position` is `before`, `after`, or `into` (as the
    /// last child of `target`).
    MoveBlock {
        ws_id: String,
        item_id: String,
        block: String,
        position: String,
        target: String,
    },
    /// Set a `todo` block's checked state.
    SetTodoDone {
        ws_id: String,
        item_id: String,
        block: String,
        done: bool,
    },
    /// Set a `code` block's language tag (e.g. `rust`, `python`).
    SetCodeLang {
        ws_id: String,
        item_id: String,
        block: String,
        lang: String,
    },
    /// Apply a flag mark (`bold`, `italic`, `strike`, `code`) over `[start, end)` code points.
    ApplyMark {
        ws_id: String,
        item_id: String,
        block: String,
        start: usize,
        end: usize,
        mark: String,
    },
    /// Apply a `link` mark carrying `url` over `[start, end)` code points.
    ApplyLink {
        ws_id: String,
        item_id: String,
        block: String,
        start: usize,
        end: usize,
        url: String,
    },
    /// Remove a mark (`bold`/`italic`/`strike`/`code`/`link`) over `[start, end)` code points.
    ClearMark {
        ws_id: String,
        item_id: String,
        block: String,
        start: usize,
        end: usize,
        mark: String,
    },
    /// Create a new item (`kind` is the lower-case tag: `doc`, `app`, `table`, `canvas`).
    CreateItem {
        ws_id: String,
        name: String,
        kind: String,
    },
    /// List the source-file paths of an item's folder tree (an .app's `main.lua`, etc.).
    ListFiles {
        ws_id: String,
        item_id: String,
    },
    /// Read one source file's text content.
    ReadFile {
        ws_id: String,
        item_id: String,
        path: String,
    },
    /// Write (create or overwrite) one source file's text content.
    WriteFile {
        ws_id: String,
        item_id: String,
        path: String,
        content: String,
    },
    /// Read a `.lua` file as its blocks (one per top-level construct), with stable IDs — the
    /// per-block counterpart of [`Request::ReadFile`], so an agent can target a single construct.
    ReadFileBlocks {
        ws_id: String,
        item_id: String,
        path: String,
    },
    /// Replace the text of one block of a `.lua` file, identified by its stable block ID (from
    /// [`Request::ReadFileBlocks`]). Block identity — and any other block — is preserved.
    SetFileBlockText {
        ws_id: String,
        item_id: String,
        path: String,
        block: String,
        text: String,
    },
    /// Insert a new block into a `.lua` file, right after `after` (or appended when `after` is
    /// `None`). `kind` is a free structural tag (`statement`, `comment`, `function`); returns the
    /// new block's id.
    InsertFileBlock {
        ws_id: String,
        item_id: String,
        path: String,
        after: Option<String>,
        kind: String,
        text: String,
    },
    /// Delete one block of a `.lua` file by its stable block ID.
    DeleteFileBlock {
        ws_id: String,
        item_id: String,
        path: String,
        block: String,
    },
    /// Read an .app's runtime data CRDT (named top-level containers) as a deep JSON value.
    AppDataGet {
        ws_id: String,
        item_id: String,
    },
    /// Replace the content of one top-level text container in an .app's runtime data CRDT — the
    /// same operation the app's own `ui.editor` makes, so an open run pane updates live.
    AppDataSetText {
        ws_id: String,
        item_id: String,
        name: String,
        text: String,
    },
    /// Add a row (a JSON object of scalar fields) to a top-level list in an .app's runtime data
    /// CRDT — the same op the app's Lua `list:add` makes. Stamps a stable `id` unless one is
    /// supplied; returns `{ id }`.
    AppDataRowAdd {
        ws_id: String,
        item_id: String,
        list: String,
        fields: serde_json::Value,
    },
    /// Set scalar fields (JSON `null` deletes a field) on the row with stable id `row` in a
    /// top-level list.
    AppDataRowSet {
        ws_id: String,
        item_id: String,
        list: String,
        row: String,
        fields: serde_json::Value,
    },
    /// Remove the row with stable id `row` from a top-level list.
    AppDataRowRemove {
        ws_id: String,
        item_id: String,
        list: String,
        row: String,
    },
    /// Export a page-declaring .app to PDF. Returns the written file's path.
    ExportPdf {
        ws_id: String,
        item_id: String,
    },
    /// Render an .app off-screen and return it as base64 PNG. `width`/`height` are logical px
    /// (default: the app's page, else 900×700); `scale` is px per logical px (default 2).
    Screenshot {
        ws_id: String,
        item_id: String,
        width: Option<f32>,
        height: Option<f32>,
        scale: Option<f32>,
    },
    /// Open an .xlsx into migration staging (calamine → Polars), returning a handle plus a
    /// per-sheet summary (rows, cols, inferred column dtypes). Dev: `path` is a host filesystem
    /// path; the eventual model is a user-selected upload buffer (no arbitrary path read).
    ImportOpen {
        path: String,
    },
    /// The first `n` rows of a staged sheet as a text table (omit `sheet` for the first sheet).
    ImportHead {
        handle: String,
        sheet: Option<String>,
        n: usize,
    },
    /// Run SQL over a staged sheet (registered as table `data`) — the profiling escape hatch
    /// (DISTINCT / GROUP BY / COUNT / aggregates) the agent uses to understand the data.
    ImportSql {
        handle: String,
        sheet: Option<String>,
        query: String,
    },
    /// Drop a staged workbook (the .xlsx is transient migration input).
    ImportClose {
        handle: String,
    },
    /// Run Polars SQL over a *stored* `.table` item (registered both as `t` and under its item
    /// name) — the live-table profiling tool the agent uses to design dashboards over real data.
    TableSql {
        ws_id: String,
        item_id: String,
        query: String,
    },
    /// Materialize a staged sheet into a new `.table` item in `ws_id`. `columns` is the agent's
    /// plan — a JSON array of `{ source, key, label, type }` (type = text/number/decimal/check/
    /// select/date) — typed-coerced into a stored schema + rows. Returns the new item.
    ImportToLayer {
        handle: String,
        sheet: Option<String>,
        ws_id: String,
        name: String,
        columns: serde_json::Value,
    },
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
        Response::Ok {
            result: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Response::Err {
            message: message.into(),
        }
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
