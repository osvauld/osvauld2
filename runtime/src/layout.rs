//! Layout pass: turn an `El<M>` tree into a flat list of `Placed<M>` (absolute rects + paint props +
//! click message), using Taffy for the flexbox math. Text leaves size themselves, but from inside
//! the solve — Taffy calls back into parley once it knows how much width it can offer, which is
//! what lets a paragraph wrap. Consumes the tree (props move into `Placed`).

use taffy::prelude::*;
use vello::kurbo::{Insets, Rect};

use crate::anim::Transition;
use crate::col;
use crate::el::{Anchor, Appearance, Behaviour, El, Overlay};
use crate::id::Id;
use crate::scroll::Scroll;
use crate::state::{Slot, Store};
use crate::text::{Run, TextEngine};

/// One positioned node, ready to paint and hit-test. `rect` is in logical points.
pub(crate) struct Placed<M> {
    pub id: Option<Id>,
    pub rect: Rect,
    pub pad: Insets,
    pub behaviour: Behaviour<M>,
    pub appearance: Appearance,
    pub clip: Option<Rect>,
    pub content_size: (f32, f32),
    pub scroll_parent: Option<Id>,
    pub alpha: f32,
}

/// El props + its Taffy node id + mapped children, retained between build and emit.
struct Mapped<M> {
    id: Option<Id>,
    node: NodeId,
    appearance: Appearance,
    behaviour: Behaviour<M>,
    children: Vec<Mapped<M>>,
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
}

/// Answer Taffy's "how big is this leaf?" for a text node.
///
/// `available` is the source of truth for the wrap width, not `known`: on the final pass Taffy
/// passes `known` as empty (`leaf.rs:136`) but has already folded any resolved width into
/// `available` as `Definite` — and subtracted padding and border, so this is the content box both
/// ways in and out. Taffy adds the inset back at `leaf.rs:146`, which is why nothing here does.
fn measure_text(
    available: Size<AvailableSpace>,
    ctx: Option<&mut TextCtx>,
    text: &mut TextEngine,
) -> Size<f32> {
    let Some(ctx) = ctx else {
        return Size::ZERO;
    };
    let max_width = match available.width {
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

/// Lay out `root` in `viewport`, letting text leaves size themselves through [`measure_text`].
fn compute<M>(
    tree: &mut TaffyTree<TextCtx>,
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
        |_known, available, _node, ctx, _style| measure_text(available, ctx, text),
    )
    .expect("compute_layout");
}

fn build<M>(mut el: El<M>, tree: &mut TaffyTree<TextCtx>) -> Mapped<M> {
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
        }
    }
    // A text leaf sizes itself, but not here: it is handed to Taffy as node context and asked
    // during layout, once there is a width to wrap to. Explicit `.w()`/`.h()` still win — Taffy
    // clamps the measured size against them. Inputs are the exception: designed sized, never text
    // sized, so they carry no context and keep whatever the style said.
    let text_ctx = match &el.appearance.text {
        Some(ts) if el.behaviour.input.is_none() => Some(TextCtx {
            text: ts.text.clone(),
            family: ts.family,
            size: ts.size,
            runs: ts.runs.clone(),
        }),
        _ => None,
    };
    let children: Vec<Mapped<M>> = el.children.into_iter().map(|c| build(c, tree)).collect();
    let node = match text_ctx {
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
    }
}

