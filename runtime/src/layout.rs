//! Layout pass: turn an `El<M>` tree into a flat list of `Placed<M>` (absolute rects + paint props +
//! click message), using Taffy for the flexbox math. Text leaves size themselves, but from inside
//! the solve — Taffy calls back into parley once it knows how much width it can offer, which is
//! what lets a paragraph wrap. Consumes the tree (props move into `Placed`).

use taffy::prelude::*;
use vello::kurbo::{Affine, Insets, Rect};

use crate::anim::{Spring, Transition};
use crate::col;
use crate::el::{Anchor, Appearance, Behaviour, El, Overlay};
use crate::id::Id;
use crate::scroll::Scroll;
use crate::state::{Slot, Store};
use crate::text::{Run, TextEngine};
use crate::zoom::Zoom;

/// One positioned node, ready to paint and hit-test. `rect` is in logical points.
pub(crate) enum PlacedKind {
    Node,
    PushClip { rect: Rect, transform: Affine },
    PopClip,
}

pub(crate) struct Placed<M> {
    pub kind: PlacedKind,
    pub id: Option<Id>,
    pub rect: Rect,
    pub pad: Insets,
    pub behaviour: Behaviour<M>,
    pub appearance: Appearance,
    pub content_size: (f32, f32),
    pub scroll_parent: Option<Id>,
    /// The nearest zoomable ancestor, so a press inside a camera can still pan it.
    pub zoom_parent: Option<Id>,
    pub alpha: f32,
    pub transform: Affine,
}

/// El props + its Taffy node id + mapped children, retained between build and emit.
struct Mapped<M> {
    id: Option<Id>,
    node: NodeId,
    appearance: Appearance,
    behaviour: Behaviour<M>,
    children: Vec<Mapped<M>>,
    zoom_children: Vec<El<M>>,
}

fn clip_marker<M>(kind: PlacedKind) -> Placed<M> {
    Placed {
        kind,
        id: None,
        rect: Rect::ZERO,
        pad: Insets::new(0.0, 0.0, 0.0, 0.0),
        behaviour: Behaviour::default(),
        appearance: Appearance::default(),
        content_size: (0.0, 0.0),
        scroll_parent: None,
        zoom_parent: None,
        alpha: 1.0,
        transform: Affine::IDENTITY,
    }
}

/// What a text leaf needs to answer Taffy's measure question, hung on its node as context.
///
/// A copy of the string rather than a borrow: Taffy owns the context for the length of the layout,
/// while the original lives on in `Appearance` to be painted. Cheap next to the shaping it feeds.
pub(crate) struct TextCtx {
    text: String,
    family: &'static str,
    size: f32,
    /// Empty for a plain leaf. Copied for the same reason the string is.
    runs: Vec<Run>,
    /// False for a label — see [`crate::El::no_wrap`]. Makes the answer below ignore `available`
    /// entirely and report max-content.
    wrap: bool,
}

/// Answer Taffy's "how big is this leaf?" for a text node.
///
/// `available` is the source of truth for the wrap width, not `known`: on the final pass Taffy
/// passes `known` as empty (`leaf.rs:136`) but has already folded any resolved width into
/// `available` as `Definite` — and subtracted padding and border, so this is the content box both
/// ways in and out. Taffy adds the inset back at `leaf.rs:146`, which is why nothing here does.
enum LeafCtx {
    Text(TextCtx),
    Frame(Size<f32>),
}

fn measure_leaf(
    available: Size<AvailableSpace>,
    ctx: Option<&mut LeafCtx>,
    text: &mut TextEngine,
) -> Size<f32> {
    let Some(ctx) = ctx else { return Size::ZERO };
    let ctx = match ctx {
        LeafCtx::Text(ctx) => ctx,
        LeafCtx::Frame(size) => return *size,
    };
    // A label answers max-content whatever it is offered, so nothing downstream can fold it. It
    // overflows instead, which is the right failure for a control (`El::no_wrap`).
    let max_width = match available.width {
        _ if !ctx.wrap => None,
        AvailableSpace::Definite(w) => Some(w),
        // Parley refuses to break inside a word, so any width under the longest one *is*
        // min-content — see `text::tests::a_constraint_below_min_content_does_not_break_a_word`.
        // Asking that way costs one shaping pass instead of `content_widths`' two.
        AvailableSpace::MinContent => Some(0.0),
        AvailableSpace::MaxContent => None,
    };
    let (width, height) = if ctx.runs.is_empty() {
        text.measure(&ctx.text, ctx.family, ctx.size, max_width)
    } else {
        text.measure_rich(&ctx.text, &ctx.runs, max_width)
    };
    Size { width, height }
}

