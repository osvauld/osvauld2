//! Lay a [`Node`] tree out with Taffy and flatten it into absolutely-placed boxes ready to paint.
//!
//! A fresh [`taffy::TaffyTree`] is built each frame (the engine is immediate-mode, so there's no
//! retained tree to keep in sync). Containers become flex nodes; a text node is a leaf sized from
//! a galley shaped by egui's fonts through Taffy's measure callback — the one load-bearing seam
//! between layout and text. Then we walk once into absolute [`egui::Rect`]s (re-shaping each text
//! galley at its final width), so [`crate::paint`] is a flat, geometry-free draw loop.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, FontId, Galley, Pos2, Rect, Vec2};
use rich_text::Run;
use taffy::{AvailableSpace, NodeId, Size, TaffyTree};

use crate::node::{Border, BoxShadow, Corners, Node, Style};

/// The engine's mark palette passed to `rich_text` (theme-in — the renderer hardcodes nothing).
const THEME: rich_text::Theme = rich_text::Theme {
    code_color: Color32::from_rgb(0x9c, 0xc0, 0xff),
    code_bg: Color32::from_rgb(0x22, 0x26, 0x2e),
    link_color: Color32::from_rgb(0x4c, 0x8b, 0xf5),
    link_underline: Color32::from_rgb(0x2a, 0x4a, 0x80),
    strike_color: Color32::from_rgb(0x80, 0x80, 0x80),
};

/// The paint properties of one node in one state (resting / hover / active).
#[derive(Clone, Copy)]
pub(crate) struct Look {
    pub background: Option<Color32>,
    pub color: Color32,
    pub corner_radius: Corners,
    pub border: Option<Border>,
    pub shadow: Option<BoxShadow>,
    /// Effective opacity: this node's own × every ancestor's (CSS subtree semantics), resolved
    /// here so paint just multiplies colours by it.
    pub opacity: f32,
}

/// One node, laid out for this frame in absolute (cell-local) coordinates.
pub(crate) struct Placed {
    pub rect: Rect,
    /// Text origin (content-box top-left) and a colour-neutral galley (shaped with `PLACEHOLDER`),
    /// so paint can recolour per state without re-shaping.
    pub text: Option<(Pos2, Arc<Galley>)>,
    /// Paint props for the resting state, plus optional hover/active overlays.
    pub base: Look,
    pub hover: Option<Look>,
    pub active: Option<Look>,
    /// Opaque click-handler id (see `Node::on_click`), carried through for hit-testing.
    pub on_click: Option<u32>,
    /// `Some(id)` ⇒ an editable field (see `Node::editor`); paint draws its caret/selection when
    /// focused, and a click focuses + places the caret.
    pub editor: Option<String>,
    /// `Some(id)` ⇒ an embedded block document (see `Node::doc`); the engine renders a native
    /// doc_editor into `rect` in its own child Ui.
    pub doc: Option<String>,
    /// `Some(spec)` ⇒ a chart leaf (see `Node::chart`); the engine paints it with `egui_plot` into
    /// `rect` in its own child Ui, after the scene.
    pub chart: Option<crate::node::ChartSpec>,
    /// The clip rect this box paints within — the nearest scroll ancestor's box, else the cell.
    pub clip: Rect,
    /// `Some((id, max_scroll))` ⇒ a scroll region: content offset by `id`'s retained position,
    /// scrollable over `0..=max_scroll` points per axis (an axis with no overflow has max 0).
    pub scroll: Option<(String, Vec2)>,
    /// `Some` ⇒ a table-resize handle (see `Node::resize`); the engine hit-tests its trailing edge.
    pub resize: Option<crate::node::Resize>,
}

/// Per-node data Taffy carries for us, used after layout.
struct Ctx {
    base: Look,
    hover: Option<Look>,
    active: Option<Look>,
    text: Option<TextSpec>,
    on_click: Option<u32>,
    editor: Option<String>,
    doc: Option<String>,
    chart: Option<crate::node::ChartSpec>,
    scroll: Option<crate::node::ScrollSpec>,
    resize: Option<crate::node::Resize>,
    popup: bool,
}

/// A text leaf's styled runs and base size. Colour is resolved at shape time — per-run for rich
/// text, left neutral for a single plain run so a hover can recolour it.
struct TextSpec {
    runs: Vec<Run>,
    size: f32,
}

/// Pull the paint properties out of a style.
fn look(style: &Style) -> Look {
    Look {
        background: style.background,
        color: style.color,
        corner_radius: style.corner_radius,
        border: style.border,
        shadow: style.shadow,
        opacity: style.opacity.clamp(0.0, 1.0),
    }
}