fn emit<M>(
    mut m: Mapped<M>,
    tree: &TaffyTree<TextCtx>,
    ox: f32,
    oy: f32,
    out: &mut Vec<Placed<M>>,
    clip: Option<Rect>,
    store: &Store,
    scroll_parent: Option<Id>,
    overlays: &mut Vec<(Rect, Overlay<M>)>,
    alpha: f32,
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

    let content_size = (l.content_size.width, l.content_size.height);
    let (mut cx, mut cy) = (x, y);
    let mut child_clip = clip;
    let mut parent_scroll = scroll_parent.clone();
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
        child_clip = Some(match clip {
            Some(c) => c.intersect(rect),
            None => rect,
        })
    }
    let overlay = m.behaviour.overlay.take();
    if let Some(overlay) = overlay {
        let rect = match overlay.anchor {
            Anchor::Element => rect,
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
    let appearance = m.appearance;
    out.push(Placed {
        id: m.id,
        rect,
        scroll_parent,
        appearance,
        behaviour,
        pad: Insets::new(
            (l.padding.left + l.border.left) as f64,
            (l.padding.top + l.border.top) as f64,
            (l.padding.right + l.border.right) as f64,
            (l.padding.bottom + l.border.bottom) as f64,
        ),
        clip,
        content_size,
        alpha: node_alpha,
    });

    for c in m.children {
        emit(
            c,
            tree,
            cx,
            cy,
            out,
            child_clip,
            store,
            parent_scroll.clone(),
            overlays,
            node_alpha,
        );
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
    let mapped = build(root, &mut tree);
    compute(&mut tree, &mapped, viewport, text);
    let mut out = Vec::new();
    let mut overlays = Vec::new();
    emit(
        mapped,
        &tree,
        0.0,
        0.0,
        &mut out,
        None,
        store,
        None,
        &mut overlays,
        1.0,
    );

    let mut new_overlays = Vec::new();
    for (rect, overlay) in overlays {
        let mut tree = TaffyTree::new();

        let mapped = build(*overlay.panel, &mut tree);
        compute(&mut tree, &mapped, viewport, text);
        let pl = tree.layout(mapped.node).expect("layout");

        let (ox, oy) = overlay
            .placement
            .resolve(rect, (pl.size.width, pl.size.height), viewport);
        if let Some(msg) = overlay.dismiss {
            let mut capture_tree = TaffyTree::new();
            let dismiss_panel = col().w(viewport.0).h(viewport.1).on_click(msg);
            let mapped = build(dismiss_panel, &mut capture_tree);
            let opacity = mapped.behaviour.opacity;
            compute(&mut capture_tree, &mapped, viewport, text);

            let mut overlays = Vec::new();
            emit(
                mapped,
                &capture_tree,
                0.0,
                0.0,
                &mut out,
                None,
                store,
                None,
                &mut overlays,
                opacity,
            );
        }
        let opacity = mapped.behaviour.opacity;

        emit(
            mapped,
            &tree,
            ox,
            oy,
            &mut out,
            None,
            store,
            None,
            &mut new_overlays,
            opacity,
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
mod tests {
    use super::*;
    use crate::el::{rich, text, text_input};
    use vello::peniko::Color;

    /// Long enough to overflow any of the widths below, with no long word to get stuck on.
    const PARA: &str = "the quick brown fox jumps over the lazy dog and keeps on running";
    const WHITE: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF);

    /// The rect of the one node carrying text.
    fn text_rect(root: El<()>) -> Rect {
        solve(root, &mut TextEngine::new(), (800.0, 600.0), &Store::new())
            .iter()
            .find(|p| p.appearance.text.is_some())
            .expect("no text node")
            .rect
    }

    /// The acceptance test for the whole hook, and the bug it was written against: before it, a
    /// text leaf was shaped at max-content and frozen into `style.size` before Taffy ran, so this
    /// string came out one line tall and far wider than the 200pt parent that contained it.
    #[test]
    fn text_wraps_to_the_width_its_parent_offers() {
        let wrapped = text_rect(col().w(200.0).child(text(PARA)));
        let loose = text_rect(col().child(text(PARA)));

        assert!(
            wrapped.width() <= 200.0,
            "text overflowed its parent: {} > 200",
            wrapped.width()
        );
        assert!(
            loose.width() > 200.0,
            "the test string is too short to prove anything: {}",
            loose.width()
        );
        assert!(
            wrapped.height() > loose.height(),
            "wrapping added no height: {} vs {}",
            wrapped.height(),
            loose.height()
        );
    }

    /// A narrower box is a taller one, all the way down. Guards the direction of the constraint:
    /// passing Taffy's available width through unchanged is easy to get inverted or dropped, and
    /// either mistake still produces *a* layout.
    #[test]
    fn a_narrower_parent_makes_taller_text() {
        let heights: Vec<f64> = [400.0, 200.0, 100.0]
            .iter()
            .map(|w| text_rect(col().w(*w).child(text(PARA))).height())
            .collect();

        assert!(
            heights.windows(2).all(|p| p[1] > p[0]),
            "height did not grow as the parent shrank: {heights:?}"
        );
    }

    /// Padding is added exactly once. It used to be added by hand here *and* by Taffy at
    /// `leaf.rs:146`; the hand-written half is gone, so this is what proves the survivor is
    /// counted and not the ghost.
    #[test]
    fn padding_is_added_once_around_the_measured_text() {
        let bare = text_rect(text("hello"));
        let padded = text_rect(text("hello").pad(10.0));

        assert!(
            (padded.width() - bare.width() - 20.0).abs() < 1.0,
            "padded {} vs bare {} — expected exactly 20 more",
            padded.width(),
            bare.width()
        );
        assert!((padded.height() - bare.height() - 20.0).abs() < 1.0);
    }

    /// An explicit size still wins. Taffy clamps the measured size against `style.size`, which is
    /// the mechanism that replaced the old `is_auto()` guards — worth pinning, because losing it
    /// would silently make every `.w()` on a text leaf advisory.
    #[test]
    fn an_explicit_width_beats_the_measurement() {
        let r = text_rect(text("hi").w(300.0));
        assert!((r.width() - 300.0).abs() < 1.0, "got {}", r.width());
    }

    /// The layout pass and the paint pass have to shape the same way — same width, same plain/rich
    /// routing — and only the layout pass is told either. Getting it wrong is invisible to every
    /// test above: the box is reserved correctly and the glyphs land somewhere else, which is
    /// exactly what shipped for one commit here.
    ///
    /// Asserted as the height coming *back*, not as a bound, because the two ways to get it wrong
    /// point opposite ways. Shaping unwrapped overflows the box; shaping a rich leaf through the
    /// plain path underfills one sized for runs it then ignored. Only equality catches both.
    #[test]
    fn paint_shapes_text_to_the_box_the_layout_reserved() {
        let runs = vec![
            Run::new(0..20, 15.0, WHITE).bold(),
            Run::new(20..PARA.len(), 34.0, WHITE),
        ];
        for (name, leaf) in [("plain", text(PARA)), ("rich", rich(PARA, runs))] {
            let placed = solve::<()>(
                col().w(200.0).pad(12.0).child(leaf),
                &mut TextEngine::new(),
                (800.0, 600.0),
                &Store::new(),
            );
            let p = placed
                .iter()
                .find(|p| p.appearance.text.is_some())
                .expect("no text node");
            let ts = p.appearance.text.as_ref().unwrap();

            let (w, h) = crate::paint::measure_placed(&mut TextEngine::new(), ts, p.rect, p.pad);
            let (box_w, box_h) = (
                p.rect.width() - p.pad.x0 - p.pad.x1,
                p.rect.height() - p.pad.y0 - p.pad.y1,
            );

            assert!(
                w as f64 <= box_w + 1.0,
                "{name}: glyphs run {w} wide out of a {box_w} box"
            );
            assert!(
                (h as f64 - box_h).abs() < 1.0,
                "{name}: paint shapes {h} tall into a box reserved for {box_h}"
            );
        }
    }

    /// A rich leaf measures through the run list, which is the whole point of carrying it into
    /// `TextCtx`. Pinned by size rather than by inspecting the tree, because a run that never
    /// reaches the measure hook produces a perfectly plausible box — just the plain one.
    #[test]
    fn a_rich_leaf_is_measured_from_its_runs() {
        let s = "small BIG";
        let plain = text_rect(text(s));
        let mixed = text_rect(rich(
            s,
            vec![
                Run::new(0..6, 15.0, WHITE),
                Run::new(6..s.len(), 40.0, WHITE),
            ],
        ));

        assert!(
            mixed.height() > plain.height(),
            "the large run did not make the leaf taller: {} vs {}",
            mixed.height(),
            plain.height()
        );
        assert!(
            mixed.width() > plain.width(),
            "the large run did not make the leaf wider: {} vs {}",
            mixed.width(),
            plain.width()
        );
    }

    /// An empty run list is exactly `text`, so `rich` can be the only builder a caller reaches for
    /// without paying for it when there is nothing to style.
    #[test]
    fn rich_with_no_runs_is_plain_text() {
        assert_eq!(text_rect(rich(PARA, Vec::new())), text_rect(text(PARA)));
    }

    /// Rich leaves wrap like plain ones — the run list rides through the measure hook rather than
    /// round it, so the width constraint still reaches parley.
    #[test]
    fn a_rich_leaf_still_wraps_to_its_parent() {
        let runs = vec![
            Run::new(0..20, 15.0, WHITE).bold(),
            Run::new(20..PARA.len(), 15.0, WHITE),
        ];
        let r = text_rect(col().w(200.0).child(rich(PARA, runs)));

        assert!(r.width() <= 200.0, "rich text overflowed: {}", r.width());
        assert!(r.height() > 20.0, "rich text did not wrap: {}", r.height());
    }

    /// An input is designed sized, never text sized — the one exception `build` carves out, and
    /// the reason it hands Taffy no context for one. Stated as "its box does not move when its
    /// value does", which holds whatever the designed width happens to be.
    #[test]
    fn an_input_does_not_grow_with_its_value() {
        let short = text_rect(text_input("hi", "field", |_| ()));
        let long = text_rect(text_input(PARA, "field", |_| ()));

        assert_eq!(
            (short.width(), short.height()),
            (long.width(), long.height()),
            "an input resized itself to fit its value"
        );
    }
}
