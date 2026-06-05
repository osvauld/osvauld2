//! The declarative view tree an app produces, and its `Style`.
//!
//! A [`Node`] is a box: it has a [`Style`], optional `text`, and `children`. This is the
//! Rust mirror of the `{tag, props, children}` table a Lua app will eventually return —
//! built by hand for now so the render spine (layout → paint) can be proven before the
//! script layer lands.
//!
//! [`Style`] is a small, **flat** subset of CSS — inline attributes, no cascade ("CSS-like
//! attributes are enough"). The layout fields map onto Taffy; the paint fields
//! (`background`, `corner_radius`, `color`, `font_size`) are read straight by the painter.
//! The builder methods (`col`/`row`/`text`, then `.padding(..)`, `.bg(..)`, …) keep a tree
//! readable to write by hand.

use egui::Color32;
use taffy::prelude::{auto, length, percent};
use taffy::{Dimension, Display, FlexDirection, Rect as TaffyRect, Size, Style as TaffyStyle};

/// A length in our CSS-vocabulary: `auto` (size to content / stretch), a pixel length, or
/// a percent of the parent's corresponding axis.
#[derive(Clone, Copy, Debug)]
pub enum Val {
    Auto,
    Px(f32),
    /// Percent of the parent axis, `0.0..=100.0`.
    Pct(f32),
}

impl Val {
    fn to_dim(self) -> Dimension {
        match self {
            Val::Auto => auto(),
            Val::Px(px) => length(px),
            Val::Pct(p) => percent(p / 100.0),
        }
    }
}

/// Main-axis direction of a box's children (CSS `flex-direction`).
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Row,
    Column,
}

/// The inline style on one node. Flat by design — there is no cascade or inheritance; a
/// node carries every value it's drawn with.
#[derive(Clone, Debug)]
pub struct Style {
    pub direction: Direction,
    pub width: Val,
    pub height: Val,
    /// Uniform inner padding (CSS `padding`), in points.
    pub padding: f32,
    /// Space between children along the main axis (CSS `gap`), in points.
    pub gap: f32,
    /// How much free main-axis space this node absorbs (CSS `flex-grow`).
    pub flex_grow: f32,
    pub background: Option<Color32>,
    pub corner_radius: f32,
    /// Text colour for this node's own `text` (not inherited by children).
    pub color: Color32,
    pub font_size: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            direction: Direction::Column,
            width: Val::Auto,
            height: Val::Auto,
            padding: 0.0,
            gap: 0.0,
            flex_grow: 0.0,
            background: None,
            corner_radius: 0.0,
            color: Color32::from_gray(0xdd),
            font_size: 16.0,
        }
    }
}

impl Style {
    /// Lower the layout-relevant fields onto a Taffy style. Every node is a flex container;
    /// a text leaf is just a flex node whose size comes from a measured galley.
    pub(crate) fn to_taffy(&self) -> TaffyStyle {
        TaffyStyle {
            display: Display::Flex,
            flex_direction: match self.direction {
                Direction::Row => FlexDirection::Row,
                Direction::Column => FlexDirection::Column,
            },
            size: Size { width: self.width.to_dim(), height: self.height.to_dim() },
            padding: TaffyRect {
                left: length(self.padding),
                right: length(self.padding),
                top: length(self.padding),
                bottom: length(self.padding),
            },
            gap: Size { width: length(self.gap), height: length(self.gap) },
            flex_grow: self.flex_grow,
            ..Default::default()
        }
    }
}

/// One node in the view tree: a styled box that is either a container (`children`) or a
/// text leaf (`text`).
#[derive(Clone, Debug)]
pub struct Node {
    pub style: Style,
    pub text: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    /// A column container (children stack top-to-bottom).
    pub fn col() -> Self {
        Node { style: Style { direction: Direction::Column, ..Style::default() }, text: None, children: Vec::new() }
    }

    /// A row container (children flow left-to-right).
    pub fn row() -> Self {
        Node { style: Style { direction: Direction::Row, ..Style::default() }, text: None, children: Vec::new() }
    }

    /// A text leaf. Its box sizes to the shaped, wrapped galley plus this node's padding.
    pub fn text(s: impl Into<String>) -> Self {
        Node { style: Style::default(), text: Some(s.into()), children: Vec::new() }
    }

    pub fn children(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }

    pub fn width(mut self, v: Val) -> Self {
        self.style.width = v;
        self
    }

    pub fn height(mut self, v: Val) -> Self {
        self.style.height = v;
        self
    }

    pub fn padding(mut self, v: f32) -> Self {
        self.style.padding = v;
        self
    }

    pub fn gap(mut self, v: f32) -> Self {
        self.style.gap = v;
        self
    }

    pub fn grow(mut self, v: f32) -> Self {
        self.style.flex_grow = v;
        self
    }

    pub fn bg(mut self, c: Color32) -> Self {
        self.style.background = Some(c);
        self
    }

    pub fn radius(mut self, v: f32) -> Self {
        self.style.corner_radius = v;
        self
    }

    pub fn color(mut self, c: Color32) -> Self {
        self.style.color = c;
        self
    }

    /// Set this node's font size (named `font` to avoid colliding with `width`/`height`).
    pub fn font(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self
    }
}