/// Lay `root` out to fill `host` (the rect the engine was handed — a panel, a dock tab body, or the
/// whole window), returning every box in pre-order (parents first = painter back-to-front).
/// `offsets` are the retained scroll positions by region id.
pub(crate) fn layout(ctx: &egui::Context, host: Rect, root: &Node, offsets: &HashMap<String, Vec2>) -> Vec<Placed> {
    let mut tree: TaffyTree<Ctx> = TaffyTree::new();
    let root_id = build(&mut tree, root);

    let screen = host;
    let available = Size {
        width: AvailableSpace::Definite(screen.width()),
        height: AvailableSpace::Definite(screen.height()),
    };

    tree.compute_layout_with_measure(root_id, available, |known, space, _id, node_ctx, _style| {
        measure(ctx, known, space, node_ctx)
    })
    .expect("taffy layout never fails for a well-formed tree");

    let mut out = Vec::new();
    let mut popups = Vec::new();
    collect(&tree, root_id, screen.min, screen, 1.0, offsets, ctx, &mut out, &mut popups);
    // Popup subtrees collect after everything else — painted on top, hit-tested first — and
    // clip to the host, not their scroll ancestor, so a dropdown escapes its table body.
    while let Some((id, origin, opacity)) = popups.pop() {
        let mut nested = Vec::new();
        collect(&tree, id, origin, screen, opacity, offsets, ctx, &mut out, &mut nested);
        popups.extend(nested);
    }
    out
}

/// Build a Taffy node (and its subtree) from a view node, attaching the paint/text context.
fn build(tree: &mut TaffyTree<Ctx>, node: &Node) -> NodeId {
    let mut style = node.style.to_taffy();
    // `Overflow::Hidden` zeroes the region's automatic minimum size on that axis, so it stays at
    // its laid-out size while larger content overflows (which `collect` clips and offsets). No
    // scrollbar gutter (unlike `Overflow::Scroll`). Only the scrolling axes — a non-scrolling
    // axis keeps content-driven sizing (e.g. an x-only table region still grows with its rows).
    if let Some(s) = &node.scroll {
        if s.x {
            style.overflow.x = taffy::Overflow::Hidden;
        }
        if s.y {
            style.overflow.y = taffy::Overflow::Hidden;
        }
    }
    let ctx = Ctx {
        base: look(&node.style),
        hover: node.hover.as_ref().map(look),
        active: node.active.as_ref().map(look),
        text: node.text.as_ref().map(|runs| TextSpec { runs: runs.clone(), size: node.style.font_size }),
        on_click: node.on_click,
        editor: node.editor.clone(),
        doc: node.doc.clone(),
        chart: node.chart.clone(),
        scroll: node.scroll.clone(),
        resize: node.resize.clone(),
        popup: node.popup,
    };
    if node.children.is_empty() {
        tree.new_leaf_with_context(style, ctx).expect("new leaf")
    } else {
        let kids: Vec<NodeId> = node.children.iter().map(|c| build(tree, c)).collect();
        // A scroll region's children must keep their natural main-axis size; CSS's default
        // `flex-shrink: 1` would compress them so nothing overflows, but that overflow is exactly
        // what becomes scrollable. Only when the region scrolls its *main* axis — a cross-axis
        // region (x-scroll column) must leave children shrinkable (a table's body compresses to
        // the leftover height and scrolls internally).
        let scrolls_main = node.scroll.as_ref().is_some_and(|s| match node.style.direction {
            crate::node::Direction::Column => s.y,
            crate::node::Direction::Row => s.x,
        });
        if scrolls_main {
            for &k in &kids {
                let mut child_style = tree.style(k).expect("child style").clone();
                child_style.flex_shrink = 0.0;
                tree.set_style(k, child_style).expect("set child style");
            }
        }
        let id = tree.new_with_children(style, &kids).expect("new container");
        tree.set_node_context(id, Some(ctx)).expect("set context");
        id
    }
}

/// Taffy's measure callback: a text node sizes to its shaped galley; a container measures 0 (it's
/// sized from its children).
fn measure(
    ctx: &egui::Context,
    known: Size<Option<f32>>,
    space: Size<AvailableSpace>,
    node_ctx: Option<&mut Ctx>,
) -> Size<f32> {
    let Some(node_ctx) = node_ctx else {
        return Size { width: 0.0, height: 0.0 };
    };
    let Some(spec) = node_ctx.text.as_ref() else {
        return Size { width: 0.0, height: 0.0 };
    };
    // Wrap at the width Taffy fixed, else the offered space: definite → that width, min-content
    // → 0 (longest word, CSS min-content), max-content → unwrapped.
    let wrap = known.width.unwrap_or(match space.width {
        AvailableSpace::Definite(w) => w,
        AvailableSpace::MinContent => 0.0,
        AvailableSpace::MaxContent => f32::INFINITY,
    });
    let galley = shape_text(ctx, &spec.runs, spec.size, node_ctx.base.color, wrap);
    // Ceil so Taffy's whole-pixel rounding can't hand back a box a hair narrower than the galley
    // (re-shaping at that shaved width would wrap an extra line).
    Size { width: galley.size().x.ceil(), height: galley.size().y.ceil() }
}

