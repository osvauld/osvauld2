//! Layout pass: turn an `El<M>` tree into a flat list of `Placed<M>` (absolute rects + paint props +
//! click message), using Taffy for the flexbox math. Text leaves are pre-measured so Taffy sizes
//! them correctly. Consumes the tree (props move into `Placed`).

use taffy::prelude::*;
use vello::kurbo::{Insets, Rect};

use crate::el::{Content, El};
use crate::text::TextEngine;

/// One positioned node, ready to paint and hit-test. `rect` is in logical points.
pub(crate) struct Placed<M> {
    pub rect: Rect,
    pub pad: Insets,
    pub content: Content<M>,
}

/// El props + its Taffy node id + mapped children, retained between build and emit.
struct Mapped<M> {
    node: NodeId,
    content: Content<M>,
    children: Vec<Mapped<M>>,
}

fn build<M>(el: El<M>, tree: &mut TaffyTree<()>, text_engine: &mut TextEngine) -> Mapped<M> {
    let mut style = el.layout;
    // A text leaf's intrinsic size is its shaped extent — measure once, fix the leaf size. An input
    // is the exception: it's a field with a designed size (explicit `.w()`/`.h()`), so we must NOT
    // shrink it to its (possibly empty) current text.
    if let Some(ts) = &el.content.text {
        if el.content.input.is_none() {
            let (w, h) = text_engine.measure(&ts.text, ts.family, ts.size);
            style.size = Size {
                width: length(w),
                height: length(h),
            };
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

fn emit<M>(m: Mapped<M>, tree: &TaffyTree<()>, ox: f32, oy: f32, out: &mut Vec<Placed<M>>) {
    let l = tree.layout(m.node).expect("layout");
    // Taffy gives parent-relative locations; accumulate to absolute.
    let x = ox + l.location.x;
    let y = oy + l.location.y;
    out.push(Placed {
        rect: Rect::new(
            x as f64,
            y as f64,
            (x + l.size.width) as f64,
            (y + l.size.height) as f64,
        ),
        content: m.content,
        pad: Insets::new(
            (l.padding.left + l.border.left) as f64,
            (l.padding.top + l.border.top) as f64,
            (l.padding.right + l.border.right) as f64,
            (l.padding.bottom + l.border.bottom) as f64,
        ),
    });
    for c in m.children {
        emit(c, tree, x, y, out);
    }
}

/// Lay out `root` within `viewport` (logical points); return painted nodes in paint order
/// (parents before children).
pub(crate) fn solve<M>(root: El<M>, text: &mut TextEngine, viewport: (f32, f32)) -> Vec<Placed<M>> {
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
    emit(mapped, &tree, 0.0, 0.0, &mut out);
    out
}
