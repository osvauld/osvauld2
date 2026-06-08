//! The [`DocEditor`] widget: renders a [`Doc`] and drives editing from keyboard/pointer events.
//! The widget does *not* own the document (the shell does, so it can be fed by the network and
//! persisted) — it owns only the caret, anchored to a stable block `TreeID` and re-validated
//! every frame, so a remote delete relocates the caret instead of crashing.
//!
//! Block-row anatomy, left to right:
//! ```text
//!  margin     gutter        spine  content
//! ┌──────┬──────────────┬─┬────────────────────────┐
//! │  H2  │   +   ⋮⋮      │┃│  Conflict resolution    │
//! └──────┴──────────────┴─┴────────────────────────┘
//!  type-tag  affordances  │  lead + text
//! (focus only) (hover/focus) persistent 1px hairline
//! ```
//! The gutter only paints on hover/focus, so the page never reflows. The gutter is pinned to
//! the page margin at every depth; nesting indents only the spine and text column to its right.
//! Each frame: read blocks → lay out → handle pointer+keyboard → re-lay-out if changed → paint.
//! Orchestration + shared state lives here; per-frame work is in the sibling modules.

mod compose;
mod drag;
mod input;
mod langpick;
mod layout;
mod paint;
mod selection;
mod slash;
mod toolbar;

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

/// The code-block **language dropdown**, while open. Anchored to the code block whose tag was
/// clicked; `selected` is the hover/keyboard-highlighted row. Lives off the document.
struct LangPick {
    block: TreeID,
    selected: usize,
}

/// A block laid out for one frame. Coordinates are relative to the allocated rect's top-left;
/// paint/hit-test add `rect.min`. `placed[i]` corresponds to `block_ids()[i]`.
struct Placed {
    id: TreeID,
    kind: BlockKind,
    depth: usize,
    done: bool,
    ordinal: Option<usize>,
    galley: Arc<Galley>,
    /// Left of the fixed gutter (`+` / grip), pinned to the page margin at every depth.
    gutter_left: f32,
    /// The 1px spine at the text-column left; this is what indents with nesting depth.
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

/// A cached shaped galley for one block, reused while content/kind/done/wrap are unchanged, so
/// a clean block costs an `Arc` clone instead of re-shaping. Invalidated by `fp`, a run
/// fingerprint, so a mark that leaves the length unchanged still invalidates.
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
    /// The selection (anchor + head); collapsed (anchor == head) is the caret. `None` until
    /// the first `resolve_caret`.
    sel: Option<Selection>,
    /// Sticky galley-local x for Up/Down movement; cleared by any non-vertical move.
    desired_x: Option<f32>,
    focused_once: bool,
    /// `Some` while the slash command palette is open.
    slash: Option<Slash>,
    /// `Some` while a code block's language dropdown is open.
    lang_pick: Option<LangPick>,
    /// The block being drag-reordered by its grip, while a drag is in progress.
    drag: Option<TreeID>,
    /// Per-block shaped-galley cache, keyed by stable `TreeID`. See [`CachedBlock`].
    layout_cache: HashMap<TreeID, CachedBlock>,
    /// Wall-clock time the caret last moved/edited; the blink phase is measured from here, so
    /// the caret snaps solid on input instead of sitting in its "off" phase.
    blink_origin: f64,
    /// Last frame's caret position, to detect movement and reset the blink.
    last_caret: Option<(TreeID, usize)>,
}

