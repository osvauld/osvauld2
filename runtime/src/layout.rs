//! Layout pass: turn an `El<M>` tree into a flat list of `Placed<M>` (absolute rects + paint props +
//! click message), using Taffy for the flexbox math. Text leaves are pre-measured so Taffy sizes
//! them correctly. Consumes the tree (props move into `Placed`).

use taffy::prelude::*;
use vello::kurbo::{Insets, Rect};

use crate::el::{Content, El};
use crate::scroll::Scrolls;
use crate::text::TextEngine;

/// One positioned node, ready to paint and hit-test. `rect` is in logical points.
pub(crate) struct Placed<M> {
    pub rect: Rect,
    pub pad: Insets,
    pub content: Content<M>,
    pub clip: Option<Rect>,
    pub content_size: (f32, f32),
}

/// El props + its Taffy node id + mapped children, retained between build and emit.
struct Mapped<M> {
    node: NodeId,
    content: Content<M>,
    children: Vec<Mapped<M>>,
}

fn build<M>(mut el: El<M>, tree: &mut TaffyTree<()>, text_engine: &mut TextEngine) -> Mapped<M> {
    let mut style = el.layout;

    if let Some(spec) = &el.content.scroll {
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
    // A text leaf's intrinsic size is its shaped extent — measure once, fix the leaf size. An input
    // is the exception: it's a field with a designed size (explicit `.w()`/`.h()`), so we must NOT
    // shrink it to its (possibly empty) current text.
    if let Some(ts) = &el.content.text {
        if el.content.input.is_none() {
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
        node,
        content: el.content,
        children,
    }
}

fn emit<M>(
    m: Mapped<M>,
    tree: &TaffyTree<()>,
    ox: f32,
    oy: f32,
    out: &mut Vec<Placed<M>>,
    clip: Option<Rect>,
    scrolls: &Scrolls,
) {
    let l = tree.layout(m.node).expect("layout");
    // Taffy gives parent-relative locations; accumulate to absolute.
    let x = ox + l.location.x;
    let y = oy + l.location.y;
    let rect = Rect::new(
        x as f64,
        y as f64,
        (x + l.size.width) as f64,
        (y + l.size.height) as f64,
    );

    let content_size = (l.content_size.width, l.content_size.height);

    let (mut cx, mut cy) = (x, y);
    let mut child_clip = clip;
    if let Some(s) = &m.content.scroll {
        let scroll = scrolls.get(&s.id);
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
    out.push(Placed {
        rect,
        content: m.content,
        pad: Insets::new(
            (l.padding.left + l.border.left) as f64,
            (l.padding.top + l.border.top) as f64,
            (l.padding.right + l.border.right) as f64,
            (l.padding.bottom + l.border.bottom) as f64,
        ),
        clip,
        content_size,
    });

    for c in m.children {
        emit(c, tree, cx, cy, out, child_clip, scrolls);
    }
}

/// Lay out `root` within `viewport` (logical points); return painted nodes in paint order
/// (parents before children).
pub(crate) fn solve<M>(
    root: El<M>,
    text: &mut TextEngine,
    viewport: (f32, f32),
    scrolls: &Scrolls,
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
    emit(mapped, &tree, 0.0, 0.0, &mut out, None, scrolls);
    out
}
