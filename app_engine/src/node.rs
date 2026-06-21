//! The declarative view tree an app produces, and its `Style`.
//!
//! A [`Node`] is a box with a [`Style`], optional `text`, and `children` — the Rust mirror of
//! the `{tag, props, children}` table a Lua app returns. [`Style`] is a flat subset of CSS
//! (inline attributes, no cascade): layout fields map onto Taffy, paint fields are read straight
//! by the painter. Field names follow CSS so the vocabulary stays in-distribution for an LLM
//! author (`justify_content`, `border_radius`, …).

use egui::Color32;
use rich_text::Run;
use taffy::prelude::{auto, length, percent};
use taffy::{
    AlignItems, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, LengthPercentage,
    LengthPercentageAuto, Position as TaffyPosition, Rect as TaffyRect, Size, Style as TaffyStyle,
};

/// A length in our CSS-vocabulary: `auto` (size to content / stretch), a pixel length, or
/// a percent of the parent's corresponding axis.
#[derive(Clone, Copy, Debug, PartialEq)]
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

    /// As a Taffy `LengthPercentage` (no auto — used for padding, where `auto` means `0`).
    fn to_lp(self) -> LengthPercentage {
        match self {
            Val::Auto => length(0.0),
            Val::Px(px) => length(px),
            Val::Pct(p) => percent(p / 100.0),
        }
    }

    /// As a Taffy `LengthPercentageAuto` (margin / inset, where `auto` is meaningful).
    fn to_lpa(self) -> LengthPercentageAuto {
        match self {
            Val::Auto => auto(),
            Val::Px(px) => length(px),
            Val::Pct(p) => percent(p / 100.0),
        }
    }
}

/// Per-side values (CSS `padding` / `margin` / inset sides).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub top: Val,
    pub right: Val,
    pub bottom: Val,
    pub left: Val,
}

impl Edges {
    pub fn all(v: Val) -> Self {
        Edges { top: v, right: v, bottom: v, left: v }
    }

    pub fn zero() -> Self {
        Edges::all(Val::Px(0.0))
    }

    /// All four sides `auto` — the default for `inset`, where auto means "not set".
    pub fn unset() -> Self {
        Edges::all(Val::Auto)
    }

    pub fn px(v: f32) -> Self {
        Edges::all(Val::Px(v))
    }

    fn to_lp_rect(self) -> TaffyRect<LengthPercentage> {
        TaffyRect {
            left: self.left.to_lp(),
            right: self.right.to_lp(),
            top: self.top.to_lp(),
            bottom: self.bottom.to_lp(),
        }
    }

    fn to_lpa_rect(self) -> TaffyRect<LengthPercentageAuto> {
        TaffyRect {
            left: self.left.to_lpa(),
            right: self.right.to_lpa(),
            top: self.top.to_lpa(),
            bottom: self.bottom.to_lpa(),
        }
    }
}

/// Main-axis direction of a box's children (CSS `flex-direction`).
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Row,
    Column,
}

/// One CSS alignment keyword, shared by `justify_content` / `align_items` / `align_self`
/// (keywords that don't apply to an axis fall back to something sensible there).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
    Baseline,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Align {
    fn to_justify(self) -> JustifyContent {
        match self {
            Align::Start | Align::Baseline => JustifyContent::FlexStart,
            Align::Center => JustifyContent::Center,
            Align::End => JustifyContent::FlexEnd,
            Align::Stretch => JustifyContent::Stretch,
            Align::SpaceBetween => JustifyContent::SpaceBetween,
            Align::SpaceAround => JustifyContent::SpaceAround,
            Align::SpaceEvenly => JustifyContent::SpaceEvenly,
        }
    }

    fn to_align(self) -> AlignItems {
        match self {
            Align::Start | Align::SpaceBetween | Align::SpaceAround | Align::SpaceEvenly => {
                AlignItems::FlexStart
            }
            Align::Center => AlignItems::Center,
            Align::End => AlignItems::FlexEnd,
            Align::Stretch => AlignItems::Stretch,
            Align::Baseline => AlignItems::Baseline,
        }
    }
}

/// CSS `position`: `Absolute` takes the node out of flex flow and places it by `inset`
/// relative to its parent's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    Relative,
    Absolute,
}

/// A solid border painted just inside the box edge (CSS `border`). The width also participates
/// in layout (border-box), so content never sits under the stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color32,
}

/// A drop shadow behind the box (CSS `box-shadow`: offset, blur, spread, colour).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub color: Color32,
}

