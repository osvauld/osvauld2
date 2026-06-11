//! Headless tests for the bare block container — no egui, no Lua. These pin the invariants P0
//! promises every layer above relies on: stable ids, ordered insert/move/delete, text edits, CRDT
//! undo/redo (including the eager-container redo gotcha), snapshot round-trip, and two-replica
//! convergence.

use super::*;

/// The kinds + texts of every block, in order — the easy shape to assert on.
fn dump(d: &BlockDoc) -> Vec<(String, String)> {
    d.block_ids().iter().map(|&id| (d.kind(id), d.text(id))).collect()
}

#[test]
fn new_is_empty() {
    let d = BlockDoc::new();
    assert!(d.is_empty());
    assert_eq!(d.len(), 0);
    assert!(d.block_ids().is_empty());
}

#[test]
fn push_appends_in_order() {
    let d = BlockDoc::new();
    d.push("function", "f()");
    d.push("comment", "-- hi");
    d.push("statement", "x = 1");
    assert_eq!(
        dump(&d),
        [
            ("function".into(), "f()".into()),
            ("comment".into(), "-- hi".into()),
            ("statement".into(), "x = 1".into()),
        ]
    );
}

#[test]
fn insert_at_and_after_place_correctly() {
    let d = BlockDoc::new();
    let a = d.push("k", "a");
    let c = d.push("k", "c");
    // insert "b" between a and c, both ways.
    d.insert_at(1, "k", "b");
    d.insert_after(c, "k", "d");
    assert_eq!(
        dump(&d).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    // a and c kept their identity through the insert.
    assert_eq!(d.index_of(a), Some(0));
    assert_eq!(d.index_of(c), Some(2));
}

#[test]
fn insert_at_clamps_past_the_end() {
    let d = BlockDoc::new();
    d.push("k", "a");
    d.insert_at(99, "k", "b");
    assert_eq!(dump(&d).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
fn ids_are_stable_across_edits() {
    let d = BlockDoc::new();
    let id = d.push("k", "hello");
    let other = d.push("k", "world");
    // Insert before, delete the other, edit text — `id` must keep pointing at the same block.
    d.insert_at(0, "k", "first");
    d.delete_block(other);
    d.insert_text(id, 5, " there");
    assert_eq!(d.text(id), "hello there");
    assert_eq!(d.kind(id), "k");
}

#[test]
fn text_edits_by_codepoint() {
    let d = BlockDoc::new();
    let id = d.push("k", "héllo"); // é is one code point
    assert_eq!(d.text_len(id), 5);
    d.insert_text(id, 5, "!");
    d.delete_text(id, 0, 1);
    assert_eq!(d.text(id), "éllo!");
    d.set_block_text(id, "reset");
    assert_eq!(d.text(id), "reset");
}

#[test]
fn move_before_and_after_reorder() {
    let d = BlockDoc::new();
    let a = d.push("k", "a");
    let b = d.push("k", "b");
    let c = d.push("k", "c");
    d.move_after(a, c); // a,b,c -> b,c,a
    assert_eq!(dump(&d).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
    d.move_before(a, b); // -> a,b,c
    assert_eq!(dump(&d).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
}

#[test]
fn delete_removes_only_the_target() {
    let d = BlockDoc::new();
    let a = d.push("k", "a");
    let b = d.push("k", "b");
    d.push("k", "c");
    d.delete_block(b);
    assert_eq!(dump(&d).iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["a", "c"]);
    assert_eq!(d.index_of(a), Some(0));
    assert_eq!(d.index_of(b), None);
}

#[test]
fn kind_and_meta_round_trip() {
    let d = BlockDoc::new();
    let id = d.push("comment", "-- x");
    d.set_kind(id, "function");
    d.set_meta(id, "sep", "\n\n");
    assert_eq!(d.kind(id), "function");
    assert_eq!(d.meta(id, "sep").as_deref(), Some("\n\n"));
    assert_eq!(d.meta(id, "absent"), None);
}

#[test]
fn undo_redo_a_burst_is_one_step() {
    // No checkpoint between the create and the type: the merge interval folds them into a single
    // undo step (the right feel for an uninterrupted burst). One undo removes the whole block; the
    // redo restores the block AND its text — exercising the eager-container redo fix.
    let d = BlockDoc::new();
    d.push("k", "hello");
    d.commit();
    assert_eq!(d.len(), 1);
    assert_eq!(d.undo_count(), 1, "create+type merged into one step");
    assert!(d.undo());
    assert_eq!(d.len(), 0, "the burst undid as a unit");
    assert!(d.redo());
    assert_eq!(d.len(), 1);
    let r = d.block_ids()[0];
    assert_eq!(d.text(r), "hello", "redo restored the block AND its text (eager container)");
}

#[test]
fn contains_distinguishes_gone_from_empty() {
    // The stale-read guard: after delete, the node lingers in the oplog so `text` still returns
    // its old content — `contains` is how a caller tells "gone" from "empty".
    let d = BlockDoc::new();
    let id = d.push("k", "bye");
    assert!(d.contains(id));
    d.delete_block(id);
    assert!(!d.contains(id), "deleted block reports gone");
    assert_eq!(d.index_of(id), None);
}

#[test]
fn construction_is_not_undoable() {
    // The manager attaches after construction, so a fresh empty doc has nothing to undo until the
    // first real edit.
    let d = BlockDoc::new();
    assert!(!d.can_undo(), "nothing before the first edit");
    d.push("k", "x");
    d.commit();
    assert!(d.can_undo());
}

#[test]
fn snapshot_round_trips() {
    let d = BlockDoc::new();
    d.push("function", "local f = 1");
    d.push("comment", "-- note");
    let bytes = d.export_snapshot();

    let d2 = BlockDoc::from_snapshot(&bytes).expect("import snapshot");
    assert_eq!(dump(&d2), dump(&d));
}

#[test]
fn concurrent_rewrite_vs_typing_interleaves() {
    // The agent-race shape: agent rewrites a block (delete-all + insert) from a vault copy while
    // the human types into the live doc. Convergence is guaranteed; the merged TEXT is not what
    // either side wrote. This pins the actual interleaving.
    let a = BlockDoc::new();
    let id = a.push("statement", "local x = 1\n");
    let base = a.export_snapshot();
    let b = BlockDoc::from_snapshot(&base).expect("fork");

    a.insert_text(id, 11, " -- hi"); // human, before the newline
    b.set_block_text(id, "local x = 2\n"); // agent rewrite

    a.import(&b.export_snapshot()).expect("a <- b");
    b.import(&a.export_snapshot()).expect("b <- a");
    assert_eq!(a.text(id), b.text(id), "replicas converge");
    // The human's fragment survives the delete-all and dangles AFTER the replacement line.
    assert_eq!(a.text(id), "local x = 2\n -- hi");
}

#[test]
fn concurrent_block_delete_vs_typing_drops_the_typing() {
    // Agent deletes the block while the human types into it: the delete wins, the typing is gone.
    let a = BlockDoc::new();
    let id = a.push("statement", "old");
    let base = a.export_snapshot();
    let b = BlockDoc::from_snapshot(&base).expect("fork");

    a.insert_text(id, 3, " typed");
    b.delete_block(id);

    a.import(&b.export_snapshot()).expect("a <- b");
    b.import(&a.export_snapshot()).expect("b <- a");
    assert!(!a.contains(id), "delete wins over concurrent typing");
    assert_eq!(a.len(), 0);
}

#[test]
fn nesting_reparents_and_walks_in_dfs_order() {
    let d = BlockDoc::new();
    let a = d.push("k", "a");
    let b = d.push("k", "b");
    let c = d.push("k", "c");

    assert!(d.indent(b), "b nests under its previous sibling a");
    assert_eq!(d.parent_of(b), Some(a));
    assert_eq!(d.depth(b), 1);
    assert_eq!(d.children(a), vec![b]);
    // DFS: a, then its child b, then top-level c — and len counts only the top level.
    assert_eq!(d.block_ids(), vec![a, b, c]);
    assert_eq!(d.blocks(), vec![(a, 0), (b, 1), (c, 0)]);
    assert_eq!(d.len(), 2);
    assert!(!d.indent(a), "first sibling has nothing to nest under");

    assert!(d.outdent(b), "b lifts back out, right after a");
    assert_eq!(d.parent_of(b), None);
    assert_eq!(d.block_ids(), vec![a, b, c]);
    assert!(!d.outdent(b), "already top-level");

    assert!(d.move_into(c, a), "drop-INTO nests c as a's last child");
    assert_eq!(d.children(a), vec![c]);
    assert!(!d.move_into(a, c), "no cycles: a can't move into its own subtree");
    assert!(!d.move_before(a, c) && !d.move_after(a, c));
}

#[test]
fn delete_block_promotes_children() {
    let d = BlockDoc::new();
    let a = d.push("k", "a");
    let b = d.push("k", "b");
    let c = d.push("k", "c");
    d.move_into(b, a);
    d.move_into(c, a);

    d.delete_block(a);
    assert_eq!(d.block_ids(), vec![b, c], "children survive in order, one level up");
    assert_eq!(d.depth(b), 0);
}

#[test]
fn setup_seeds_pre_undo() {
    let d = BlockDoc::new_with(|doc| BlockDoc::seed_if_empty(doc, "paragraph"));
    assert_eq!(d.len(), 1);
    assert_eq!(d.kind(d.block_ids()[0]), "paragraph");
    assert!(!d.can_undo(), "the seed is construction, not an edit");

    // A snapshot that already has blocks must not get a second seed.
    let bytes = d.export_snapshot();
    let d2 = BlockDoc::from_snapshot_with(&bytes, |doc| BlockDoc::seed_if_empty(doc, "paragraph"))
        .expect("import");
    assert_eq!(d2.len(), 1);
}

#[test]
fn two_replicas_converge() {
    // Disjoint concurrent edits on two replicas of the same doc merge to the same state.
    let a = BlockDoc::new();
    a.push("k", "shared");
    let base = a.export_snapshot();
    let b = BlockDoc::from_snapshot(&base).expect("import base");

    a.push("k", "from-a");
    b.push("k", "from-b");

    // Exchange updates (full snapshots are valid CRDT updates).
    a.import(&b.export_snapshot()).expect("a <- b");
    b.import(&a.export_snapshot()).expect("b <- a");

    assert_eq!(dump(&a), dump(&b), "replicas converge");
    let texts: Vec<String> = a.block_ids().iter().map(|&id| a.text(id)).collect();
    assert!(texts.contains(&"from-a".to_string()) && texts.contains(&"from-b".to_string()));
}