/// Lay out `root` in `viewport`, letting measured leaves size themselves through [`measure_leaf`].
fn compute<M>(
    tree: &mut TaffyTree<LeafCtx>,
    mapped: &Mapped<M>,
    viewport: (f32, f32),
    text: &mut TextEngine,
) {
    tree.compute_layout_with_measure(
        mapped.node,
        Size {
            width: AvailableSpace::Definite(viewport.0),
            height: AvailableSpace::Definite(viewport.1),
        },
        |_known, available, _node, ctx, _style| measure_leaf(available, ctx, text),
    )
    .expect("compute_layout");
}

fn build<M>(mut el: El<M>, tree: &mut TaffyTree<LeafCtx>, store: &Store) -> Mapped<M> {
    let mut style = el.layout;

    if let Some(spec) = &el.behaviour.scroll {
        if spec.x {
            style.overflow.x = taffy::style::Overflow::Hidden;
        }

        if spec.y {
            style.overflow.y = taffy::style::Overflow::Hidden;
        }
        let main_scrolled = match style.flex_direction {
            FlexDirection::Column | FlexDirection::ColumnReverse => spec.y,
            FlexDirection::Row | FlexDirection::RowReverse => spec.x,
        };
        if main_scrolled {
            el.children
                .iter_mut()
                .for_each(|c| c.layout.flex_shrink = 0.0);
            if style.flex_grow > 0.0 {
                style.flex_basis = length(0.0);
            }
        }
    }
    // Measured leaves are handed to Taffy as node context. Text waits for an offered wrap width;
    // Frame reports its fixed intrinsic size. Explicit `.w()`/`.h()` still win. Inputs are designed
    // sized, never text sized, so they carry no context and keep whatever the style said.
    let leaf_ctx = match (&el.appearance.text, &el.appearance.frame) {
        (Some(ts), _) if el.behaviour.input.is_none() => Some(LeafCtx::Text(TextCtx {
            text: ts.text.clone(),
            family: ts.family,
            size: ts.size,
            runs: ts.runs.clone(),
            wrap: ts.wrap,
        })),
        (_, Some(frame)) => {
            let (width, height) = frame.size();
            Some(LeafCtx::Frame(Size {
                width: width as f32,
                height: height as f32,
            }))
        }
        _ => None,
    };
    let zoom_children = if el.behaviour.zoom.is_some() {
        std::mem::take(&mut el.children)
    } else {
        Vec::new()
    };
    let children: Vec<Mapped<M>> = el
        .children
        .into_iter()
        .map(|c| build(c, tree, store))
        .collect();
    let node = match leaf_ctx {
        // Context only reaches a measure call on a childless node, so a text leaf with children
        // would silently measure nothing. Not reachable today: text and children are set by
        // different builders.
        Some(ctx) if children.is_empty() => {
            tree.new_leaf_with_context(style, ctx).expect("text leaf")
        }
        _ if children.is_empty() => tree.new_leaf(style).expect("leaf"),
        _ => {
            let ids: Vec<NodeId> = children.iter().map(|c| c.node).collect();
            tree.new_with_children(style, &ids).expect("node")
        }
    };
    Mapped {
        id: el.id,
        node,
        appearance: el.appearance,
        behaviour: el.behaviour,
        children,
        zoom_children,
    }
}

