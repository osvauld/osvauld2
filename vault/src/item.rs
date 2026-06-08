//! Workspace items: the typed files a workspace holds (.doc, .table, .app, .canvas, …).
//!
//! Each item is a named, kinded container with two storage buckets:
//!   - **blobs** (`ws/<ws>/item/<id>/blob/<name>`) — raw bytes: Lua scripts, manifest.osv,
//!     assets. Not encrypted at rest; authorship comes from the identity layer later.
//!   - **CRDT layers** (`ws/<ws>/item/<id>/crdt/<name>`) — Loro snapshots: the collaborative
//!     data the item type defines (e.g. the block tree for a .doc, per-channel messages for
//!     an app).
//!
//! The item's header (`ws/<ws>/item/<id>/meta`) is sealed to the account key, same as
//! workspace headers. Discovery is a prefix scan for meta keys; other keys under
//! `ws/<ws>/item/<id>/` are only loaded on demand.

use serde::{Deserialize, Serialize};

use crate::workspace::{new_id, now_secs};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Doc,
    Table,
    App,
    Canvas,
}

impl ItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Doc => "doc",
            ItemKind::Table => "table",
            ItemKind::App => "app",
            ItemKind::Canvas => "canvas",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceItem {
    pub id: String,
    pub ws_id: String,
    pub name: String,
    pub kind: ItemKind,
    pub created: u64,
}

impl WorkspaceItem {
    pub(crate) fn new(ws_id: &str, name: &str, kind: ItemKind) -> Self {
        Self {
            id: new_id(),
            ws_id: ws_id.to_string(),
            name: name.to_string(),
            kind,
            created: now_secs(),
        }
    }
}

// ── Key helpers ───────────────────────────────────────────────────────────────

pub(crate) fn meta_key(ws_id: &str, item_id: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/meta")
}

pub(crate) fn blob_key(ws_id: &str, item_id: &str, name: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/blob/{name}")
}

pub(crate) fn crdt_key(ws_id: &str, item_id: &str, name: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/crdt/{name}")
}

/// Prefix for all meta keys under a workspace — used to enumerate items.
pub(crate) fn items_prefix(ws_id: &str) -> String {
    format!("ws/{ws_id}/item/")
}

/// Extract the item id from a `ws/<ws>/item/<id>/meta` key, `None` for any other key
/// under the same prefix (blobs, layers, etc.).
pub(crate) fn id_from_meta_key<'a>(ws_id: &str, key: &'a str) -> Option<&'a str> {
    let rest = key.strip_prefix(&format!("ws/{ws_id}/item/"))?.strip_suffix("/meta")?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

/// The placeholder manifest written into every new App item until the real
/// manifest.osv format is defined.
pub(crate) const APP_MANIFEST_PLACEHOLDER: &[u8] = b"app \"unnamed\" version \"0.1.0\" {}\n";
