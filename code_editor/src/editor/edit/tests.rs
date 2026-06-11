//! Tests for the pure text-mutation core. Each builds a real `BlockDoc` (no egui), applies one or
//! more edits, and checks both the per-block text — emit is concatenation, so this *is* the emitted
//! source — and the returned caret. The invariant under test everywhere: P3c only ever rewrites
//! block text; the block COUNT changes solely via the one lazy create on an empty document.

use block_doc::{BlockDoc, BlockId};

use super::{apply, block_ranges, extract, Edit};
use crate::editor::selection::{Caret, Selection};

/// A doc with one "statement" block per text, plus the ids in document order.
fn doc_with(texts: &[&str]) -> (BlockDoc, Vec<BlockId>) {
    let doc = BlockDoc::new();
    for t in texts {
        doc.push("statement", t);
    }
    let ids = doc.block_ids();
    (doc, ids)
}

fn at(ids: &[BlockId], i: usize, offset: usize) -> Caret {
    Caret { block: ids[i], offset }
}

fn caret_sel(ids: &[BlockId], i: usize, offset: usize) -> Option<Selection> {
    Some(Selection::caret(at(ids, i, offset)))
}

#[test]
fn insert_lands_at_the_caret() {
    let (doc, ids) = doc_with(&["local x = 1"]);
    let out = apply(&doc, caret_sel(&ids, 0, 5), Edit::Insert("y".into())).unwrap();
    assert_eq!(doc.text(ids[0]), "localy x = 1");
    assert_eq!(out.head, at(&ids, 0, 6));
    assert!(out.is_empty(), "an edit collapses the selection to a caret");
    assert_eq!(doc.len(), 1, "text insert never changes the block count");
}

#[test]
fn insert_replaces_a_selection() {
    let (doc, ids) = doc_with(&["abcdef"]);
    let sel = Some(Selection { anchor: at(&ids, 0, 1), head: at(&ids, 0, 4) });
    let out = apply(&doc, sel, Edit::Insert("X".into())).unwrap();
    assert_eq!(doc.text(ids[0]), "aXef");
    assert_eq!(out.head, at(&ids, 0, 2));
}

#[test]
fn newline_does_not_split_in_p3c() {
    let (doc, ids) = doc_with(&["ab"]);
    let out = apply(&doc, caret_sel(&ids, 0, 1), Edit::Insert("\n".into())).unwrap();
    assert_eq!(doc.text(ids[0]), "a\nb");
    assert_eq!(out.head, at(&ids, 0, 2));
    assert_eq!(doc.len(), 1, "Enter is a plain newline; the re-split is P3d");
}

#[test]
fn backspace_deletes_one_code_point_before_the_caret() {
    let (doc, ids) = doc_with(&["abc"]);
    let out = apply(&doc, caret_sel(&ids, 0, 2), Edit::Backspace).unwrap();
    assert_eq!(doc.text(ids[0]), "ac");
    assert_eq!(out.head, at(&ids, 0, 1));
}

#[test]
fn backspace_with_a_selection_deletes_the_selection() {
    let (doc, ids) = doc_with(&["abcdef"]);
    let sel = Some(Selection { anchor: at(&ids, 0, 1), head: at(&ids, 0, 4) });
    let out = apply(&doc, sel, Edit::Backspace).unwrap();
    assert_eq!(doc.text(ids[0]), "aef");
    assert_eq!(out.head, at(&ids, 0, 1));
}

#[test]
fn backspace_at_block_start_eats_the_previous_block_tail() {
    // C1: the caret sits at offset 0 of block 1; the seam newline is the last char of block 0.
    // Backspace reaches across the seam via the global mapping and removes it — both blocks survive.
    let (doc, ids) = doc_with(&["foo\n", "bar"]);
    let out = apply(&doc, caret_sel(&ids, 1, 0), Edit::Backspace).unwrap();
    assert_eq!(doc.text(ids[0]), "foo");
    assert_eq!(doc.text(ids[1]), "bar");
    assert_eq!(doc.len(), 2);
    assert_eq!(out.head, at(&ids, 0, 3));
}