impl DocEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render and edit the document for one frame; returns whether it was mutated (so the
    /// caller can persist). The editor owns its own vertical `ScrollArea`.
    ///
    /// `read_only` gates every mutation (typing, slash menu, cut/paste, undo/redo, affordances)
    /// while leaving selection, copy, navigation, and scrolling live — the published reader.
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

        // Allocate the full content height (so the scrollbar reflects it) but at least the
        // viewport, so clicks below the last block still land in the editor.
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

        // Overlays paint on a foreground layer (no widgets) so the editor never loses focus to
        // them; we hit-test their rects ourselves.
        let slash_menu = self.slash_menu_rect(rect, viewport, &placed);
        let lang_menu = self.lang_menu_rect(rect, viewport, &placed);
        let toolbar = if response.has_focus() {
            self.toolbar_rect(rect, viewport, &placed)
        } else {
            None
        };
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        let pointer_over_menu = matches!((slash_menu, hover_pos), (Some(m), Some(p)) if m.contains(p));
        let pointer_over_toolbar = matches!((toolbar, hover_pos), (Some(t), Some(p)) if t.contains(p));
        let pointer_over_langmenu = matches!((lang_menu, hover_pos), (Some(m), Some(p)) if m.contains(p));
        // Any overlay swallows pointer input meant for the text / gutter beneath it.
        let over_overlay = pointer_over_menu || pointer_over_toolbar || pointer_over_langmenu;

        let hovered = hover_pos
            .filter(|_| !over_overlay)
            .and_then(|p| layout::block_at_y(&placed, rect, p.y));
        let caret_idx = ids.iter().position(|&x| x == self.caret().block);

        let mut changed = false;
        let mut undo_redo = false;
        let mut consumed_click = false;

        // Underlying affordances are inert while the pointer is over an overlay, so a click
        // there can't also toggle a control beneath.
        if !over_overlay {
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

            // Code-block language tags are always interactive (the tag is always shown). A click
            // toggles the language dropdown for that block; clicking the open block's tag closes it.
            for p in placed.iter().filter(|p| p.kind == BlockKind::Code) {
                let r = layout::lang_tag_rect(p, rect);
                let resp = ui
                    .interact(r, response.id.with(("lang", p.id)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() && !read_only {
                    self.lang_pick = match self.lang_pick.take() {
                        Some(lp) if lp.block == p.id => None,
                        _ => Some(LangPick { block: p.id, selected: 0 }),
                    };
                    self.slash = None; // mutually exclusive with the slash palette
                    consumed_click = true;
                    response.request_focus();
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
                // The grip drag-reorders its block. A press that doesn't move is a click
                // (reserved for the block menu); a press + movement is a drag.
                let grip = ui
                    .interact(layout::grip_rect(p, rect), response.id.with(("grip", p.id)), Sense::click_and_drag())
                    .on_hover_cursor(egui::CursorIcon::Grab);
                if grip.drag_started() && !read_only {
                    self.start_drag(p.id);
                }
            }
        }

        // An in-progress block drag: commit the move on release, else keep the ghost animating.
        if self.drag.is_some() {
            if ui.input(|i| i.pointer.any_released()) {
                if self.commit_drag(ui, doc, rect, &placed) {
                    changed = true;
                }
            } else {
                ui.ctx().request_repaint();
            }
        }

        // Palette pointer input: hover moves the highlight, a click applies the item.
        if self.slash.is_some() && self.slash_pointer(ui, doc, rect, viewport, &placed) {
            changed = true;
            consumed_click = true;
        }

        // Language dropdown pointer input: hover highlights a row, a click applies the language.
        if self.lang_pick.is_some() && self.lang_pointer(ui, doc, rect, viewport, &placed) {
            changed = true;
            consumed_click = true;
        }

        // Inline toolbar: a click on a button toggles its mark over the selection.
        if let Some(tb) = toolbar {
            if self.toolbar_pointer(ui, doc, tb) {
                changed = true;
                consumed_click = true;
            }
        }

        // A real pointer click outside the palette dismisses it and places the caret. Gate on
        // a genuine pointer click: `response.clicked()` also fires on Enter/Space (egui's
        // keyboard activation), which would close the palette before the keyboard step applies.
        let pointer_clicked = ui.input(|i| i.pointer.any_click());
        let click_pos = response.interact_pointer_pos();
        let click_over_overlay = matches!((slash_menu, click_pos), (Some(m), Some(p)) if m.contains(p))
            || matches!((toolbar, click_pos), (Some(t), Some(p)) if t.contains(p))
            || matches!((lang_menu, click_pos), (Some(m), Some(p)) if m.contains(p));
        if response.clicked() && pointer_clicked && !consumed_click && !click_over_overlay {
            self.slash = None;
            self.lang_pick = None; // a click elsewhere closes the language dropdown too
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(bi) = layout::block_at_y(&placed, rect, pos.y) {
                    let loc = self.loc_at(&ids, &placed, bi, pos, rect);
                    self.set_caret(loc);
                    self.desired_x = None;
                    response.request_focus();
                }
            }
        }

        // Pointer drag selects a range: the press sets the anchor, dragging moves the head.
        if self.drag.is_none() && !over_overlay && response.drag_started() {
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()) {
                if let Some(bi) = layout::block_at_y(&placed, rect, pos.y) {
                    let loc = self.loc_at(&ids, &placed, bi, pos, rect);
                    self.set_caret(loc);
                    self.desired_x = None;
                    response.request_focus();
                }
            }
        } else if self.drag.is_none() && !over_overlay && response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(bi) = layout::block_at_y(&placed, rect, pos.y) {
                    let loc = self.loc_at(&ids, &placed, bi, pos, rect);
                    self.set_head(loc, true);
                }
            }
        }

        // Keep keyboard focus while an overlay is open, so its keys keep reaching it.
        if self.slash.is_some() || self.lang_pick.is_some() {
            response.request_focus();
        }

        // Capture Tab / arrows / Escape as editing keys instead of letting egui spend them on
        // focus navigation (the lock a `TextEdit` sets); else egui swallows Tab before we see it.
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

        if response.has_focus() || self.slash.is_some() || self.lang_pick.is_some() {
            if self.lang_pick.is_some() {
                // The dropdown owns the keyboard: ↑/↓ move, Enter applies, Esc closes — nothing
                // falls through to editing.
                if self.lang_keyboard(ui, doc) {
                    changed = true;
                }
            } else if self.slash.is_some() {
                // The palette owns the keyboard entirely, so its keys never fall through to
                // editing (Enter applies the item; it never splits the block).
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
                            // Undo/redo are tracked by the UndoManager itself, so they must NOT
                            // go through the per-edit commit below (an extra commit after undo
                            // clears the redo stack) — hence a separate flag.
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
                    self.apply_markdown(doc); // line-start rules (# , - , > …)
                    self.apply_inline_markdown(doc); // **bold**, *italic*, `code`, ~~strike~~
                }
            }
        }

        // If the doc changed, the galleys above are stale — re-read and re-lay-out so the caret
        // and slash anchor match what we paint.
        let placed = if changed || undo_redo {
            // Commit this frame's edits so the UndoManager checkpoints them (bursts within
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
        // Reset the blink phase on any caret move or edit, so the caret stays solid while you
        // type instead of sitting in its "off" phase mid-keystroke (reads as input lag).
        let caret_key = self.sel.map(|s| (s.head.block, s.head.index));
        if changed || undo_redo || caret_key != self.last_caret {
            self.blink_origin = ui.input(|i| i.time);
            self.last_caret = caret_key;
        }

        self.paint(ui, &response, rect, doc, &placed, hovered);

        // Overlays paint last, above the text. Palette and toolbar are mutually exclusive (one
        // wants an empty block, the other a selection); the toolbar rect is recomputed from the
        // final post-edit layout.
        if self.slash.is_some() {
            self.paint_slash(ui, rect, viewport, &placed);
        } else if response.has_focus() {
            if let Some(tb) = self.toolbar_rect(rect, viewport, &placed) {
                self.paint_toolbar(ui, doc, tb, viewport);
            }
        }

        // The language dropdown paints over the text (independent of palette/toolbar).
        if self.lang_pick.is_some() {
            self.paint_lang(ui, doc, rect, viewport, &placed);
        }

        // The drag ghost + drop indicator paint on top of everything while a block is held.
        if self.drag.is_some() {
            self.paint_drag(ui, doc, rect, viewport, &placed);
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

/// Lay out + compose a document into the content [`Scene`](crate::scene) at `width`, using a
/// throwaway galley cache. This is the bridge the PDF export (`crate::pdf`) uses to obtain the
/// *exact* display list the screen renders — same layout, same galleys — so paper can't drift
/// from glass. It needs an `&Ui` only for the egui font system (shaping); nothing is painted.
pub(crate) fn scene_for_export(doc: &Doc, ui: &Ui, width: f32) -> crate::scene::Scene {
    let ids = doc.block_ids();
    let mut cache = HashMap::new();
    let (placed, _height) = layout::layout_all(doc, &ids, ui, width, &mut cache);
    compose::build(doc, &placed)
}