fn emit<M>(
    mut m: Mapped<M>,
    tree: &TaffyTree<LeafCtx>,
    text: &mut TextEngine,
    ox: f32,
    oy: f32,
    out: &mut Vec<Placed<M>>,
    store: &Store,
    scroll_parent: Option<Id>,
    zoom_parent: Option<Id>,
    overlays: &mut Vec<(Rect, Overlay<M>)>,
    alpha: f32,
    transform: Affine,
    stable_transform: Affine,
) {
    let l = tree.layout(m.node).expect("layout");
    // Taffy gives parent-relative locations; accumulate to absolute.
    let (mut dx, mut dy) = m.behaviour.offset;
    if let Some((spec, (sx, sy))) = &m.behaviour.slide
        && let Some(id) = &m.id
    {
        let p = store
            .get::<Transition>(id, Slot::Slide)
            .map(|t| t.progress)
            .unwrap_or(0.0);
        let e = spec.easing.apply(p);
        dx += (1.0 - e) * sx;
        dy += (1.0 - e) * sy;
    }
    let x = ox + l.location.x + dx;
    let y = oy + l.location.y + dy;
    let rect = Rect::new(
        x as f64,
        y as f64,
        (x + l.size.width) as f64,
        (y + l.size.height) as f64,
    );

    let mut transform = transform;
    let mut stable_transform = stable_transform;
    if m.behaviour.scale != 1.0 {
        let scale = Affine::translate((rect.x0, rect.y0))
            * Affine::scale(m.behaviour.scale as f64)
            * Affine::translate((-rect.x0, -rect.y0));
        transform *= scale;
        stable_transform *= scale;
    }
    if let Some((spec, target_scale)) = &m.behaviour.press_scale
        && let Some(id) = &m.id
    {
        let p = store
            .get::<Spring>(id, Slot::PressScale)
            .map(|s| s.value.clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let s = 1.0 + spec.easing.apply(p) * (target_scale - 1.0);
        let c = rect.center();
        transform = transform
            * Affine::translate((c.x, c.y))
            * Affine::scale(s as f64)
            * Affine::translate((-c.x, -c.y));
    }

    let content_size = (l.content_size.width, l.content_size.height);
    let (mut cx, mut cy) = (x, y);
    let mut child_transform = transform;
    let mut stable_child_transform = stable_transform;
    let mut parent_scroll = scroll_parent.clone();
    let mut parent_zoom = zoom_parent.clone();
    if let Some(s) = &m.behaviour.scroll
        && let Some(id) = &m.id
    {
        let scroll = store
            .get::<Scroll>(id, Slot::Scroll)
            .copied()
            .unwrap_or_default();
        parent_scroll = Some(id.clone());
        if s.x {
            cx -= scroll.x;
        }
        if s.y {
            cy -= scroll.y;
        }
    }
    if m.behaviour.zoom.is_some()
        && let Some(id) = &m.id
    {
        let zoom = store
            .get::<Zoom>(id, Slot::Zoom)
            .copied()
            .unwrap_or_default();
        let camera = Affine::translate((rect.x0 + zoom.pan.0 as f64, rect.y0 + zoom.pan.1 as f64))
            * Affine::scale(zoom.scale as f64)
            * Affine::translate((-rect.x0, -rect.y0));
        child_transform = transform * camera;
        stable_child_transform = stable_transform * camera;
        parent_zoom = Some(id.clone());
    }
    let overlay = m.behaviour.overlay.take();
    if let Some(overlay) = overlay {
        let rect = match overlay.anchor {
            Anchor::Element => stable_transform.transform_rect_bbox(rect),
            Anchor::Point(x, y) => Rect::new(x as f64, y as f64, x as f64, y as f64),
        };
        overlays.push((rect, overlay));
    }
    let mut opacity = m.behaviour.opacity;
    if let Some(spec) = &m.behaviour.fade
        && let Some(id) = &m.id
    {
        let p = store
            .get::<Transition>(id, Slot::Fade)
            .map(|t| t.progress)
            .unwrap_or(0.0);
        opacity *= spec.easing.apply(p)
    }
    let node_alpha = opacity * alpha;
    let behaviour = m.behaviour;
    let clips_children = behaviour.scroll.is_some();
    let appearance = m.appearance;
    out.push(Placed {
        kind: PlacedKind::Node,
        id: m.id,
        rect,
        scroll_parent,
        zoom_parent,
        appearance,
        behaviour,
        pad: Insets::new(
            (l.padding.left + l.border.left) as f64,
            (l.padding.top + l.border.top) as f64,
            (l.padding.right + l.border.right) as f64,
            (l.padding.bottom + l.border.bottom) as f64,
        ),
        content_size,
        alpha: node_alpha,
        transform,
    });

    if clips_children {
        out.push(clip_marker(PlacedKind::PushClip { rect, transform }));
    }

    if !m.zoom_children.is_empty() {
        out.push(clip_marker(PlacedKind::PushClip { rect, transform }));
        let mut child_tree = TaffyTree::new();
        let child_root = col()
            .w(rect.width() as f32)
            .h(rect.height() as f32)
            .children(m.zoom_children);
        let mapped = build(child_root, &mut child_tree, store);
        compute(
            &mut child_tree,
            &mapped,
            (rect.width() as f32, rect.height() as f32),
            text,
        );
        emit(
            mapped,
            &child_tree,
            text,
            rect.x0 as f32,
            rect.y0 as f32,
            out,
            store,
            parent_scroll.clone(),
            parent_zoom.clone(),
            overlays,
            node_alpha,
            child_transform,
            stable_child_transform,
        );
        out.push(clip_marker(PlacedKind::PopClip));
    }

    for c in m.children {
        emit(
            c,
            tree,
            text,
            cx,
            cy,
            out,
            store,
            parent_scroll.clone(),
            parent_zoom.clone(),
            overlays,
            node_alpha,
            child_transform,
            stable_child_transform,
        );
    }

    if clips_children {
        out.push(clip_marker(PlacedKind::PopClip));
    }
}

/// Lay out `root` within `viewport` (logical points); return painted nodes in paint order
/// (parents before children).
pub(crate) fn solve<M>(
    root: El<M>,
    text: &mut TextEngine,
    viewport: (f32, f32),
    store: &Store,
) -> Vec<Placed<M>> {
    let mut tree = TaffyTree::new();
    let mapped = build(root, &mut tree, store);
    compute(&mut tree, &mapped, viewport, text);
    let mut out = Vec::new();
    let mut overlays = Vec::new();
    emit(
        mapped,
        &tree,
        text,
        0.0,
        0.0,
        &mut out,
        store,
        None,
        None,
        &mut overlays,
        1.0,
        Affine::IDENTITY,
        Affine::IDENTITY,
    );

    let mut new_overlays = Vec::new();
    for (rect, overlay) in overlays {
        let mut tree = TaffyTree::new();

        let mapped = build(*overlay.panel, &mut tree, store);
        compute(&mut tree, &mapped, viewport, text);
        let pl = tree.layout(mapped.node).expect("layout");

        let (ox, oy) = overlay
            .placement
            .resolve(rect, (pl.size.width, pl.size.height), viewport);
        if let Some(msg) = overlay.dismiss {
            let mut capture_tree = TaffyTree::new();
            let dismiss_panel = col().w(viewport.0).h(viewport.1).on_click(msg);
            let mapped = build(dismiss_panel, &mut capture_tree, store);
            let opacity = mapped.behaviour.opacity;
            compute(&mut capture_tree, &mapped, viewport, text);

            let mut overlays = Vec::new();
            emit(
                mapped,
                &capture_tree,
                text,
                0.0,
                0.0,
                &mut out,
                store,
                None,
                None,
                &mut overlays,
                opacity,
                Affine::IDENTITY,
                Affine::IDENTITY,
            );
        }
        let opacity = mapped.behaviour.opacity;

        emit(
            mapped,
            &tree,
            text,
            ox,
            oy,
            &mut out,
            store,
            None,
            None,
            &mut new_overlays,
            opacity,
            Affine::IDENTITY,
            Affine::IDENTITY,
        );
    }
    out
}

/// What the measure hook buys: a text leaf that answers *after* Taffy knows its width.
///
/// Relational again, for the reason [`crate::text`]'s tests are — the default family is the OS's
/// sans-serif, so no pixel count here is portable. What is portable is that text now fits the box
/// it was given.
#[cfg(test)]
mod tests;
