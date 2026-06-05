//! The document is the merge point for the editor program *and* the network. These
//! tests stand in for courier: bytes out of one `Doc`, into another, must converge.

use super::*;

#[test]
fn new_doc_has_one_empty_paragraph() {
    let d = Doc::new();
    let ids = d.block_ids();
    assert_eq!(ids.len(), 1);
    assert_eq!(d.kind(ids[0]), BlockKind::Paragraph);
    assert_eq!(d.text(ids[0]), "");
}

#[test]
fn edits_read_back() {
    let d = Doc::new();
    let p = d.block_ids()[0];
    d.insert_text(p, 0, "hello");
    assert_eq!(d.text(p), "hello");
    assert_eq!(d.text_len(p), 5);

    let h = d.create_block(1, BlockKind::H1, "Title");
    let ids = d.block_ids();
    assert_eq!(ids, vec![p, h], "new block lands after the paragraph");
    assert_eq!(d.kind(h), BlockKind::H1);

    d.delete_text(p, 0, 1); // drop the leading 'h'
    assert_eq!(d.text(p), "ello");
    d.set_kind(p, BlockKind::H2);
    assert_eq!(d.kind(p), BlockKind::H2);

    d.delete_block(h);
    assert_eq!(d.block_ids(), vec![p]);
}

#[test]
fn undo_redo_direct() {
    let d = Doc::new();
    let p = d.block_ids()[0];
    d.insert_text(p, 0, "a");
    d.commit();
    assert!(d.undo(), "there is an edit to undo");
    assert_eq!(d.text(p), "", "undo reverts the insert");
    assert!(d.redo(), "there is an undone edit to redo");
    assert_eq!(d.text(p), "a", "redo restores the insert");
}

// --- Raw loro isolation: is the redo bug about tree-nested text? ----------------------

#[test]
fn raw_toplevel_text_undo_redo() {
    use loro::{LoroDoc, UndoManager};
    let doc = LoroDoc::new();
    let mut undo = UndoManager::new(&doc);
    let text = doc.get_text("text");
    text.insert(0, "a").unwrap();
    doc.commit();
    assert!(undo.undo().unwrap(), "undo");
    assert_eq!(text.to_string(), "");
    assert!(undo.redo().unwrap(), "top-level redo");
    assert_eq!(text.to_string(), "a");
}

#[test]
fn raw_tree_nested_text_undo_redo() {
    use loro::{LoroDoc, LoroText, TreeParentId, UndoManager};
    let doc = LoroDoc::new();
    let tree = doc.get_tree("body");
    tree.enable_fractional_index(0);
    let id = tree.create_at(TreeParentId::Root, 0).unwrap();
    // Pre-create the content container BEFORE the UndoManager exists, so the first text
    // edit is a pure text op (not bundled with a nested-container creation).
    let text = tree
        .get_meta(id)
        .unwrap()
        .get_or_create_container("content", LoroText::new())
        .unwrap();
    doc.commit();
    let mut undo = UndoManager::new(&doc);
    text.insert(0, "a").unwrap();
    doc.commit();
    assert!(undo.undo().unwrap(), "undo");
    assert_eq!(text.to_string(), "");
    assert!(undo.redo().unwrap(), "tree-nested redo");
    assert_eq!(text.to_string(), "a");
}

// --- Structural nesting (real tree, not a cosmetic indent number) ---------------------

#[test]
fn indent_outdent_reparent_structurally() {
    let d = Doc::new();
    let a = d.block_ids()[0];
    d.set_kind(a, BlockKind::BulletList);
    let b = d.create_block(1, BlockKind::BulletList, "b");

    // b nests under a (its previous sibling); depth and parent reflect the real tree.
    assert!(d.indent(b), "b has a previous sibling to nest under");
    assert_eq!(d.parent_of(b), Some(a));
    assert_eq!(d.depth(b), 1);
    assert_eq!(d.children(a), vec![b]);
    // Document order is a pre-order DFS: a, then its child b.
    assert_eq!(d.block_ids(), vec![a, b]);
    // The first item has no previous sibling, so it can't indent.
    assert!(!d.indent(a));

    // Outdent lifts b back to the top level, positioned right after a.
    assert!(d.outdent(b));
    assert_eq!(d.parent_of(b), None);
    assert_eq!(d.depth(b), 0);
    assert!(!d.outdent(b), "already top-level — nothing to outdent");
    assert_eq!(d.block_ids(), vec![a, b]);
}

#[test]
fn delete_promotes_children() {
    let d = Doc::new();
    let parent = d.block_ids()[0];
    d.set_kind(parent, BlockKind::BulletList);
    let child = d.create_block(1, BlockKind::BulletList, "child");
    d.indent(child); // child nested under parent
    assert_eq!(d.parent_of(child), Some(parent));

    // Deleting the parent keeps the child — promoted into the parent's slot, one level up.
    d.delete_block(parent);
    assert_eq!(d.block_ids(), vec![child], "child survives the parent's deletion");
    assert_eq!(d.depth(child), 0);
    assert_eq!(d.text(child), "child");
}

