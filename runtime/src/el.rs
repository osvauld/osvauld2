//! The element model: a declarative tree describing *what* a screen looks like. Screens build an
//! `El` with the builder fns (`col`/`row`/`text`/`custom`) + chained setters; the runtime lays it
//! out (Taffy), paints it (vello), and routes clicks. `El<M>` is generic over a screen's message
//! type (the Elm/Iced shape): `on_click(msg)` attaches data, never behavior.
//!
//! NOTE: this mirrors `app_engine`'s `Node` design but is its own type — app_engine still depends on
//! egui, so importing it here would contaminate `runtime`. Converging the two is a later refactor.

use std::rc::Rc;
use taffy::prelude::*; // Style, Display, FlexDirection, length(), auto(), Size, Rect (geometry), …
use vello::kurbo::Affine;
use vello::peniko::Color;
use vello::Scene;

use crate::id::Id;
/// A text leaf's content + face. The real color is applied at vello draw time.
pub(crate) struct TextSpec {
    pub text: String,
    pub family: &'static str,
    pub size: f32,
    pub color: Color,
}

/// Escape hatch: draw arbitrary vello (shapes and/or text) into the element's computed rect — e.g.
/// the identicon, or the wordmark's offset-shadow double-draw.
pub(crate) type CustomFn =
    Box<dyn Fn(&mut Scene, &mut crate::text::TextEngine, vello::kurbo::Rect, Affine)>;

/// Marks an `El` as an editable text field. The display text + face live in the element's `text`
/// (`TextSpec`); this only carries the focus identity and how to message an edit.
pub(crate) struct InputSpec<M> {
    pub id: Id,
    pub map: Option<Box<dyn Fn(String) -> M>>,
    pub multiline: bool,
    pub autofocus: bool,
    pub on_enter: Option<M>,
    pub on_esc: Option<M>,
}

// A stroked outline: width (logical px) + color. Distinct from kurbo's `Stroke`
#[derive(Clone, Copy)]
pub(crate) struct Border {
    pub width: f64,
    pub color: Color,
}

/// Visual decoration with hover variants, resolved against pointer-inside at paint time
pub(crate) struct Look {
    pub fill: Option<Color>,
    pub stroke: Option<Border>,
    pub radius: f64,
    pub hover_fill: Option<Color>,
    pub hover_stroke: Option<Border>,
}

impl Look {
    fn new() -> Self {
        Self {
            fill: None,
            stroke: None,
            radius: 0.0,
            hover_fill: None,
            hover_stroke: None,
        }
    }
    pub(crate) fn resolve(&self, over: bool) -> (Option<Color>, Option<Border>) {
        if over {
            (
                self.hover_fill.or(self.fill),
                self.hover_stroke.or(self.stroke),
            )
        } else {
            (self.fill, self.stroke)
        }
    }
}

#[derive(Clone)]
pub struct ScrollSpec {
    pub id: Id,
    pub x: bool,
    pub y: bool,
}

pub(crate) struct Content<M> {
    pub look: Look,
    pub text: Option<TextSpec>,
    pub custom: Option<CustomFn>,
    pub on_click: Option<M>,
    pub input: Option<InputSpec<M>>,
    pub scroll: Option<ScrollSpec>,
}

impl<M> Content<M> {
    fn new() -> Self {
        Self {
            look: Look::new(),
            text: None,
            custom: None,
            on_click: None,
            input: None,
            scroll: None,
        }
    }
}

/// One node of the view tree. Layout style + paint decoration + optional text/custom content +
/// optional click message + children. Built via the free fns below and the chained setters.
pub struct El<M> {
    pub(crate) layout: Style,
    pub(crate) content: Content<M>,
    pub(crate) children: Vec<El<M>>,
}

impl<M> El<M> {
    fn new(layout: Style) -> Self {
        El {
            layout,
            content: Content::new(),
            children: Vec::new(),
        }
    }
}

/// A vertical flex container.
pub fn col<M>() -> El<M> {
    El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        ..Default::default()
    })
}

/// A horizontal flex container.
pub fn row<M>() -> El<M> {
    El::new(Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        ..Default::default()
    })
}

/// A single-line text leaf (default face = UI sans, 15px, white; override with `.font/.font_size/.color`).
pub fn text<M>(s: impl Into<String>) -> El<M> {
    let mut e = El::new(Style::default());
    e.content.text = Some(TextSpec {
        text: s.into(),
        family: crate::UI_FAMILY,
        size: 15.0,
        color: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF),
    });
    e
}

