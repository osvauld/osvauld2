use std::ops::Range;

use block_doc::{BlockDoc, BlockId};

use super::selection::{from_global, to_global, Caret, Selection};

pub(super) enum Edit {
    Insert(String),
    Backspace,
    Delete,
}

pub(super) fn apply(doc: &BlockDoc, sel: Option<Selection>, edit: Edit) -> Option<Selection> {
    // G1: empty document has no block — first inserted text lazily creates one
    if doc.is_empty() {
        let Edit::Insert(s) = edit else { return None };
        let id = doc.push("statement", &s);
        return Some(Selection::caret(Caret { block: id, offset: s.chars().count() }));
    }

    let blocks = block_ranges(doc);
    let sel = sel.or_else(|| from_global(&blocks, 0).map(Selection::caret))?;
    // Humans edit inside one block; structure is the agent's job — cross-block edits no-op.
    if sel.anchor.block != sel.head.block {
        return None;
    }
    let a = to_global(&blocks, sel.anchor);
    let h = to_global(&blocks, sel.head);
    let (mut lo, mut hi) = (a.min(h), a.max(h));

    let inserted: &str = match &edit {
        Edit::Insert(s) => s,
        Edit::Backspace => {
            if lo == hi {
                lo = lo.saturating_sub(1);
            }
            ""
        }
        Edit::Delete => {
            if lo == hi {
                hi += 1;
            }
            ""
        }
    };

    let head = replace_range(doc, &blocks, lo, hi, inserted);
    Some(Selection::caret(head))
}

/// Blocks that fall entirely inside the span are emptied but kept — identity survives, re-splitting
/// is P3d's job. The seam at `lo` resolves to the start of the later block (matching from_global).
fn replace_range(
    doc: &BlockDoc,
    blocks: &[(BlockId, Range<usize>)],
    lo: usize,
    hi: usize,
    inserted: &str,
) -> Caret {
    let total = blocks.last().map_or(0, |(_, r)| r.end);
    let lo = lo.min(total);
    let hi = hi.min(total).max(lo);

    let target = from_global(blocks, lo).expect("non-empty document");
    for (id, range) in blocks {
        let a = lo.max(range.start);
        let b = hi.min(range.end);
        if a < b {
            doc.delete_text(*id, a - range.start, b - a);
        }
    }
    doc.insert_text(target.block, target.offset, inserted);
    Caret { block: target.block, offset: target.offset + inserted.chars().count() }
}

/// The text of the global char span `[lo, hi)` — clipboard reads cross seams freely.
pub(super) fn extract(doc: &BlockDoc, blocks: &[(BlockId, Range<usize>)], lo: usize, hi: usize) -> String {
    let mut out = String::new();
    for (id, range) in blocks {
        let a = lo.max(range.start);
        let b = hi.min(range.end);
        if a < b {
            out.extend(doc.text(*id).chars().skip(a - range.start).take(b - a));
        }
    }
    out
}

// Read fresh from the doc — not from the frame's layout snapshot — so several edits in one
// frame each see the true current offsets (stale offsets risk an out-of-range Loro panic)
fn block_ranges(doc: &BlockDoc) -> Vec<(BlockId, Range<usize>)> {
    let mut ranges = Vec::new();
    let mut base = 0usize;
    for id in doc.block_ids() {
        let end = base + doc.text_len(id);
        ranges.push((id, base..end));
        base = end;
    }
    ranges
}

#[cfg(test)]
mod tests;
