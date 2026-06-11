use std::time::Duration;

use egui::text::CCursor;
use egui::{pos2, vec2, Align2, Event, Key, Rect, Sense, Stroke};

use block_doc::BlockDoc;

use super::{edit, layout, selection, Editor, Theme};

// The host provides the scroll viewport (e.g. an egui_dock tab body); we render straight into `ui`
// and pin the gutter to its clip rect. Nesting our own ScrollArea here would fight the host's.
pub(super) fn show(editor: &mut Editor, ui: &mut egui::Ui, doc: &BlockDoc, theme: &Theme) -> bool {
    let surface_id = ui.id().with("code_surface");

    {
        let laid = layout::layout(ui, doc, theme, &mut editor.hl);
        let galley = laid.galley;
        let blocks = laid.blocks;

        let font = layout::font();
        let line_h = ui.ctx().fonts_mut(|f| f.row_height(&font));
        let digit_w = ui.ctx().fonts_mut(|f| f.glyph_width(&font, '0'));
        let line_count = galley.rows.len().max(1);
        let digits = line_count.to_string().len().max(2);
        let gutter_w = digits as f32 * digit_w + 16.0;

        // Small top inset so line 1 isn't flush against the host's tab strip / top edge.
        const PAD_TOP: f32 = 6.0;
        let total = vec2(gutter_w + galley.size().x + 24.0, galley.size().y + line_h + PAD_TOP);
        let (_, content_rect) = ui.allocate_space(total);
        let origin = content_rect.min + vec2(0.0, PAD_TOP);
        let code_origin = origin + vec2(gutter_w, 0.0);

        let response = ui.interact(content_rect, surface_id, Sense::click_and_drag());
        let now = ui.input(|i| i.time);

        let hit = |p: egui::Pos2| selection::from_global(&blocks, galley.cursor_from_pos(p - code_origin).index);
        if response.clicked() || response.drag_started() {
            response.request_focus();
            if let Some(p) = response.interact_pointer_pos() {
                if let Some(c) = hit(p) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    editor.sel = Some(match (shift, editor.sel) {
                        (true, Some(s)) => selection::Selection { anchor: s.anchor, head: c },
                        _ => selection::Selection::caret(c),
                    });
                    editor.preferred_x = None;
                    editor.blink_origin = now;
                }
            }
        } else if response.dragged() {
            if let (Some(p), Some(s)) = (response.interact_pointer_pos(), editor.sel.as_mut()) {
                if let Some(c) = hit(p) {
                    s.head = c;
                    editor.blink_origin = now;
                }
            }
        }

        if response.has_focus() {
            // Capture arrow keys: egui treats them as focus navigation by default, which would jump
            // focus to the surrounding tab instead of moving the caret
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    surface_id,
                    egui::EventFilter {
                        tab: false,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                )
            });
            if editor.sel.is_none() {
                editor.sel = selection::from_global(&blocks, 0).map(selection::Selection::caret);
            }
            let events = ui.input(|i| i.events.clone());
            let mut edited = false;
            for ev in &events {
                // Copy/cut read the selection across seams; cut's delete obeys the in-block rule.
                if matches!(ev, Event::Copy | Event::Cut) {
                    if let Some(sel) = editor.sel.filter(|s| !s.is_empty()) {
                        let a = selection::to_global(&blocks, sel.anchor);
                        let h = selection::to_global(&blocks, sel.head);
                        let text = edit::extract(doc, &blocks, a.min(h), a.max(h));
                        ui.ctx().copy_text(text);
                        if matches!(ev, Event::Cut) {
                            if let Some(s) = edit::apply(doc, editor.sel, edit::Edit::Backspace) {
                                editor.sel = Some(s);
                                edited = true;
                            }
                        }
                    }
                    continue;
                }
                if let Event::Key { key, pressed: true, modifiers, .. } = ev {
                    let z = *key == Key::Z;
                    if modifiers.command && (z || *key == Key::Y) {
                        let acted = if z && !modifiers.shift { doc.undo() } else { doc.redo() };
                        if acted {
                            editor.sel = None; // ids/offsets may no longer exist
                            editor.dirty = true;
                            ui.ctx().request_repaint();
                        }
                        continue;
                    }
                }
                let edit = match ev {
                    Event::Paste(t) if !t.is_empty() => Some(edit::Edit::Insert(t.clone())),
                    Event::Text(t) if !t.is_empty() => Some(edit::Edit::Insert(t.clone())),
                    Event::Key { key: Key::Enter, pressed: true, .. } => {
                        Some(edit::Edit::Insert("\n".to_owned()))
                    }
                    Event::Key { key: Key::Backspace, pressed: true, .. } => Some(edit::Edit::Backspace),
                    Event::Key { key: Key::Delete, pressed: true, .. } => Some(edit::Edit::Delete),
                    _ => None,
                };
                if let Some(edit) = edit {
                    if let Some(sel) = edit::apply(doc, editor.sel, edit) {
                        editor.sel = Some(sel);
                        editor.preferred_x = None;
                        editor.blink_origin = now;
                        edited = true;
                    }
                    continue;
                }

                let Event::Key { key, pressed: true, modifiers, .. } = ev else { continue };
                let Some(sel) = editor.sel else { continue };
                let global = selection::to_global(&blocks, sel.head);
                let Some(ng) = selection::move_cursor(&galley, global, *key, modifiers, &mut editor.preferred_x)
                else {
                    continue;
                };
                if let Some(c) = selection::from_global(&blocks, ng) {
                    editor.sel = Some(if modifiers.shift {
                        selection::Selection { anchor: sel.anchor, head: c }
                    } else {
                        selection::Selection::caret(c)
                    });
                    editor.blink_origin = now;
                }
            }
            if edited {
                doc.commit();
                editor.dirty = true;
                ui.ctx().request_repaint();
            }
        }

        let painter = ui.painter();
        let clip = ui.clip_rect();
        painter.rect_filled(clip, 0.0, theme.bg);

        if let Some(sel) = editor.sel {
            if !sel.is_empty() {
                let a = selection::to_global(&blocks, sel.anchor);
                let h = selection::to_global(&blocks, sel.head);
                for r in selection::selection_rects(&galley, a.min(h), a.max(h)) {
                    painter.rect_filled(r.translate(code_origin.to_vec2()), 0.0, theme.selection);
                }
            }
        }

        painter.galley(code_origin, galley.clone(), theme.fg);

        // Gutter: pinned to the visible left edge, painted over the code so long lines slide under
        let gutter_x = clip.left();
        let gutter_rect = Rect::from_min_size(pos2(gutter_x, clip.top()), vec2(gutter_w, clip.height()));
        painter.rect_filled(gutter_rect, 0.0, theme.gutter_bg);
        painter.vline(gutter_x + gutter_w - 0.5, gutter_rect.y_range(), Stroke::new(1.0, theme.rule));
        for (i, row) in galley.rows.iter().enumerate() {
            let y = origin.y + row.pos.y;
            if y + line_h < clip.top() || y > clip.bottom() {
                continue;
            }
            painter.text(
                pos2(gutter_x + gutter_w - 8.0, y),
                Align2::RIGHT_TOP,
                i + 1,
                font.clone(),
                theme.gutter_fg,
            );
        }

        if response.has_focus() {
            if let Some(sel) = editor.sel {
                let solid = ((now - editor.blink_origin) * 1.4).fract() < 0.6;
                if solid {
                    let g = selection::to_global(&blocks, sel.head);
                    let cr = galley.pos_from_cursor(CCursor::new(g));
                    let x = code_origin.x + cr.min.x;
                    painter.vline(x, (code_origin.y + cr.min.y)..=(code_origin.y + cr.max.y), Stroke::new(2.0, theme.caret));
                }
            }
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        response.lost_focus()
    }
}
