//! Compose the per-frame content [`Scene`] from laid-out blocks — the one place the body is
//! turned into primitives, so screen (`paint`) and PDF (`crate::pdf`) can't disagree.
//! Coordinates are column-relative (matching `Placed`); backends add their offset. Bullets are
//! vector shapes (dot/ring/square) so they render without a bullet glyph in the PDF fonts.

use egui::{pos2, vec2, Align2, FontFamily, FontId, Rect, Stroke};

use crate::model::{BlockKind, Doc};
use crate::scene::{BlockPrims, Prim, Scene};
use crate::{block, theme};

use super::Placed;

/// Build the content scene for the laid-out blocks.
pub(super) fn build(doc: &Doc, placed: &[Placed]) -> Scene {
    let mut blocks = Vec::with_capacity(placed.len());
    for p in placed {
        let st = block::block_style(p.kind);
        let text_pos = pos2(p.text_x, p.content_top);
        let mut prims = Vec::new();

        match p.kind {
            BlockKind::Divider => {
                let y = (p.row_top + p.row_bottom) * 0.5;
                prims.push(Prim::Line {
                    from: pos2(p.content_x, y),
                    to: pos2(p.content_right, y),
                    stroke: Stroke::new(1.0, theme::BD),
                });
            }
            BlockKind::Code => {
                let box_rect = Rect::from_min_max(
                    pos2(p.content_x, p.row_top + 4.0),
                    pos2(p.content_right, p.row_bottom - 4.0),
                );
                prims.push(Prim::Rect {
                    rect: box_rect,
                    fill: Some(theme::CODE_BG),
                    stroke: Some(Stroke::new(1.0, theme::HAIR)),
                });
                let lang = doc.lang(p.id).unwrap_or_else(|| "text".into()).to_uppercase();
                prims.push(Prim::Marker {
                    pos: pos2(box_rect.right() - 7.0, box_rect.top() + 4.0),
                    text: lang,
                    font: FontId::new(9.5, FontFamily::Monospace),
                    color: theme::MUTED,
                    align: Align2::RIGHT_TOP,
                });
                prims.push(Prim::Galley { pos: text_pos, galley: p.galley.clone() });
            }
            BlockKind::Quote => {
                let h = p.galley.size().y;
                prims.push(Prim::Line {
                    from: pos2(p.content_x, p.content_top),
                    to: pos2(p.content_x, p.content_top + h),
                    stroke: Stroke::new(2.0, theme::ACCENT),
                });
                prims.push(Prim::Galley { pos: text_pos, galley: p.galley.clone() });
            }
            BlockKind::BulletList => {
                let cx = p.content_x + 7.0;
                let cy = p.content_top + st.line_height * 0.5;
                match p.depth % 3 {
                    0 => prims.push(Prim::Circle {
                        center: pos2(cx, cy),
                        radius: 2.5,
                        fill: Some(theme::FG_2),
                        stroke: None,
                    }),
                    1 => prims.push(Prim::Circle {
                        center: pos2(cx, cy),
                        radius: 3.0,
                        fill: None,
                        stroke: Some(Stroke::new(1.3, theme::FG_2)),
                    }),
                    _ => prims.push(Prim::Rect {
                        rect: Rect::from_center_size(pos2(cx, cy), vec2(4.5, 4.5)),
                        fill: Some(theme::FG_2),
                        stroke: None,
                    }),
                }
                prims.push(Prim::Galley { pos: text_pos, galley: p.galley.clone() });
            }
            BlockKind::NumberedList => {
                prims.push(Prim::Marker {
                    pos: pos2(p.content_x, p.content_top),
                    text: format!("{}.", p.ordinal.unwrap_or(1)),
                    font: FontId::new(16.0, FontFamily::Proportional),
                    color: theme::FG_2,
                    align: Align2::LEFT_TOP,
                });
                prims.push(Prim::Galley { pos: text_pos, galley: p.galley.clone() });
            }
            BlockKind::Todo => {
                let r = Rect::from_min_size(pos2(p.content_x, p.content_top + 3.0), vec2(16.0, 16.0));
                let (fill, border) =
                    if p.done { (Some(theme::ACCENT), theme::ACCENT) } else { (None, theme::BD_HI) };
                prims.push(Prim::Rect { rect: r, fill, stroke: Some(Stroke::new(1.5, border)) });
                if p.done {
                    prims.push(Prim::Path {
                        points: vec![
                            pos2(r.left() + 4.0, r.center().y),
                            pos2(r.left() + 6.5, r.bottom() - 4.0),
                            pos2(r.right() - 3.5, r.top() + 4.5),
                        ],
                        stroke: Stroke::new(2.0, theme::BG_PAGE),
                    });
                }
                prims.push(Prim::Galley { pos: text_pos, galley: p.galley.clone() });
            }
            _ => {
                prims.push(Prim::Galley { pos: text_pos, galley: p.galley.clone() });
            }
        }

        blocks.push(BlockPrims { row_top: p.row_top, row_bottom: p.row_bottom, prims });
    }
    Scene { blocks }
}
