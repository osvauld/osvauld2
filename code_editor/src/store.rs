use block_doc::{BlockDoc, LoroError};

use crate::lua::split;

pub fn doc_from_source(src: &str) -> BlockDoc {
    let doc = BlockDoc::new();
    for block in split(src) {
        doc.push(block.kind.as_str(), &block.text);
    }
    doc
}

pub fn source_from_doc(doc: &BlockDoc) -> String {
    doc.block_ids().iter().map(|&id| doc.text(id)).collect()
}

pub fn snapshot_from_source(src: &str) -> Vec<u8> {
    doc_from_source(src).export_snapshot()
}

pub fn source_from_snapshot(bytes: &[u8]) -> Result<String, LoroError> {
    Ok(source_from_doc(&BlockDoc::from_snapshot(bytes)?))
}

// Load directly into a BlockDoc, preserving existing block identity. Do not go through
// decode + doc_from_source — that re-splits and mints fresh IDs, breaking anchors.
pub fn doc_from_bytes(path: &str, bytes: &[u8]) -> Option<BlockDoc> {
    is_block_file(path).then(|| BlockDoc::from_snapshot(bytes).ok()).flatten()
}

pub fn snapshot_from_doc(doc: &BlockDoc) -> Vec<u8> {
    doc.export_snapshot()
}

pub fn is_block_file(path: &str) -> bool {
    path.ends_with(".lua")
}

pub fn decode(path: &str, bytes: &[u8]) -> String {
    if is_block_file(path) {
        if let Ok(src) = source_from_snapshot(bytes) {
            return src;
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn encode(path: &str, source: &str) -> Vec<u8> {
    if is_block_file(path) {
        snapshot_from_source(source)
    } else {
        source.as_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests;