/// Per-corner radii (CSS `border-radius`, top-left first, clockwise).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
}

impl Corners {
    pub fn same(r: f32) -> Self {
        Corners { tl: r, tr: r, br: r, bl: r }
    }
}

/// The inline style on one node. Flat by design — no cascade or inheritance; a node carries
/// every value it's drawn with (the one exception is `opacity`, which multiplies down the
/// subtree like CSS).
#[derive(Clone, Debug)]
pub struct Style {
    pub direction: Direction,
    /// CSS `flex-wrap: wrap` when true.
    pub wrap: bool,
    /// Main-axis distribution of children (CSS `justify-content`). `None` = Taffy default.
    pub justify_content: Option<Align>,
    /// Cross-axis alignment of children (CSS `align-items`). `None` = Taffy default (stretch).
    pub align_items: Option<Align>,
    /// This node's own cross-axis override (CSS `align-self`).
    pub align_self: Option<Align>,
    pub width: Val,
    pub height: Val,
    pub min_width: Val,
    pub min_height: Val,
    pub max_width: Val,
    pub max_height: Val,
    /// Inner padding per side (CSS `padding`), in points.
    pub padding: Edges,
    /// Outer margin per side (CSS `margin`); `auto` centres along that axis.
    pub margin: Edges,
    pub position: Position,
    /// Offsets for `position: absolute` (CSS `top`/`right`/`bottom`/`left`); `auto` = unset.
    pub inset: Edges,
    /// Space between children along the main axis (CSS `gap`), in points.
    pub gap: f32,
    /// How much free main-axis space this node absorbs (CSS `flex-grow`).
    pub flex_grow: f32,
    /// How readily this node gives up space (CSS `flex-shrink`; CSS default `1`).
    pub flex_shrink: f32,
    pub background: Option<Color32>,
    pub corner_radius: Corners,
    pub border: Option<Border>,
    pub shadow: Option<BoxShadow>,
    /// `0.0..=1.0`; multiplies every colour this node and its subtree paint with (CSS `opacity`).
    pub opacity: f32,
    /// Text colour for this node's own `text` (not inherited by children).
    pub color: Color32,
    pub font_size: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            direction: Direction::Column,
            wrap: false,
            justify_content: None,
            align_items: None,
            align_self: None,
            width: Val::Auto,
            height: Val::Auto,
            min_width: Val::Auto,
            min_height: Val::Auto,
            max_width: Val::Auto,
            max_height: Val::Auto,
            padding: Edges::zero(),
            margin: Edges::zero(),
            position: Position::Relative,
            inset: Edges::unset(),
            gap: 0.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            background: None,
            corner_radius: Corners::default(),
            border: None,
            shadow: None,
            opacity: 1.0,
            color: Color32::from_gray(0xdd),
            font_size: 16.0,
        }
    }
}

