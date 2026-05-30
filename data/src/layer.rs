use loro::{ExportMode, LoroDoc, LoroText};
use storage::Store;

use crate::error::DataError;

/// One CRDT document — the unit of sync and (later) of sharing.
///
/// A layer wraps a [`LoroDoc`] and persists into a [`Store`] under a single key
/// (its *layer id*, e.g. `"ws/<wsid>/file/<fid>"`). Everything inside a layer
/// travels together: it is the smallest thing that can be granted or synced.
///
/// MVP persistence is **snapshot-on-save** — each [`Layer::save`] writes a full
/// snapshot, replacing the previous one. The incremental oplog (append updates,
/// compact periodically) is deferred until sync needs it; the key scheme already
/// leaves room for it (`.../snapshot`, `.../oplog/<seq>`).
pub struct Layer {
    key: String,
    doc: LoroDoc,
}

impl Layer {
    /// A fresh, empty layer. In memory only until [`Layer::save`].
    pub fn create(key: impl Into<String>) -> Self {
        Layer { key: key.into(), doc: LoroDoc::new() }
    }

    /// Load a layer from its stored snapshot. `Ok(None)` means nothing is stored
    /// under `key` — the caller decides whether to [`create`](Layer::create) one.
    pub fn open(store: &Store, key: &str) -> Result<Option<Self>, DataError> {
        let Some(bytes) = store.get(key)? else {
            return Ok(None);
        };
        let doc = LoroDoc::new();
        doc.import(&bytes)?;
        Ok(Some(Layer { key: key.to_string(), doc }))
    }

    /// Commit pending edits and write the whole layer as a snapshot under its key.
    pub fn save(&self, store: &Store) -> Result<(), DataError> {
        self.doc.commit();
        let snapshot = self.doc.export(ExportMode::Snapshot)?;
        store.put(&self.key, &snapshot)?;
        Ok(())
    }

    /// The layer id this layer persists under.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The underlying CRDT document, for typed access to its containers
    /// (`get_text` / `get_map` / `get_list` / `get_tree`).
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// The named text container, created on first access. The `.doc` MVP body
    /// lives here.
    pub fn text(&self, name: &str) -> LoroText {
        self.doc.get_text(name)
    }
}

#[cfg(test)]
mod tests;