/// Walk the computed layout, turning Taffy's parent-relative boxes into absolute `Placed`s.
/// `clip` is the rect this box paints within; a scroll region offsets its children by the
/// retained scroll position and clips them to itself. `opacity` is the inherited ancestor
/// product, folded into every state's `Look` (CSS subtree opacity).
fn collect(
    tree: &TaffyTree<Ctx>,
    id: NodeId,
    origin: Pos2,
    clip: Rect,
    opacity: f32,
    offsets: &HashMap<String, Vec2>,
    ctx: &egui::Context,
    out: &mut Vec<Placed>,
    popups: &mut Vec<(NodeId, Pos2, f32)>,
) {
    let layout = tree.layout(id).expect("layout was computed");
    let rect = Rect::from_min_size(
        origin + Vec2::new(layout.location.x, layout.location.y),
        Vec2::new(layout.size.width, layout.size.height),
    );

    let node_ctx = tree.get_node_context(id);
    let fold = |mut l: Look| {
        l.opacity *= opacity;
        l
    };
    let base = node_ctx.map_or(
        Look {
            background: None,
            color: Color32::from_gray(0xdd),
            corner_radius: Corners::default(),
            border: None,
            shadow: None,
            opacity,
        },
        |c| fold(c.base),
    );
    let hover = node_ctx.and_then(|c| c.hover).map(fold);
    let active = node_ctx.and_then(|c| c.active).map(fold);
    let on_click = node_ctx.and_then(|c| c.on_click);
    let editor = node_ctx.and_then(|c| c.editor.clone());
    let doc = node_ctx.and_then(|c| c.doc.clone());
    let chart = node_ctx.and_then(|c| c.chart.clone());
    let scroll_id = node_ctx.and_then(|c| c.scroll.clone());
    let resize = node_ctx.and_then(|c| c.resize.clone());
    let text = node_ctx.and_then(|c| c.text.as_ref()).map(|spec| {
        // Re-shape at the final content width so the painted wrapping matches the laid-out box.
        // Content box sits inside padding + border (border-box layout).
        let pad = layout.padding;
        let bord = layout.border;
        let content_w = (rect.width() - pad.left - pad.right - bord.left - bord.right).max(0.0);
        let galley = shape_text(ctx, &spec.runs, spec.size, base.color, content_w);
        (rect.min + Vec2::new(pad.left + bord.left, pad.top + bord.top), galley)
    });

    // A scroll region offsets its children up by the (clamped) scroll position and confines them
    // to its own box; everything else passes parent origin and clip straight down.
    let (child_origin, child_clip, scroll) = match &scroll_id {
        Some(spec) => {
            let pad = layout.padding;
            let (mut right, mut bottom) = (0.0_f32, 0.0_f32);
            for &c in tree.children(id).expect("children list").iter() {
                let cl = tree.layout(c).expect("child layout");
                right = right.max(cl.location.x + cl.size.width);
                bottom = bottom.max(cl.location.y + cl.size.height);
            }
            // Only the declared axes scroll; the other axis reports no range.
            let max_scroll = Vec2::new(
                if spec.x { (right + pad.right - rect.width()).max(0.0) } else { 0.0 },
                if spec.y { (bottom + pad.bottom - rect.height()).max(0.0) } else { 0.0 },
            );
            let offset =
                offsets.get(&spec.id).copied().unwrap_or(Vec2::ZERO).clamp(Vec2::ZERO, max_scroll);
            (rect.min - offset, clip.intersect(rect), Some((spec.id.clone(), max_scroll)))
        }
        None => (rect.min, clip, None),
    };

    let child_opacity = base.opacity;
    out.push(Placed { rect, text, base, hover, active, on_click, editor, doc, chart, clip, scroll, resize });

    for child in tree.children(id).expect("children list") {
        // A popup child is deferred (with the origin it would have had) to a top layer.
        if tree.get_node_context(child).is_some_and(|c| c.popup) {
            popups.push((child, child_origin, child_opacity));
        } else {
            collect(tree, child, child_origin, child_clip, child_opacity, offsets, ctx, out, popups);
        }
    }
}

/// Shape a text leaf's runs into a galley. A single unmarked, uncoloured run stays colour-neutral
/// (`PLACEHOLDER`, so paint recolours it per state); anything richer bakes per-run colour via
/// `rich_text`.
fn shape_text(ctx: &egui::Context, runs: &[Run], size: f32, base_color: Color32, wrap: f32) -> Arc<Galley> {
    if runs.len() == 1 && runs[0].marks.is_empty() && runs[0].color.is_none() {
        ctx.fonts_mut(|f| f.layout(runs[0].text.clone(), FontId::proportional(size), Color32::PLACEHOLDER, wrap))
    } else {
        rich_text::galley(ctx, runs, rich_text::Style::new(size, base_color), THEME, wrap)
    }
}
