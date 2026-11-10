//! Binary item files (models, images): the source doc's `blobs` map records `path → {blob,
//! sha256, size}`; the bytes are sealed vault entries keyed by the random `blob` id. Keeping
//! bytes out of the CRDT means re-exporting a model never grows source history by its size,
//! and the random id keeps a plaintext hash out of on-disk key names.
use loro::{LoroDoc, LoroValue, ValueOrContainer};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;

pub const MAX_BINARY_FILE: usize = 64 * 1024 * 1024;
pub const MAX_BINARY_ITEM: usize = 256 * 1024 * 1024;
const BLOBS: &str = "blobs";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryFile {
    pub path: String,
    pub blob: String,
    pub sha256: String,
    pub size: usize,
}

pub(crate) struct Revision {
    bindings: HashMap<String, BinaryFile>,
    retained: RefCell<HashMap<String, Vec<u8>>>,
    read: crate::Files,
}

impl Revision {
    pub(crate) fn new(doc: &LoroDoc, read: crate::Files) -> Self {
        Self {
            bindings: binary_files(doc).into_iter().map(|f| (f.path.clone(), f)).collect(),
            retained: RefCell::new(HashMap::new()),
            read,
        }
    }

    pub(crate) fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let file = self.bindings.get(path).ok_or_else(|| format!("no binary file {path:?}"))?;
        if let Some(bytes) = self.retained.borrow().get(&file.blob) {
            return Ok(bytes.clone());
        }
        (self.read)(&file.blob)?.ok_or_else(|| format!("{path}: stored bytes are missing"))
    }

    // A failed reload keeps this VM alive after the source's old blob is deleted. Preserve
    // its bytes before replacement, including assets this VM has not loaded yet.
    pub(crate) fn retain(&self, path: &str) -> Result<(), String> {
        let Some(file) = self.bindings.get(path) else { return Ok(()) };
        if !self.retained.borrow().contains_key(&file.blob) {
            let bytes = self.read(path)?;
            self.retained.borrow_mut().insert(file.blob.clone(), bytes);
        }
        Ok(())
    }
}

/// After a bind the caller persists `snapshot`, then deletes `orphan` (a blob no path
/// references any more). New bytes were already stored, before the doc changed.
#[derive(Debug, PartialEq, Eq)]
pub struct BoundFile {
    pub blob: String,
    pub snapshot: Vec<u8>,
    pub orphan: Option<String>,
}

pub fn binary_files(doc: &LoroDoc) -> Vec<BinaryFile> {
    let mut out: Vec<BinaryFile> = doc
        .get_map(BLOBS)
        .keys()
        .filter_map(|path| binary_file(doc, &path))
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

pub fn binary_file(doc: &LoroDoc, path: &str) -> Option<BinaryFile> {
    let Some(ValueOrContainer::Value(LoroValue::Map(m))) = doc.get_map(BLOBS).get(path) else {
        return None;
    };
    let text = |k: &str| match m.get(k) {
        Some(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    };
    let size = match m.get("size") {
        Some(LoroValue::I64(n)) if *n >= 0 => *n as usize,
        _ => return None,
    };
    Some(BinaryFile {
        path: path.to_string(),
        blob: text("blob")?,
        sha256: text("sha256")?,
        size,
    })
}

pub fn is_binary_path(doc: &LoroDoc, path: &str) -> bool {
    doc.get_map(BLOBS).get(path).is_some()
}

/// Record `bytes` at `path`. `store` seals new bytes and returns their blob id; it runs only
/// when no path already holds the same bytes, and before the doc changes, so a failed store
/// leaves the doc untouched. Size and kind are validated first.
pub fn bind_binary_file(
    doc: &LoroDoc,
    path: &str,
    bytes: &[u8],
    store: impl FnOnce(&[u8]) -> Result<String, String>,
) -> Result<BoundFile, String> {
    if bytes.len() > MAX_BINARY_FILE {
        return Err(format!("{path}: binary file over {MAX_BINARY_FILE} bytes"));
    }
    if doc.get_map("files").get(path).is_some() {
        return Err(format!("{path} is a text source file"));
    }
    let files = binary_files(doc);
    let total: usize = files.iter().filter(|f| f.path != path).map(|f| f.size).sum();
    if total + bytes.len() > MAX_BINARY_ITEM {
        return Err(format!("item binary files over {MAX_BINARY_ITEM} bytes"));
    }
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let previous = files.iter().find(|f| f.path == path).map(|f| f.blob.clone());
    let blob = match files.iter().find(|f| f.sha256 == sha256) {
        Some(same) => same.blob.clone(),
        None => store(bytes)?,
    };
    let entry: HashMap<String, LoroValue> = [
        ("blob".to_string(), LoroValue::from(blob.as_str())),
        ("sha256".to_string(), LoroValue::from(sha256.as_str())),
        ("size".to_string(), LoroValue::I64(bytes.len() as i64)),
    ]
    .into();
    doc.get_map(BLOBS)
        .insert(path, LoroValue::from(entry))
        .map_err(|e| e.to_string())?;
    doc.commit();
    let snapshot = doc
        .export(loro::ExportMode::Snapshot)
        .map_err(|e| e.to_string())?;
    let orphan = previous.filter(|old| {
        *old != blob && !binary_files(doc).iter().any(|f| &f.blob == old)
    });
    Ok(BoundFile {
        blob,
        snapshot,
        orphan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> impl FnMut(&[u8]) -> Result<String, String> {
        let mut n = 0;
        move |_| {
            n += 1;
            Ok(format!("b{n}"))
        }
    }
    fn id(b: &'static str) -> impl FnOnce(&[u8]) -> Result<String, String> {
        move |_| Ok(b.into())
    }

    #[test]
    fn identical_bytes_share_one_blob_and_replacing_frees_the_old_one() {
        let doc = LoroDoc::new();
        let mut id = ids();
        let a = bind_binary_file(&doc, "a.glb", b"one", &mut id).unwrap();
        assert!(a.orphan.is_none());
        let b = bind_binary_file(&doc, "b.glb", b"one", |_| panic!("stored twice")).unwrap();
        assert_eq!(b.blob, a.blob);
        // a.glb moves to new bytes; b.glb still holds the old blob, so nothing is orphaned.
        let a2 = bind_binary_file(&doc, "a.glb", b"two", &mut id).unwrap();
        assert!(a2.blob != a.blob && a2.orphan.is_none());
        let b2 = bind_binary_file(&doc, "b.glb", b"two", &mut id).unwrap();
        assert_eq!(b2.orphan, Some(a.blob.clone()));
        let files = binary_files(&doc);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.blob == a2.blob && f.size == 3));
    }

    #[test]
    fn text_and_binary_paths_do_not_mix_and_limits_hold() {
        let doc = LoroDoc::new();
        crate::write_source_file(&doc, "main.lua", "return 1").unwrap();
        assert!(bind_binary_file(&doc, "main.lua", b"x", id("b")).is_err());
        bind_binary_file(&doc, "m.glb", b"x", id("b")).unwrap();
        assert!(crate::write_source_file(&doc, "m.glb", "x").is_err());
        let big = vec![0u8; MAX_BINARY_FILE + 1];
        assert!(bind_binary_file(&doc, "big.bin", &big, id("c")).is_err());
        let failed = bind_binary_file(&doc, "n.glb", b"y", |_| Err("locked".into()));
        assert!(failed.is_err());
        assert_eq!(binary_files(&doc).len(), 1, "a failed store leaves the doc untouched");
    }
}
