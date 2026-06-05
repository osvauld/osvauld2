//! The [`DocEditor`] widget: renders a [`Doc`] in the Osvauld block-editor design and
//! drives editing from raw keyboard / pointer events.
//!
//! The widget does **not** own the document — the shell does, so the document can also be
//! fed by the network (courier) and persisted to the vault. The widget owns only the
//! **caret**, which is anchored to a stable block `TreeID`; every frame it re-reads the
//! tree and re-validates the caret, so a block deleted by a remote peer simply relocates
//! the caret instead of crashing.
//!
//! **Block-row anatomy** (per the design handoff), left to right:
//! ```text
//!  margin     gutter        spine  content
//! ┌──────┬──────────────┬─┬────────────────────────┐
//! │  H2  │   +   ⋮⋮      │┃│  Conflict resolution    │
//! └──────┴──────────────┴─┴────────────────────────┘
//!  type-tag  affordances  │  lead + text
//! (focus only) (hover/focus) persistent 1px hairline
//! ```
//! The gutter only paints on hover/focus, so the resting page stays writerly and layout
//! never reflows. The gutter (and the focus type-tag) are **pinned to the page margin** — a
//! single fixed column at every depth; nesting indents only the spine and the text column to
//! its right (so the affordances never "march" rightward as items nest). Each frame: read
//! blocks → lay out (geometry + galleys) → handle pointer + keyboard (applying Loro ops) →
//! re-lay-out if changed → paint.
//!
//! This file is the **orchestration + shared state**; the per-frame work lives in siblings:
//! `layout` (geometry + galleys), `paint` (rows + caret), `input` (keys + editing),
//! `slash` (the command palette), and `selection` (ranges — landing next).

mod input;
mod layout;
mod paint;
mod selection;
mod slash;

use std::collections::HashMap;
use std::sync::Arc;

use egui::{CornerRadius, Event, Galley, Key, Pos2, Rect, Sense, Ui, Vec2};
use loro::TreeID;

use crate::model::{BlockKind, Doc};
use crate::theme;

use selection::Selection;

/// A position in the document: a block (by stable id) and a character (code-point) offset
/// within its text. Also serves as a selection endpoint (anchor / head).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Caret {
    block: TreeID,
    index: usize,
}

/// The slash command palette, while open. Anchored to the block it was triggered in; the
/// `/` and the typed query live here, never in the document.
struct Slash {
    block: TreeID,
    query: String,
    selected: usize,
}

/// A block laid out for one frame. All coordinates are **relative to the allocated
/// rect's top-left**; paint/hit-test add `rect.min`. `placed[i]` corresponds to
/// `block_ids()[i]`.
struct Placed {
    id: TreeID,
    kind: BlockKind,
    depth: usize,
    done: bool,
    ordinal: Option<usize>,
    galley: Arc<Galley>,
    /// Left of the fixed gutter (`+` / grip) — pinned to the page margin, the same for
    /// every block regardless of nesting depth.
    gutter_left: f32,
    /// The 1px spine at the text-column left; this is what *indents* with nesting depth.
    spine_x: f32,
    /// Left of the text column (after the spine + pad), before the lead.
    content_x: f32,
    /// Right edge of the text column.
    content_right: f32,
    /// Galley origin x (after the lead marker).
    text_x: f32,
    /// Galley origin y.
    content_top: f32,
    row_top: f32,
    row_bottom: f32,
}

/// A cached shaped galley for one block. Reused across frames while the block's styled
/// content, kind, to-do state, and wrap width are unchanged — so a clean block costs an `Arc`
/// clone instead of a `LayoutJob` build + text shaping. Invalidation is by `fp`, a fingerprint
/// of the block's runs (text + marks), so a mark that leaves the length unchanged still
/// invalidates. (Computing the runs each frame to take the fingerprint is the documented cost
/// to revisit — switch to Loro's `doc.diff` — once docs get large.)
struct CachedBlock {
    fp: u64,
    kind: BlockKind,
    done: bool,
    wrap: f32,
    galley: Arc<Galley>,
    height: f32,
}