impl Style {
    /// Lower the layout-relevant fields onto a Taffy style. Every node is a flex container; a
    /// text leaf is a flex node whose size comes from a measured galley.
    pub(crate) fn to_taffy(&self) -> TaffyStyle {
        let border = self.border.map_or(0.0, |b| b.width.max(0.0));
        TaffyStyle {
            display: Display::Flex,
            flex_direction: match self.direction {
                Direction::Row => FlexDirection::Row,
                Direction::Column => FlexDirection::Column,
            },
            flex_wrap: if self.wrap { FlexWrap::Wrap } else { FlexWrap::NoWrap },
            position: match self.position {
                Position::Relative => TaffyPosition::Relative,
                Position::Absolute => TaffyPosition::Absolute,
            },
            inset: self.inset.to_lpa_rect(),
            size: Size { width: self.width.to_dim(), height: self.height.to_dim() },
            min_size: Size { width: self.min_width.to_dim(), height: self.min_height.to_dim() },
            max_size: Size { width: self.max_width.to_dim(), height: self.max_height.to_dim() },
            padding: self.padding.to_lp_rect(),
            margin: self.margin.to_lpa_rect(),
            // Border participates in layout (border-box) so content clears the stroke.
            border: Edges::px(border).to_lp_rect(),
            gap: Size { width: length(self.gap), height: length(self.gap) },
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            justify_content: self.justify_content.map(Align::to_justify),
            align_items: self.align_items.map(Align::to_align),
            align_self: self.align_self.map(Align::to_align),
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
    /// `Some(id)` ⇒ this leaf is an embedded **block document** (`ui.doc{ id }`), rendered by a
    /// native `doc_editor` over a tree inside the app's CRDT. Layout reserves its box; the engine
    /// paints + edits it in its own child `Ui` keyed by `id`.
    pub doc: Option<String>,
    /// `Some(spec)` ⇒ this leaf is a **chart** (`ui.chart`), painted by the engine with `egui_plot`
    /// into its laid-out box (like `doc`, in its own child `Ui`). The data is resolved host-side.
    pub chart: Option<ChartSpec>,
    /// `Some(spec)` ⇒ this container scrolls its overflowing content on the spec's axes, offset
    /// kept across frames keyed by the spec's id (`""` for the common single-scroll case). The box
    /// stays its laid-out size; overflow is clipped to it and offset by the scroll position.
    pub scroll: Option<ScrollSpec>,
    /// `Some` ⇒ dragging this box's trailing edge resizes part of a table (see [`Resize`]).
    pub resize: Option<Resize>,
    /// `true` ⇒ this subtree floats above the page (a dropdown, a picker): laid out in place
    /// (use `position: absolute`) but painted after everything else, clipped only by the host.
    pub popup: bool,
}

/// What dragging a marked box's trailing edge resizes: a header cell's right edge sets its
/// column's width, a data row's bottom edge sets that row's height. `table` keys the retained
/// sizes (the list name); `row` is the stable row id.
#[derive(Clone, Debug, PartialEq)]
pub enum Resize {
    Col { table: String, key: String },
    Row { table: String, row: String },
}

/// A chart leaf's resolved data (`ui.chart`): the kind plus one or more named y-series, each
/// aligned with `x_labels`. Built host-side during the walk from a query result — the engine paints
/// it with `egui_plot`; the values are Rust-side (a small aggregate), never marshalled into Lua.
/// v1 x-axis is categorical: positions are `0..x_labels.len()`, labels shown on the ticks (covers
/// segment/month/date dashboards; a true numeric x-axis can come later).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub x_labels: Vec<String>,
    pub series: Vec<ChartSeries>,
    pub color: Color32,
}

/// A chart's render style. v1: line / bar / scatter (static, no interaction).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChartKind {
    Line,
    Bar,
    Scatter,
}

/// One named series of a chart: y values aligned 1:1 with the chart's `x_labels` (x = index).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
}

/// A scroll region declaration: which axes scroll, keyed by a retained-offset id.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollSpec {
    pub id: String,
    pub x: bool,
    pub y: bool,
}

impl ScrollSpec {
    pub fn y(id: impl Into<String>) -> Self {
        ScrollSpec { id: id.into(), x: false, y: true }
    }

    pub fn x(id: impl Into<String>) -> Self {
        ScrollSpec { id: id.into(), x: true, y: false }
    }

    pub fn both(id: impl Into<String>) -> Self {
        ScrollSpec { id: id.into(), x: true, y: true }
    }
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
            doc: None,
            chart: None,
            scroll: None,
            resize: None,
            popup: false,
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

    /// An embedded block-document leaf bound to a stable `id`. Carries no text/children — the
    /// engine renders a native `doc_editor` into its laid-out box.
    pub fn doc(id: impl Into<String>) -> Self {
        let mut node = Node::base(Style::default(), None);
        node.doc = Some(id.into());
        node
    }

    /// A chart leaf with resolved data. Carries no text/children — the engine paints it with
    /// `egui_plot` into its laid-out box.
    pub fn chart(spec: ChartSpec) -> Self {
        let mut node = Node::base(Style::default(), None);
        node.chart = Some(spec);
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
        self.style.padding = Edges::px(v);
        self
    }

    pub fn margin(mut self, v: f32) -> Self {
        self.style.margin = Edges::px(v);
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

    pub fn justify(mut self, a: Align) -> Self {
        self.style.justify_content = Some(a);
        self
    }

    pub fn align(mut self, a: Align) -> Self {
        self.style.align_items = Some(a);
        self
    }

    pub fn bg(mut self, c: Color32) -> Self {
        self.style.background = Some(c);
        self
    }

    pub fn radius(mut self, v: f32) -> Self {
        self.style.corner_radius = Corners::same(v);
        self
    }

    pub fn border(mut self, width: f32, color: Color32) -> Self {
        self.style.border = Some(Border { width, color });
        self
    }

    pub fn shadow(mut self, s: BoxShadow) -> Self {
        self.style.shadow = Some(s);
        self
    }

    pub fn opacity(mut self, v: f32) -> Self {
        self.style.opacity = v;
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
