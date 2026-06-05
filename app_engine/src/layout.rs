//! Lay a [`Node`] tree out with Taffy and flatten it into absolutely-placed boxes ready to
//! paint.
//!
//! Each frame we build a fresh [`taffy::TaffyTree`] from the view tree (the engine is
//! immediate-mode — `view = f(state)` rebuilt per repaint, so there is no retained tree to
//! keep in sync). Containers become flex nodes; a text node becomes a leaf whose size comes
//! from a galley shaped by **egui's own fonts** through Taffy's measure callback — that
//! callback is the one load-bearing seam between the layout engine and the text engine.
//!
//! After `compute_layout` we walk the tree once, turning Taffy's parent-relative
//! coordinates into absolute [`egui::Rect`]s (and re-shaping each text node's galley at its
//! final width), so [`crate::paint`] is a flat, geometry-free draw loop.

use std::sync::Arc;

use egui::{Color32, FontId, Galley, Pos2, Rect, Vec2};
use taffy::{AvailableSpace, NodeId, Size, TaffyTree};

use crate::node::Node;

/// One node, laid out for this frame in absolute (cell-local) coordinates.
pub(crate) struct Placed {
    pub rect: Rect,
    pub background: Option<Color32>,
    pub corner_radius: f32,
    /// The text origin (content-box top-left = `rect.min` + padding), and the shaped galley.
    pub text: Option<(Pos2, Arc<Galley>)>,
}

/// Per-node data Taffy carries for us: what the painter needs, plus the text to measure.
struct Ctx {
    background: Option<Color32>,
    corner_radius: f32,
    text: Option<TextRun>,
}

/// A measurable/paintable run of text and the bits needed to shape it.
struct TextRun {
    text: String,
    size: f32,
    color: Color32,
}

/// Lay `root` out to fill the egui context's current screen rect, returning every box in
/// pre-order (parents before children — i.e. painter back-to-front).
pub(crate) fn layout(ctx: &egui::Context, root: &Node) -> Vec<Placed> {
    let mut tree: TaffyTree<Ctx> = TaffyTree::new();
    let root_id = build(&mut tree, root);

    let screen = ctx.content_rect();
    let available = Size {
        width: AvailableSpace::Definite(screen.width()),
        height: AvailableSpace::Definite(screen.height()),
    };

    tree.compute_layout_with_measure(root_id, available, |known, space, _id, node_ctx, _style| {
        measure(ctx, known, space, node_ctx)
    })
    .expect("taffy layout never fails for a well-formed tree");

    let mut out = Vec::new();
    collect(&tree, root_id, screen.min, ctx, &mut out);
    out
}

/// Build a Taffy node (and its subtree) from a view node, attaching the paint/text context.
fn build(tree: &mut TaffyTree<Ctx>, node: &Node) -> NodeId {
    let style = node.style.to_taffy();
    let ctx = Ctx {
        background: node.style.background,
        corner_radius: node.style.corner_radius,
        text: node.text.as_ref().map(|t| TextRun {
            text: t.clone(),
            size: node.style.font_size,
            color: node.style.color,
        }),
    };
    if node.children.is_empty() {
        tree.new_leaf_with_context(style, ctx).expect("new leaf")
    } else {
        let kids: Vec<NodeId> = node.children.iter().map(|c| build(tree, c)).collect();
        let id = tree.new_with_children(style, &kids).expect("new container");
        tree.set_node_context(id, Some(ctx)).expect("set context");
        id
    }
}

/// Taffy's measure callback: a text node sizes to its shaped galley; a container measures 0
/// (Taffy sizes it from its children).
fn measure(
    ctx: &egui::Context,
    known: Size<Option<f32>>,
    space: Size<AvailableSpace>,
    node_ctx: Option<&mut Ctx>,
) -> Size<f32> {
    let Some(TextRun { text, size, color }) = node_ctx.and_then(|c| c.text.as_ref()) else {
        return Size { width: 0.0, height: 0.0 };
    };
    // Wrap at the width Taffy already fixed, else the definite space it offers, else don't
    // wrap (a min/max-content probe — good enough until styled line-breaking lands).
    let wrap = known.width.or(match space.width {
        AvailableSpace::Definite(w) => Some(w),
        _ => None,
    });
    let galley = shape(ctx, text, *size, *color, wrap.unwrap_or(f32::INFINITY));
    Size { width: galley.size().x, height: galley.size().y }
}

/// Walk the computed layout, turning Taffy's parent-relative boxes into absolute `Placed`s.
fn collect(tree: &TaffyTree<Ctx>, id: NodeId, origin: Pos2, ctx: &egui::Context, out: &mut Vec<Placed>) {
    let layout = tree.layout(id).expect("layout was computed");
    let rect = Rect::from_min_size(
        origin + Vec2::new(layout.location.x, layout.location.y),
        Vec2::new(layout.size.width, layout.size.height),
    );

    let node_ctx = tree.get_node_context(id);
    let background = node_ctx.and_then(|c| c.background);
    let corner_radius = node_ctx.map_or(0.0, |c| c.corner_radius);
    let text = node_ctx.and_then(|c| c.text.as_ref()).map(|t| {
        // Re-shape at the final content width (box width minus left/right padding) so the
        // painted wrapping matches the laid-out box exactly.
        let pad = layout.padding;
        let content_w = (rect.width() - pad.left - pad.right).max(0.0);
        let galley = shape(ctx, &t.text, t.size, t.color, content_w);
        (rect.min + Vec2::new(pad.left, pad.top), galley)
    });

    out.push(Placed { rect, background, corner_radius, text });

    // Children's locations are relative to this node's box origin.
    for child in tree.children(id).expect("children list") {
        collect(tree, child, rect.min, ctx, out);
    }
}

/// Shape one run through egui's fonts (the text engine, swappable for parley/cosmic-text
/// later). Colour is baked into the galley, so the painter needs no per-run colour. Shaping
/// mutates the galley cache, hence `fonts_mut`.
fn shape(ctx: &egui::Context, text: &str, size: f32, color: Color32, wrap: f32) -> Arc<Galley> {
    ctx.fonts_mut(|f| f.layout(text.to_owned(), FontId::proportional(size), color, wrap))
}