#[derive(Default)]
pub struct DocEditor {
    /// The selection (anchor + head). A collapsed selection (anchor == head) is the caret.
    /// `None` until the first `resolve_caret`. See `selection.rs`.
    sel: Option<Selection>,
    /// Sticky galley-local x for Up/Down movement; cleared by any non-vertical move.
    desired_x: Option<f32>,
    focused_once: bool,
    /// `Some` while the slash command palette is open.
    slash: Option<Slash>,
    /// Per-block shaped-galley cache, keyed by stable `TreeID`. See [`CachedBlock`].
    layout_cache: HashMap<TreeID, CachedBlock>,
    /// Wall-clock time the caret last moved/edited — the blink phase is measured from here,
    /// so the caret snaps solid on input instead of possibly sitting in its "off" phase.
    blink_origin: f64,
    /// Last frame's caret position, to detect movement and reset the blink.
    last_caret: Option<(TreeID, usize)>,
}

impl DocEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render and edit the document for one frame. Returns whether the document was
    /// mutated this frame (so the caller can persist it). The body scrolls vertically — the
    /// editor owns its own `ScrollArea`, so it behaves identically in the shell's app cell
    /// and the standalone runner.
    ///
    /// `read_only` (driven by the viewer's permit) gates every *mutation* — typing, the
    /// slash menu, cut/paste, undo/redo, the `+`/checkbox affordances — while leaving
    /// selection, copy, caret navigation, and scrolling live. This is the **reader** that
    /// `.book` / `.blog` published views use; it's the same widget with edit off.
    pub fn show(&mut self, ui: &mut Ui, doc: &Doc, read_only: bool) -> bool {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| self.show_frame(ui, doc, read_only))
            .inner
    }

    fn show_frame(&mut self, ui: &mut Ui, doc: &Doc, read_only: bool) -> bool {
        let ids = doc.block_ids();
        self.resolve_caret(doc, &ids);
        // Drop a stale slash palette if its block vanished (e.g. a remote delete).
        if self.slash.as_ref().is_some_and(|s| !ids.contains(&s.block)) {
            self.slash = None;
        }

        // The visible region of the scroll area — the "cell" overlays clamp/flip within.
        let viewport = ui.clip_rect();
        let width = ui.available_rect_before_wrap().width();
        let (placed, column_height) = layout::layout_all(doc, &ids, ui, width, &mut self.layout_cache);

        // Allocate the full content height so the scrollbar reflects it; fill at least the
        // viewport so clicks below the last block still land in the editor.
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, column_height.max(viewport.height())),
            Sense::click_and_drag(),
        );
        if !self.focused_once {
            response.request_focus();
            self.focused_once = true;
        }
        // Page background fills the visible viewport (fixed behind the scrolling text).
        ui.painter().rect_filled(viewport, CornerRadius::same(0), theme::BG_PAGE);

        // The palette is painted on a foreground layer (no widgets), so the editor never
        // loses focus to it. We hit-test pointer events against its rect ourselves.
        let slash_menu = self.slash_menu_rect(rect, viewport, &placed);
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        let pointer_over_menu = matches!((slash_menu, hover_pos), (Some(m), Some(p)) if m.contains(p));

        let hovered = hover_pos
            .filter(|_| !pointer_over_menu)
            .and_then(|p| layout::block_at_y(&placed, rect, p.y));
        let caret_idx = ids.iter().position(|&x| x == self.caret().block);

        let mut changed = false;
        let mut undo_redo = false;
        let mut consumed_click = false;

        // Underlying gutter / checkbox affordances are inert while the pointer is over the
        // palette, so a click on the palette can't also toggle a control beneath it.
        if !pointer_over_menu {
            // To-do checkboxes are always interactive (visible without hover).
            for p in placed.iter().filter(|p| p.kind == BlockKind::Todo) {
                let r = layout::checkbox_rect(p, rect);
                let resp = ui
                    .interact(r, response.id.with(("chk", p.id)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() && !read_only {
                    doc.set_done(p.id, !p.done);
                    changed = true;
                    consumed_click = true;
                }
            }

            // Gutter affordances are live only where the gutter is visible: the hovered row
            // and the caret's row. A plain `+` click inserts an empty paragraph below.
            let mut active: Vec<usize> = Vec::new();
            if let Some(h) = hovered {
                active.push(h);
            }
            if let Some(c) = caret_idx {
                if !active.contains(&c) {
                    active.push(c);
                }
            }
            for &i in &active {
                let p = &placed[i];
                let plus = ui
                    .interact(layout::plus_rect(p, rect), response.id.with(("plus", p.id)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if plus.clicked() && !read_only {
                    let new = doc.insert_after(p.id, BlockKind::Paragraph, "");
                    self.set_caret(Caret { block: new, index: 0 });
                    self.desired_x = None;
                    changed = true;
                    consumed_click = true;
                    response.request_focus();
                }
                // The grip is paint-only for now (drag-reorder + block menu are a later
                // phase); claim hover so it shows the grab cursor.
                ui.interact(layout::grip_rect(p, rect), response.id.with(("grip", p.id)), Sense::hover())
                    .on_hover_cursor(egui::CursorIcon::Grab);
            }
        }

        // Palette pointer input: hover moves the highlight, a click applies the item.
        if self.slash.is_some() && self.slash_pointer(ui, doc, rect, viewport, &placed) {
            changed = true;
            consumed_click = true;
        }

        // A real *pointer* click outside the palette dismisses it and places the caret.
        // `response.clicked()` also fires when Enter/Space activates the focused editor
        // (egui's keyboard activation of a clickable widget) — that must NOT count as a
        // click here, or it would close the palette the instant before the keyboard step
        // could apply the highlighted item. Gate on a genuine pointer click.
        let pointer_clicked = ui.input(|i| i.pointer.any_click());
        let click_over_menu = matches!(
            (slash_menu, response.interact_pointer_pos()),
            (Some(m), Some(p)) if m.contains(p)
        );
        if response.clicked() && pointer_clicked && !consumed_click && !click_over_menu {
            self.slash = None;
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(bi) = layout::block_at_y(&placed, rect, pos.y) {
                    let loc = self.loc_at(&ids, &placed, bi, pos, rect);
                    self.set_caret(loc);
                    self.desired_x = None;
                    response.request_focus();
                }
            }
        }

        // Pointer drag selects a range: the press sets the anchor (a collapsed caret at the
        // press point), then dragging moves the head to grow the selection.
        if !pointer_over_menu && response.drag_started() {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(bi) = layout::block_at_y(&placed, rect, pos.y) {
                    let loc = self.loc_at(&ids, &placed, bi, pos, rect);
                    self.set_caret(loc);
                    self.desired_x = None;
                    response.request_focus();
                }
            }
        } else if !pointer_over_menu && response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(bi) = layout::block_at_y(&placed, rect, pos.y) {
                    let loc = self.loc_at(&ids, &placed, bi, pos, rect);
                    self.set_head(loc, true);
                }
            }
        }

        // Keep keyboard focus while the palette is open, so typing keeps filtering it.
        if self.slash.is_some() {
            response.request_focus();
        }

        // Capture Tab / arrows / Escape as *editing* keys rather than letting egui spend them
        // on focus navigation (the same lock a `TextEdit` sets). Without this, egui's focus
        // system swallows Tab before our event loop ever sees it, so indent/outdent never fire.
        if response.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }

        if response.has_focus() || self.slash.is_some() {
            if self.slash.is_some() {
                // The palette owns the keyboard entirely — its keys can never fall through
                // to editing (so Enter applies the item; it never splits the block).
                if self.slash_keyboard(ui, doc) {
                    changed = true;
                }
            } else {
                let events = ui.input(|i| i.events.clone());
                let mut inserted = false;
                for ev in &events {
                    match ev {
                        Event::Text(t) => {
                            if read_only {
                                // no insertion in a reader
                            } else if self.try_open_slash(doc, t) {
                                // consumed: `/` opened the palette instead of being inserted
                            } else if self.insert_text(doc, t) {
                                changed = true;
                                inserted = true;
                            }
                        }
                        Event::Key { key, pressed: true, modifiers, .. } => {
                            // Undo/redo are committed and tracked by the UndoManager itself,
                            // so they must NOT go through the per-edit commit below (an extra
                            // commit after an undo clears the redo stack) — hence a separate
                            // flag. `command` = Ctrl on Linux/Windows, Cmd on macOS.
                            if modifiers.command && matches!(key, Key::Z | Key::Y) {
                                if !read_only {
                                    let did = if *key == Key::Z && !modifiers.shift {
                                        doc.undo()
                                    } else {
                                        doc.redo()
                                    };
                                    undo_redo |= did;
                                }
                            } else if self.handle_key(doc, *key, *modifiers, &placed, read_only) {
                                changed = true;
                            }
                        }
                        // Copy is a read — allowed in a reader. Cut/paste mutate.
                        Event::Copy => self.copy_to_clipboard(ui, doc),
                        Event::Cut => {
                            if !read_only && self.cut(ui, doc) {
                                changed = true;
                            }
                        }
                        Event::Paste(t) => {
                            if !read_only && self.paste(doc, t) {
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
                if inserted {
                    self.apply_markdown(doc);
                }
            }
        }

        // If the doc changed, the galleys above are stale — re-read and re-lay-out so the
        // caret (and the slash anchor) match what we paint. (Docs are short; this is cheap.)
        let placed = if changed || undo_redo {
            // Commit this frame's *edits* so the UndoManager checkpoints them (bursts within
            // `UNDO_MERGE_MS` fold into one step). Undo/redo must NOT be committed here.
            if changed {
                doc.commit();
            }
            let ids = doc.block_ids();
            self.resolve_caret(doc, &ids);
            layout::layout_all(doc, &ids, ui, width, &mut self.layout_cache).0
        } else {
            placed
        };
        // Reset the blink phase whenever the caret moves or the doc changes, so the caret
        // stays solid while you type (it otherwise blinks on a wall clock and can sit in its
        // "off" phase mid-keystroke — which reads as input lag / a skipping cursor).
        let caret_key = self.sel.map(|s| (s.head.block, s.head.index));
        if changed || undo_redo || caret_key != self.last_caret {
            self.blink_origin = ui.input(|i| i.time);
            self.last_caret = caret_key;
        }

        self.paint(ui, &response, rect, doc, &placed, hovered);

        // The slash palette paints last, on a foreground layer above the text.
        if self.slash.is_some() {
            self.paint_slash(ui, rect, viewport, &placed);
        }
        changed || undo_redo
    }

    // --- Caret bookkeeping ------------------------------------------------------------

    /// Ensure both selection endpoints point at blocks that still exist, with in-range
    /// offsets (a remote delete relocates an endpoint to the document start rather than
    /// crashing). Initialises a collapsed caret on the first frame.
    fn resolve_caret(&mut self, doc: &Doc, ids: &[TreeID]) {
        let fix = |loc: Caret| {
            if ids.contains(&loc.block) {
                Caret { block: loc.block, index: loc.index.min(doc.text_len(loc.block)) }
            } else {
                Caret { block: ids[0], index: 0 }
            }
        };
        self.sel = Some(match self.sel {
            Some(s) => Selection { anchor: fix(s.anchor), head: fix(s.head) },
            None => Selection::caret(Caret { block: ids[0], index: 0 }),
        });
    }

    /// The caret position under a pointer `pos` within block index `bi` (a divider has no
    /// text, so its caret sits at offset 0).
    fn loc_at(&self, ids: &[TreeID], placed: &[Placed], bi: usize, pos: Pos2, rect: Rect) -> Caret {
        let p = &placed[bi];
        if p.kind == BlockKind::Divider {
            return Caret { block: ids[bi], index: 0 };
        }
        let origin = Pos2::new(rect.left() + p.text_x, rect.top() + p.content_top);
        let cc = p.galley.cursor_from_pos(pos - origin);
        Caret { block: ids[bi], index: cc.index }
    }
}
