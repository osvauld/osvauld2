//! Layout pass: turn an `El<M>` tree into a flat list of `Placed<M>` (absolute rects + paint props +
//! click message), using Taffy for the flexbox math. Text leaves are pre-measured so Taffy sizes
//! them correctly. Consumes the tree (props move into `Placed`).

use taffy::prelude::*;
use vello::kurbo::{Insets, Rect};

use crate::anim::Transition;
use crate::col;
use crate::el::{Anchor, Appearance, Behaviour, El, Overlay};
use crate::id::Id;
use crate::scroll::Scroll;
use crate::state::{Slot, Store};
use crate::text::TextEngine;

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

fn build<M>(mut el: El<M>, tree: &mut TaffyTree<()>, text_engine: &mut TextEngine) -> Mapped<M> {
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
    //A text leaf defaults to its shaped extent (plus padding- border-box); explicit .w()/.h() win.
    //Padding read as raw lengths: never write percents. Inputs are the exception: designed sized
    //never text sized.
    if let Some(ts) = &el.appearance.text {
        if el.behaviour.input.is_none() {
            let (w, h) = text_engine.measure(&ts.text, ts.family, ts.size);
            let pad_x =
                style.padding.right.into_raw().value() + style.padding.left.into_raw().value();
            let pad_y =
                style.padding.top.into_raw().value() + style.padding.bottom.into_raw().value();
            if style.size.width.is_auto() {
                style.size.width = length(w + pad_x)
            }
            if style.size.height.is_auto() {
                style.size.height = length(h + pad_y)
            }
        }
    }
    let children: Vec<Mapped<M>> = el
        .children
        .into_iter()
        .map(|c| build(c, tree, text_engine))
        .collect();
    let node = if children.is_empty() {
        tree.new_leaf(style).expect("leaf")
    } else {
        let ids: Vec<NodeId> = children.iter().map(|c| c.node).collect();
        tree.new_with_children(style, &ids).expect("node")
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
    tree: &TaffyTree<()>,
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
    if let Some((spec, (sx, sy))) = &m.behaviour.slide && let Some(id) = &m.id {
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
    if let Some(s) = &m.behaviour.scroll && let Some(id) = &m.id {
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
    if let Some(spec) = &m.behaviour.fade && let Some(id) = &m.id {
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
    let mapped = build(root, &mut tree, text);
    tree.compute_layout(
        mapped.node,
        Size {
            width: AvailableSpace::Definite(viewport.0),
            height: AvailableSpace::Definite(viewport.1),
        },
    )
    .expect("compute_layout");
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

        let mapped = build(*overlay.panel, &mut tree, text);
        tree.compute_layout(
            mapped.node,
            Size {
                width: AvailableSpace::Definite(viewport.0),
                height: AvailableSpace::Definite(viewport.1),
            },
        )
        .expect("compute_layout");
        let pl = tree.layout(mapped.node).expect("layout");

        let (ox, oy) = overlay
            .placement
            .resolve(rect, (pl.size.width, pl.size.height), viewport);
        if let Some(msg) = overlay.dismiss {
            let mut capture_tree = TaffyTree::new();
            let dismiss_panel = col().w(viewport.0).h(viewport.1).on_click(msg);
            let mapped = build(dismiss_panel, &mut capture_tree, text);
            let opacity = mapped.behaviour.opacity;

            let _ = capture_tree.compute_layout(
                mapped.node,
                Size {
                    width: AvailableSpace::Definite(viewport.0),
                    height: AvailableSpace::Definite(viewport.1),
                },
            );

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
