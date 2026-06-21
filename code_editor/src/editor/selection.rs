use std::ops::Range;

use block_doc::BlockId;
use egui::text::CCursor;
use egui::{Galley, Key, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Caret {
    pub block: BlockId,
    pub offset: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Selection {
    pub anchor: Caret,
    pub head: Caret,
}

impl Selection {
    pub fn caret(head: Caret) -> Self {
        Self { anchor: head, head }
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

pub(super) type Ranges = [(BlockId, Range<usize>)];

pub(super) fn to_global(blocks: &Ranges, c: Caret) -> usize {
    for (id, range) in blocks {
        if *id == c.block {
            return range.start + c.offset.min(range.len());
        }
    }
    // Caret's block is gone (not expected in P3b) — clamp to document end
    blocks.last().map_or(0, |(_, r)| r.end)
}

/// Document end maps to the last block's end. Returns `None` only for an empty document.
pub(super) fn from_global(blocks: &Ranges, g: usize) -> Option<Caret> {
    for (id, range) in blocks {
        if g < range.end {
            return Some(Caret { block: *id, offset: g.saturating_sub(range.start) });
        }
    }
    blocks.last().map(|(id, range)| Caret { block: *id, offset: range.len() })
}

/// `preferred_x` carries the desired column across Up/Down so short lines don't drift the caret.
/// Returns `None` for non-navigation keys so the caller leaves the selection untouched.
pub(super) fn move_cursor(
    galley: &Galley,
    global: usize,
    key: Key,
    modifiers: &Modifiers,
    preferred_x: &mut Option<f32>,
) -> Option<usize> {
    let cursor = CCursor::new(global);
    let new = match key {
        Key::ArrowLeft => {
            *preferred_x = None;
            galley.cursor_left_one_character(&cursor)
        }
        Key::ArrowRight => {
            *preferred_x = None;
            galley.cursor_right_one_character(&cursor)
        }
        Key::ArrowUp => {
            let (c, x) = galley.cursor_up_one_row(&cursor, *preferred_x);
            *preferred_x = x;
            c
        }
        Key::ArrowDown => {
            let (c, x) = galley.cursor_down_one_row(&cursor, *preferred_x);
            *preferred_x = x;
            c
        }
        Key::Home => {
            *preferred_x = None;
            if modifiers.command { galley.begin() } else { galley.cursor_begin_of_row(&cursor) }
        }
        Key::End => {
            *preferred_x = None;
            if modifiers.command { galley.end() } else { galley.cursor_end_of_row(&cursor) }
        }
        _ => return None,
    };
    Some(new.index)
}

#[cfg(test)]
mod tests;
