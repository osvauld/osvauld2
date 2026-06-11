//! Tests for the pure caret ⇄ global-offset mapping — the load-bearing logic that lets navigation
//! and hit-testing work in the combined galley's global char space while the model caret stays
//! `(BlockId, offset)`. The galley-dependent pieces (`move_cursor`, `selection_rects`) are exercised
//! by running the widget; here we pin the mapping that everything else rests on.

use std::ops::Range;

use block_doc::{BlockDoc, BlockId};

use super::{from_global, to_global, Caret};

/// Build a doc with the given block texts and the matching char ranges (mirroring `layout`).
fn doc_with(texts: &[&str]) -> (BlockDoc, Vec<(BlockId, Range<usize>)>) {
    let doc = BlockDoc::new();
    for t in texts {
        doc.push("statement", t);
    }
    let mut ranges = Vec::new();
    let mut base = 0usize;
    for id in doc.block_ids() {
        let n = doc.text(id).chars().count();
        ranges.push((id, base..base + n));
        base += n;
    }
    (doc, ranges)
}

#[test]
fn every_global_offset_round_trips_through_a_caret() {
    // `from_global` is the canonicaliser: the caret it returns for `g` always maps back to `g`.
    let (_doc, ranges) = doc_with(&["local x = 1", "\n", "print(x)"]);
    let total = ranges.last().unwrap().1.end;
    for g in 0..=total {
        let caret = from_global(&ranges, g).expect("non-empty doc");
        assert_eq!(to_global(&ranges, caret), g, "global {g}");
    }
}

#[test]
fn canonical_carets_round_trip() {
    // A caret at the end of a *non-last* block sits at the same position as the start of the next
    // block; `from_global` canonicalises that to the later block, so only offsets strictly inside a
    // non-last block — and any offset of the last block, up to its end — are their own canonical form.
    let (_doc, ranges) = doc_with(&["local x = 1", "\n", "print(x)"]);
    let last = ranges.len() - 1;
    for (i, (id, range)) in ranges.iter().enumerate() {
        let hi = if i == last { range.len() } else { range.len() - 1 };
        for offset in 0..=hi {
            let caret = Caret { block: *id, offset };
            assert_eq!(from_global(&ranges, to_global(&ranges, caret)), Some(caret), "block {i} offset {offset}");
        }
    }
}

#[test]
fn to_global_is_block_start_plus_offset() {
    let (_doc, ranges) = doc_with(&["abc", "de", "f"]);
    // ranges: [0..3, 3..5, 5..6]
    assert_eq!(to_global(&ranges, Caret { block: ranges[0].0, offset: 0 }), 0);
    assert_eq!(to_global(&ranges, Caret { block: ranges[1].0, offset: 1 }), 4);
    assert_eq!(to_global(&ranges, Caret { block: ranges[2].0, offset: 1 }), 6);
}

#[test]
fn block_boundary_resolves_to_start_of_later_block() {
    let (_doc, ranges) = doc_with(&["abc", "def"]);
    // Global 3 is both end-of-block-0 and start-of-block-1; the half-open `[start, end)` rule picks
    // the later block at offset 0 — so a caret there belongs to a stable, editable block.
    assert_eq!(from_global(&ranges, 3), Some(Caret { block: ranges[1].0, offset: 0 }));
}

#[test]
fn document_end_maps_to_end_of_last_block() {
    let (_doc, ranges) = doc_with(&["abc", "def"]);
    let total = 6;
    assert_eq!(from_global(&ranges, total), Some(Caret { block: ranges[1].0, offset: 3 }));
    // Past the end clamps the same way.
    assert_eq!(from_global(&ranges, 99), Some(Caret { block: ranges[1].0, offset: 3 }));
}

#[test]
fn to_global_clamps_an_overshooting_offset() {
    let (_doc, ranges) = doc_with(&["abc", "def"]);
    // An offset past the block length clamps to the block's end, never bleeding into the next block.
    assert_eq!(to_global(&ranges, Caret { block: ranges[0].0, offset: 99 }), 3);
}

#[test]
fn from_global_skips_an_emptied_block() {
    // Load-bearing now that P3c deletes can empty a block in place (e.g. a wiped seam): the caret
    // must never strand inside a zero-length block. `from_global`'s half-open `[start, end)` test
    // skips an empty range (`start == end`) and resolves to the next real block.
    let (_doc, ranges) = doc_with(&["abc", "", "def"]);
    assert_eq!(ranges[1].1, 3..3, "the middle block is empty");
    let c = from_global(&ranges, 3).expect("non-empty doc");
    assert_ne!(c.block, ranges[1].0, "never the emptied block");
    assert_eq!(c, Caret { block: ranges[2].0, offset: 0 });
}

#[test]
fn empty_document_has_no_caret() {
    let ranges: Vec<(BlockId, Range<usize>)> = Vec::new();
    assert_eq!(from_global(&ranges, 0), None);
}

#[test]
fn multibyte_offsets_are_code_points_not_bytes() {
    // "héllo" is 5 code points / 6 bytes; the mapping is in code points so a caret after "hé" is
    // global 2, not 3.
    let (_doc, ranges) = doc_with(&["héllo", "x"]);
    assert_eq!(ranges[0].1, 0..5);
    assert_eq!(to_global(&ranges, Caret { block: ranges[0].0, offset: 2 }), 2);
    assert_eq!(from_global(&ranges, 5), Some(Caret { block: ranges[1].0, offset: 0 }));
}