#[test]
fn insert_after_keeps_sibling_depth() {
    let d = Doc::new();
    let a = d.block_ids()[0];
    d.set_kind(a, BlockKind::BulletList);
    let b = d.create_block(1, BlockKind::BulletList, "b");
    d.indent(b); // b nested under a, depth 1
    let c = d.insert_after(b, BlockKind::BulletList, "c");
    assert_eq!(d.parent_of(c), Some(a), "c is b's sibling → same parent");
    assert_eq!(d.depth(c), 1);
    assert_eq!(d.block_ids(), vec![a, b, c], "DFS order: a, then its children b, c");
}

#[test]
fn nesting_survives_a_snapshot_roundtrip() {
    let a = Doc::new();
    let root = a.block_ids()[0];
    a.set_kind(root, BlockKind::BulletList);
    let kid = a.create_block(1, BlockKind::BulletList, "nested");
    a.indent(kid);

    let b = Doc::from_snapshot(&a.export_snapshot()).expect("import snapshot");
    let ids = b.block_ids();
    assert_eq!(ids.len(), 2);
    assert_eq!(b.depth(ids[1]), 1, "the child is still nested after a roundtrip");
    assert_eq!(b.parent_of(ids[1]), Some(ids[0]));
}

// --- Inline marks ---------------------------------------------------------------------

#[test]
fn marks_read_back_as_runs() {
    let d = Doc::new();
    let p = d.block_ids()[0];
    d.insert_text(p, 0, "hello world");
    d.mark(p, 0, 5, "bold");
    d.mark(p, 6, 11, "italic");
    // The runs reassemble to the original text...
    assert_eq!(d.runs(p).iter().map(|r| r.text.as_str()).collect::<String>(), "hello world");
    // ...and coverage reflects exactly the marked ranges.
    assert!(d.mark_covers(p, 0, 5, "bold"));
    assert!(!d.mark_covers(p, 0, 6, "bold"), "the trailing space isn't bold");
    assert!(d.mark_covers(p, 6, 11, "italic"));
    assert!(!d.mark_covers(p, 0, 5, "italic"));
}

#[test]
fn unmark_clears_part_of_a_range() {
    let d = Doc::new();
    let p = d.block_ids()[0];
    d.insert_text(p, 0, "abcd");
    d.mark(p, 0, 4, "bold");
    d.unmark(p, 1, 3, "bold"); // punch a hole in the middle
    assert!(d.mark_covers(p, 0, 1, "bold"));
    assert!(!d.mark_covers(p, 1, 3, "bold"));
    assert!(d.mark_covers(p, 3, 4, "bold"));
}

#[test]
fn marks_shift_with_edits_and_survive_snapshot() {
    let a = Doc::new();
    let p = a.block_ids()[0];
    a.insert_text(p, 0, "abcd");
    a.mark(p, 0, 4, "bold");
    // An insertion *inside* the bold run joins it (marks are CRDT-anchored, not offset-keyed).
    a.insert_text(p, 2, "XX"); // "abXXcd"
    assert_eq!(a.text(p), "abXXcd");
    assert!(a.mark_covers(p, 0, 6, "bold"), "an edit inside a bold run stays bold");
    // And the mark survives a persistence roundtrip.
    let b = Doc::from_snapshot(&a.export_snapshot()).unwrap();
    assert!(b.mark_covers(b.block_ids()[0], 0, 6, "bold"));
}

#[test]
fn snapshot_roundtrip_converges() {
    // The persistence path: export a snapshot, load it back into a fresh doc.
    let a = Doc::new();
    let p = a.block_ids()[0];
    a.insert_text(p, 0, "hello");
    a.create_block(1, BlockKind::H1, "Title");

    let b = Doc::from_snapshot(&a.export_snapshot()).expect("import snapshot");
    let ids = b.block_ids();
    assert_eq!(ids.len(), 2);
    assert_eq!(b.text(ids[0]), "hello");
    assert_eq!(b.kind(ids[1]), BlockKind::H1);
    assert_eq!(b.text(ids[1]), "Title");
}

#[test]
fn concurrent_edits_merge() {
    // The network path: two replicas edit different blocks, then exchange snapshots.
    let base = {
        let a = Doc::new();
        a.insert_text(a.block_ids()[0], 0, "A");
        a.export_snapshot()
    };
    let a = Doc::from_snapshot(&base).unwrap();
    let b = Doc::from_snapshot(&base).unwrap();

    a.insert_text(a.block_ids()[0], 1, "X"); // paragraph becomes "AX"
    b.create_block(1, BlockKind::H2, "Z"); // b adds a heading

    a.import(&b.export_snapshot()).unwrap();
    b.import(&a.export_snapshot()).unwrap();

    assert_eq!(a.block_ids().len(), 2);
    assert_eq!(b.block_ids().len(), 2);
    assert_eq!(a.text(a.block_ids()[0]), "AX");
    assert_eq!(b.text(b.block_ids()[0]), "AX");
}
