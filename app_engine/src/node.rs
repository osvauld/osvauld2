//! The declarative view tree an app produces, and its `Style`.
//!
//! A [`Node`] is a box with a [`Style`], optional `text`, and `children` — the Rust mirror of
//! the `{tag, props, children}` table a Lua app returns. [`Style`] is a flat subset of CSS
//! (inline attributes, no cascade): layout fields map onto Taffy, paint fields are read straight
//! by the painter.

use egui::Color32;
use rich_text::Run;
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

/// The inline style on one node. Flat by design — no cascade or inheritance; a node carries
/// every value it's drawn with.
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
    /// Lower the layout-relevant fields onto a Taffy style. Every node is a flex container; a
    /// text leaf is a flex node whose size comes from a measured galley.
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

/// One node in the view tree: a styled box that is either a container (`children`) or a text
/// leaf (`text`).
///
/// `on_click` is an opaque handler id the script layer assigns (a Lua closure index); the render
/// core just carries it through layout so a click can be dispatched. `hover` / `active` are
/// paint-only state styles applied over `style` — layout uses `style` only, so a state never
/// reflows the box. `editor` makes a text leaf editable, carrying a stable id by which the engine
/// keys its retained caret/selection (the tree is rebuilt every frame, so a caret can't live on
/// the node).
#[derive(Clone, Debug)]
pub struct Node {
    pub style: Style,
    /// A text leaf's content as styled [`Run`]s (plain text is one unmarked run). `None` for a
    /// container. Rendered through `rich_text`, so any text can carry marks.
    pub text: Option<Vec<Run>>,
    pub children: Vec<Node>,
    pub on_click: Option<u32>,
    pub hover: Option<Style>,
    pub active: Option<Style>,
    /// `Some(id)` ⇒ this text leaf is an **editable field** with that stable id; `None` for
    /// plain text and containers.
    pub editor: Option<String>,
    /// `Some(id)` ⇒ this container scrolls its content vertically, offset kept across frames keyed
    /// by `id` (`""` for the common single-scroll case). The box stays its laid-out size; taller
    /// content is clipped to it and offset by the scroll position.
    pub scroll: Option<String>,
}

impl Node {
    fn base(style: Style, text: Option<Vec<Run>>) -> Self {
        Node {
            style,
            text,
            children: Vec::new(),
            on_click: None,
            hover: None,
            active: None,
            editor: None,
            scroll: None,
        }
    }

    /// A column container (children stack top-to-bottom).
    pub fn col() -> Self {
        Node::base(Style { direction: Direction::Column, ..Style::default() }, None)
    }

    /// A row container (children flow left-to-right).
    pub fn row() -> Self {
        Node::base(Style { direction: Direction::Row, ..Style::default() }, None)
    }

    /// A plain text leaf (one unmarked run), sized to its shaped/wrapped galley plus padding.
    pub fn text(s: impl Into<String>) -> Self {
        Node::base(Style::default(), Some(vec![Run::plain(s)]))
    }

    /// A text leaf from explicit styled runs (marks/colour) — for rich content.
    pub fn runs(runs: Vec<Run>) -> Self {
        Node::base(Style::default(), Some(runs))
    }

    /// An editable text leaf bound to a stable `id`: `runs` are the backing buffer's content
    /// (re-read each frame), and the engine keys this field's caret/selection and input by `id`.
    pub fn editor(id: impl Into<String>, runs: Vec<Run>) -> Self {
        let mut node = Node::base(Style::default(), Some(runs));
        node.editor = Some(id.into());
        node
    }

    /// This node's text as a plain concatenated string (runs joined), if it's a text leaf.
    pub fn plain_text(&self) -> Option<String> {
        self.text.as_ref().map(|runs| runs.iter().map(|r| r.text.as_str()).collect())
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
