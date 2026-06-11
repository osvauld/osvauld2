//! Workspace items: the typed files a workspace holds (.doc, .table, .app, .canvas, …).
//!
//! Each item is a named, kinded container under `ws/<ws>/item/<id>/` with three kinds of key:
//!   - **meta** (`…/meta`) — the sealed [`WorkspaceItem`] header, encrypted to the account key.
//!   - **state** (`…/state`) — the item's runtime CRDT (a Loro snapshot): the block tree of a
//!     .doc, the rows of a .table, an app's own opaque runtime doc. One per item.
//!   - **files** (`…/files/<path>`) — a path-addressed source tree: an .app's `main.lua`,
//!     `lib/state.lua`, `manifest.osv`, assets. The path *is* the identity (S3-style); the
//!     folder structure is read back off the `/`-separated paths, never stored as a tree.
//!
//! Item discovery is a prefix scan for `meta` keys; `state` and `files/*` are loaded on demand.

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

/// The item's single runtime CRDT snapshot (`…/state`).
pub(crate) fn state_key(ws_id: &str, item_id: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/state")
}

/// A source file at `path` in the item's folder tree (`…/files/<path>`).
pub(crate) fn file_key(ws_id: &str, item_id: &str, path: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/files/{path}")
}

/// Prefix for an item's whole source tree — used to enumerate its files.
pub(crate) fn files_prefix(ws_id: &str, item_id: &str) -> String {
    format!("ws/{ws_id}/item/{item_id}/files/")
}

/// Recover the file path from a `…/files/<path>` key under `files_prefix`.
pub(crate) fn path_from_file_key<'a>(ws_id: &str, item_id: &str, key: &'a str) -> Option<&'a str> {
    key.strip_prefix(&files_prefix(ws_id, item_id))
}

/// Prefix for all meta keys under a workspace — used to enumerate items.
pub(crate) fn items_prefix(ws_id: &str) -> String {
    format!("ws/{ws_id}/item/")
}

/// Extract the item id from a `ws/<ws>/item/<id>/meta` key, `None` for any other key
/// under the same prefix (state, files, etc.).
pub(crate) fn id_from_meta_key<'a>(ws_id: &str, key: &'a str) -> Option<&'a str> {
    let rest = key.strip_prefix(&format!("ws/{ws_id}/item/"))?.strip_suffix("/meta")?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

/// The placeholder manifest seeded into every new App item until the real manifest.osv
/// format is defined.
pub(crate) const APP_MANIFEST_SEED: &[u8] = b"app \"unnamed\" version \"0.1.0\" {}\n";

/// The starter entry point for every new App item, so a freshly created app renders immediately
/// (before an agent has written anything). A complete `view = f(state)` app.
///
/// The vault owns the *text* but no longer writes `main.lua` itself: a `.lua` file is stored as a
/// `block_doc` snapshot, and the vault deliberately knows nothing about Loro (same reason `.doc`
/// state is seeded above the vault). The host splits this into blocks and writes it — see
/// `sthalam`'s `seed_item_state`.
pub const APP_MAIN_SEED: &str = r##"return function()
  return ui.col{ style = { padding = 28, gap = 12, background = "#14161a",
                           width = "100%", height = "100%" },
    ui.text{ "new app", style = { font = 22, color = "#e6e6ea" } },
    ui.text{ "Edit main.lua to build your app.",
             style = { font = 14, color = "#9aa0ab" } },
  }
end
"##;