pub fn input<M>(
    value: impl Into<String>,
    id: impl Into<Id>,
    map: impl Fn(String) -> M + 'static,
    multiline: bool,
) -> El<M> {
    let mut e = text(value);
    e.content.input = Some(InputSpec {
        id: id.into(),
        map: Some(Box::new(map)),
        multiline,
        autofocus: false,
        on_enter: None,
        on_esc: None,
    });
    e
}
pub fn text_input<M>(
    value: impl Into<String>,
    id: impl Into<Id>,
    map: impl Fn(String) -> M + 'static,
) -> El<M> {
    input(value, id, map, false)
}
pub fn text_area<M>(
    value: impl Into<String>,
    id: impl Into<Id>,
    map: impl Fn(String) -> M + 'static,
) -> El<M> {
    input(value, id, map, true)
}

/// A leaf that paints itself via `f`, given the scene, text engine, its computed rect, and the
/// scene transform.
pub fn custom<M>(
    f: impl Fn(&mut Scene, &mut crate::text::TextEngine, vello::kurbo::Rect, Affine) + 'static,
) -> El<M> {
    let mut e = El::new(Style::default());
    e.content.custom = Some(Box::new(f));
    e
}

impl<M> El<M> {
    // ── layout ──────────────────────────────────────────────────────────
    /// Gap between children (both axes; only the main axis matters for a single-direction flex).
    pub fn gap(mut self, g: f32) -> Self {
        self.layout.gap = Size {
            width: length(g),
            height: length(g),
        };
        self
    }
    /// Uniform padding.
    pub fn pad(mut self, p: f32) -> Self {
        self.layout.padding = Rect {
            left: length(p),
            right: length(p),
            top: length(p),
            bottom: length(p),
        };
        self
    }
    /// Horizontal (left+right) padding only.
    pub fn px(mut self, p: f32) -> Self {
        self.layout.padding.left = length(p);
        self.layout.padding.right = length(p);
        self
    }
    pub fn py(mut self, p: f32) -> Self {
        self.layout.padding.top = length(p);
        self.layout.padding.bottom = length(p);
        self
    }
    pub fn w(mut self, v: f32) -> Self {
        self.layout.size.width = length(v);
        self
    }
    pub fn h(mut self, v: f32) -> Self {
        self.layout.size.height = length(v);
        self
    }
    pub fn size(self, w: f32, h: f32) -> Self {
        self.w(w).h(h)
    }
    /// Fill the available width / height (100%).
    pub fn w_full(mut self) -> Self {
        self.layout.size.width = percent(1.0);
        self
    }
    pub fn h_full(mut self) -> Self {
        self.layout.size.height = percent(1.0);
        self
    }
    pub fn full(self) -> Self {
        self.w_full().h_full()
    }
    /// Grow to absorb free space along the main axis (e.g. a spacer pushing siblings apart).
    pub fn grow(mut self) -> Self {
        self.layout.flex_grow = 1.0;
        self
    }
    /// Center children on the cross axis only (e.g. vertically centering a row's contents).
    pub fn align_center(mut self) -> Self {
        self.layout.align_items = Some(AlignItems::CENTER);
        self
    }
    /// Margin above / below this element.
    pub fn mt(mut self, v: f32) -> Self {
        self.layout.margin.top = length(v);
        self
    }
    pub fn mb(mut self, v: f32) -> Self {
        self.layout.margin.bottom = length(v);
        self
    }
    /// Center children on both axes.
    pub fn center(mut self) -> Self {
        self.layout.justify_content = Some(JustifyContent::CENTER);
        self.layout.align_items = Some(AlignItems::CENTER);
        self
    }
    /// Stretch children across the cross axis (e.g. rows filling a fixed-width column).
    pub fn stretch(mut self) -> Self {
        self.layout.align_items = Some(AlignItems::STRETCH);
        self
    }
    // Absolute positioning (for corner stamps): taken out of flex flow, inset from the root edges.
    pub fn absolute(mut self) -> Self {
        self.layout.position = Position::Absolute;
        self
    }
    pub fn top(mut self, v: f32) -> Self {
        self.layout.inset.top = length(v);
        self
    }
    pub fn left(mut self, v: f32) -> Self {
        self.layout.inset.left = length(v);
        self
    }
    pub fn right(mut self, v: f32) -> Self {
        self.layout.inset.right = length(v);
        self
    }
    pub fn bottom(mut self, v: f32) -> Self {
        self.layout.inset.bottom = length(v);
        self
    }