#[test]
fn delete_forward_reaches_across_the_seam() {
    // Caret at the end of block 0 (canonicalised to the start of block 1); forward-delete removes
    // the first code point of block 1.
    let (doc, ids) = doc_with(&["foo\n", "bar"]);
    let out = apply(&doc, caret_sel(&ids, 0, 4), Edit::Delete).unwrap();
    assert_eq!(doc.text(ids[0]), "foo\n");
    assert_eq!(doc.text(ids[1]), "ar");
    assert_eq!(out.head, at(&ids, 1, 0));
}

#[test]
fn cross_block_selection_edits_are_refused() {
    // Scope decision 2026-06-11: humans edit inside one block, structure is the agent's job — an
    // edit whose selection spans blocks is a no-op (no zombie emptied blocks, no boundary damage).
    let (doc, ids) = doc_with(&["aaa", "\n\n", "bbb"]);
    let sel = Some(Selection { anchor: at(&ids, 0, 1), head: at(&ids, 2, 2) });
    assert!(apply(&doc, sel, Edit::Insert("X".into())).is_none());
    assert!(apply(&doc, sel, Edit::Backspace).is_none());
    assert!(apply(&doc, sel, Edit::Delete).is_none());
    assert_eq!(doc.text(ids[0]), "aaa");
    assert_eq!(doc.text(ids[1]), "\n\n");
    assert_eq!(doc.text(ids[2]), "bbb");
}

#[test]
fn extract_reads_across_seams() {
    // Copy is read-only, so unlike edits it crosses blocks freely.
    let (doc, _) = doc_with(&["foo\n", "bar"]);
    let ranges = block_ranges(&doc);
    assert_eq!(extract(&doc, &ranges, 2, 6), "o\nba");
    assert_eq!(extract(&doc, &ranges, 0, 7), "foo\nbar");
    assert_eq!(extract(&doc, &ranges, 3, 3), "");
}

#[test]
fn g1_first_insert_into_an_empty_doc_creates_a_block() {
    let doc = BlockDoc::new();
    assert!(doc.is_empty());
    let out = apply(&doc, None, Edit::Insert("l".into())).unwrap();
    assert_eq!(doc.len(), 1);
    let id = doc.block_ids()[0];
    assert_eq!(doc.text(id), "l");
    assert_eq!(doc.kind(id), "statement");
    assert_eq!(out.head, Caret { block: id, offset: 1 });
}

#[test]
fn g1_delete_on_an_empty_doc_is_a_noop() {
    let doc = BlockDoc::new();
    assert!(apply(&doc, None, Edit::Backspace).is_none());
    assert!(apply(&doc, None, Edit::Delete).is_none());
    assert!(doc.is_empty());
}

#[test]
fn a_burst_of_inserts_stays_consistent() {
    // Each call recomputes ranges from the doc, so chained edits (as a single frame delivers) never
    // address stale offsets. The result is exactly the typed text — the emit invariant under churn.
    let (doc, ids) = doc_with(&["x"]);
    let mut sel = caret_sel(&ids, 0, 1);
    for ch in ["a", "b", "c"] {
        sel = apply(&doc, sel, Edit::Insert(ch.into()));
    }
    assert_eq!(doc.text(ids[0]), "xabc");
    assert_eq!(sel.unwrap().head, at(&ids, 0, 4));
}

#[test]
fn offsets_are_code_points_not_bytes() {
    // "héllo" — backspace at offset 2 removes the 'é' (one code point, two bytes), not a byte.
    let (doc, ids) = doc_with(&["héllo"]);
    let out = apply(&doc, caret_sel(&ids, 0, 2), Edit::Backspace).unwrap();
    assert_eq!(doc.text(ids[0]), "hllo");
    assert_eq!(out.head, at(&ids, 0, 1));
}
