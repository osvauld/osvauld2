//! Renders **uploadable apps** as a homegrown declarative UI engine, composited natively
//! into the egui shell.
//!
//! An app is (eventually) a Lua script that returns a [`Node`] tree as data; the engine lays
//! it out with Taffy, shapes its text with egui's fonts, and paints it with egui's epaint —
//! emitting a [`Frame`] of native egui meshes. That `Frame` is the same shape the compositor
//! already consumes from a sandboxed wasm app ([`app_abi::Surface`]'s host form), so this
//! plugs into the existing app-cell pipeline without new GPU glue: it is a *second* surface
//! producer behind the same seam, not a new renderer.
//!
//! This is the **render spine** — the first slice. The view tree is hand-built (see
//! [`EngineApp::demo`]); the script layer (Lua returns the tree), interaction (hit-test →
//! dispatch), and data binding (Loro) land on top of this loop in later steps. Text is
//! shaped by egui's fonts for now, which cannot shape complex scripts (Indic) — that swaps
//! to parley/cosmic-text behind the same `layout::shape` seam when it matters.

mod layout;
mod node;
mod paint;

use std::time::Duration;

use egui::Color32;

pub use node::{Direction, Node, Style, Val};

/// One frame's drawing from an app: GPU-ready triangles plus the texture uploads they
/// reference and egui's repaint signal. Field-for-field the host-side `app_host::Surface`,
/// kept separate so the engine doesn't depend on the wasm host (the dependency runs the
/// other way — `app_host` wraps an `EngineApp`).
pub struct Frame {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
    /// When the app wants to be drawn again (egui's repaint delay): `ZERO` while animating,
    /// `MAX` when idle. The compositor schedules redraws from this.
    pub repaint_after: Duration,
}

/// A running engine app: its persistent egui `Context` (font atlas, animations, future
/// focus/scroll state) and the view tree it draws. One `frame` call per repaint.
pub struct EngineApp {
    ctx: egui::Context,
    root: Node,
}

impl EngineApp {
    /// Build an engine app around a fixed view tree. (The Lua script that *produces* the
    /// tree per frame replaces this constructor in the next step.)
    pub fn new(root: Node) -> Self {
        EngineApp { ctx: egui::Context::default(), root }
    }

    /// The built-in demo tree — a titled card with placeholder tag pills — used to bring the
    /// render spine up in a real dock cell.
    pub fn demo() -> Self {
        EngineApp::new(demo_tree())
    }

    /// Draw one frame: lay the tree out to the cell, paint it, and tessellate to a [`Frame`].
    /// `input` is in the app's own (0,0)-based coordinates (the compositor translates real
    /// screen input into that space); `pixels_per_point` pins rasterization to the display.
    pub fn frame(&mut self, input: egui::RawInput, pixels_per_point: f32) -> Frame {
        self.ctx.set_pixels_per_point(pixels_per_point);

        let root = &self.root;
        let output = self.ctx.run_ui(input, |ui| {
            let placed = layout::layout(ui.ctx(), root);
            paint::paint(ui, &placed);
        });

        let repaint_after = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        Frame {
            primitives,
            textures_delta: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
            repaint_after,
        }
    }
}

// --- Demo tree ---------------------------------------------------------------
//
// A hand-built tree exercising the whole spine: nested column/row flex, padding + gap,
// rounded backgrounds, percent + pixel sizing, and wrapped + single-line text.

const BG: Color32 = Color32::from_rgb(0x14, 0x16, 0x1a);
const CARD: Color32 = Color32::from_rgb(0x1e, 0x22, 0x28);
const FG: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xea);
const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xab);
const ACCENT: Color32 = Color32::from_rgb(0x4c, 0x8b, 0xf5);

fn demo_tree() -> Node {
    Node::col()
        .width(Val::Pct(100.0))
        .height(Val::Pct(100.0))
        .padding(28.0)
        .gap(16.0)
        .bg(BG)
        .children(vec![
            Node::text("app_engine").font(24.0).color(FG),
            Node::text("A homegrown declarative UI engine — Taffy layout, egui paint. Lua + CRDT come next.")
                .font(15.0)
                .color(MUTED)
                .width(Val::Px(360.0)),
            card(),
        ])
}

fn card() -> Node {
    Node::col()
        .width(Val::Px(360.0))
        .padding(20.0)
        .gap(12.0)
        .bg(CARD)
        .radius(12.0)
        .children(vec![
            Node::text("Note").font(18.0).color(FG),
            Node::text("Tag-able notes are the first real app on this engine. These pills are placeholders:")
                .font(14.0)
                .color(MUTED)
                .width(Val::Px(320.0)),
            Node::row().gap(8.0).children(vec![tag("crdt"), tag("lua"), tag("taffy")]),
        ])
}

fn tag(label: &str) -> Node {
    Node::text(label).font(13.0).color(Color32::WHITE).padding(7.0).bg(ACCENT).radius(7.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The whole spine end-to-end, headless: build → Taffy layout → egui font shaping →
    // flatten. No GPU. Asserts the root fills the cell and every node placed.
    #[test]
    fn lays_out_demo_tree_to_fill_the_cell() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };

        let mut placed = Vec::new();
        let _ = ctx.run_ui(raw, |ui| {
            placed = layout::layout(ui.ctx(), &demo_tree());
        });

        // root + 3 children + (card title + body + tag-row) + 3 tags = 10 boxes.
        assert_eq!(placed.len(), 10, "every node is placed");
        let root = placed[0].rect;
        assert!((root.width() - 800.0).abs() < 1.0, "root fills width, got {}", root.width());
        assert!((root.height() - 600.0).abs() < 1.0, "root fills height, got {}", root.height());
        // The card (last top-level child) is inset by the root's 28pt padding.
        let card = placed.iter().find(|p| p.rect.width() == 360.0).expect("card present");
        assert!((card.rect.min.x - 28.0).abs() < 0.5, "card sits at the page margin");
    }
}