    // ── decoration ──────────────────────────────────────────────────────
    pub fn fill(mut self, c: Color) -> Self {
        self.content.look.fill = Some(c);
        self
    }
    pub fn stroke(mut self, w: f64, c: Color) -> Self {
        self.content.look.stroke = Some(Border { width: w, color: c });
        self
    }
    pub fn radius(mut self, r: f64) -> Self {
        self.content.look.radius = r;
        self
    }
    pub fn hover_fill(mut self, c: Color) -> Self {
        self.content.look.hover_fill = Some(c);
        self
    }
    pub fn hover_stroke(mut self, w: f64, c: Color) -> Self {
        self.content.look.hover_stroke = Some(Border { width: w, color: c });
        self
    }

    // ── text styling (no-ops on non-text elements) ──────────────────────
    pub fn font(mut self, family: &'static str) -> Self {
        if let Some(t) = &mut self.content.text {
            t.family = family;
        }
        self
    }
    pub fn font_size(mut self, sz: f32) -> Self {
        if let Some(t) = &mut self.content.text {
            t.size = sz;
        }
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        if let Some(t) = &mut self.content.text {
            t.color = c;
        }
        self
    }

    // ── interaction + nesting ───────────────────────────────────────────
    pub fn on_click(mut self, m: M) -> Self {
        self.content.on_click = Some(m);
        self
    }
    pub fn child(mut self, c: El<M>) -> Self {
        self.children.push(c);
        self
    }
    pub fn children(mut self, cs: impl IntoIterator<Item = El<M>>) -> Self {
        self.children.extend(cs);
        self
    }

    pub fn scroll_y(mut self, id: impl Into<Id>) -> Self {
        let s = self.content.scroll.get_or_insert(ScrollSpec {
            id: id.into(),
            x: false,
            y: false,
        });
        s.y = true;
        self
    }

    pub fn scroll_x(mut self, id: impl Into<Id>) -> Self {
        let s = self.content.scroll.get_or_insert(ScrollSpec {
            id: id.into(),
            y: false,
            x: false,
        });
        s.x = true;
        self
    }
    pub fn autofocus(mut self) -> Self {
        if let Some(spec) = &mut self.content.input {
            spec.autofocus = true;
        }
        self
    }
    pub fn on_enter(mut self, m: M) -> Self {
        if let Some(spec) = &mut self.content.input {
            spec.on_enter = Some(m)
        }
        self
    }

    pub fn on_esc(mut self, m: M) -> Self {
        if let Some(spec) = &mut self.content.input {
            spec.on_esc = Some(m)
        }
        self
    }
    pub fn map<B: 'static>(self, f: impl Fn(M) -> B + 'static) -> El<B>
    where
        M: 'static,
    {
        self.map_rc(Rc::new(f))
    }
    fn map_rc<B: 'static>(self, f: Rc<dyn Fn(M) -> B>) -> El<B>
    where
        M: 'static,
    {
        let El {
            layout,
            content,
            children,
        } = self;
        let Content {
            look,
            text,
            custom,
            on_click,
            input,
            scroll,
        } = content;
        let on_click = on_click.map(|m| f(m));
        let input = match input {
            Some(InputSpec {
                id,
                map,
                multiline,
                on_enter,
                on_esc,
                autofocus,
            }) => {
                let r_f = Rc::clone(&f);
                let new_map = map.map(|g| Box::new(move |s| r_f(g(s))) as Box<dyn Fn(String) -> B>);

                Some(InputSpec {
                    id,
                    multiline,
                    autofocus,
                    on_enter: on_enter.map(|e| f(e)),
                    on_esc: on_esc.map(|e| f(e)),
                    map: new_map,
                })
            }
            None => None,
        };

        let content = Content {
            look,
            text,
            custom,
            on_click,
            input,
            scroll,
        };

        let mut converted_children: Vec<El<B>> = Vec::new();
        for child in children {
            converted_children.push(child.map_rc(Rc::clone(&f)));
        }
        El {
            layout,
            content,
            children: converted_children,
        }
    }
}
