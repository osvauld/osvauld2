//! Workspace items: the typed files a workspace holds (.doc, .table, .app, .canvas, …).
//!
//! Each item is a named, kinded container under `ws/<ws>/item/<id>/` with three kinds of key,
//! all three sealed to the account key:
//!   - **meta** (`…/meta`) — the [`WorkspaceItem`] header.
//!   - **src** (`…/src`) — an .app's source doc (a Loro snapshot: a `files` map of path → text,
//!     holding `main.lua`, `lib/state.lua`, `manifest.osv`). The path *is* the identity; the
//!     folder structure is read back off the `/`-separated paths, never stored as a tree.
//!   - **doc** (`…/doc`) — the item's runtime CRDT (a Loro snapshot): the block tree of a
//!
//!

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

pub(crate) fn valid_doc_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

pub(crate) fn meta_key(ws_id: &str, item_id: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/meta")
}

/// The .app's source doc (`…/src`), kept apart from its state doc.
pub(crate) fn src_key(ws_id: &str, item_id: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/src")
}

/// Prefix for all meta keys under a workspace — used to enumerate items.
pub(crate) fn items_prefix(ws_id: &str) -> String {
    format!("ws/{ws_id}/item/")
}
/// One of the item's named runtime CRDTs (`…/doc/<name>`). Item-scoped, so two apps can both
/// have a doc called "board" without a registry to keep them apart.
pub(crate) fn doc_key(ws_id: &str, item_id: &str, name: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/doc/{name}")
}
/// Extract the item id from a `ws/<ws>/item/<id>/meta` key, `None` for any other key
/// under the same prefix (src, state).
pub(crate) fn id_from_meta_key<'a>(ws_id: &str, key: &'a str) -> Option<&'a str> {
    let rest = key
        .strip_prefix(&format!("ws/{ws_id}/item/"))?
        .strip_suffix("/meta")?;
    crate::workspace::valid_id(rest).then_some(rest)
}
