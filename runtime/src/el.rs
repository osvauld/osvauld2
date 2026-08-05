//! The element model: a declarative tree describing *what* a screen looks like. Screens build an
//! `El` with the builder fns (`col`/`row`/`text`/`custom`) + chained setters; the runtime lays it
//! out (Taffy), paints it (vello), and routes clicks. `El<M>` is generic over a screen's message
//! type (the Elm/Iced shape): `on_click(msg)` attaches data, never behavior.
//!
//! NOTE: this mirrors `app_engine`'s `Node` design but is its own type — app_engine still depends on
//! egui, so importing it here would contaminate `runtime`. Converging the two is a later refactor.

use std::rc::Rc;
use taffy::prelude::*; // Style, Display, FlexDirection, length(), auto(), Size, Rect (geometry), …
use vello::Scene;
use vello::kurbo::Affine;
use vello::peniko::Color;

use crate::anim::{Driver, Easing};
use crate::drag::{DragEvent, DropEvent};
use crate::id::Id;
use crate::state::Slot;
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
    pub map: Option<Box<dyn Fn(String) -> M>>,
    pub multiline: bool,
    pub autofocus: bool,
    pub on_enter: Option<M>,
    pub on_esc: Option<M>,
}

// A stroked outline: width (logical px) + color. Distinct from kurbo's `Stroke`
#[derive(Clone, Copy)]
pub(crate) struct Border {
    pub width: f32,
    pub color: Color,
}

/// Visual decoration with hover variants, resolved against pointer-inside at paint time
#[derive(Default)]
pub(crate) struct Look {
    pub fill: Option<Color>,
    pub stroke: Option<Border>,
    pub radius: f32,
    pub hover_fill: Option<Color>,
    pub hover_stroke: Option<Border>,
}

impl Look {
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
    pub(crate) fn resolve_t(&self, t: f32) -> (Option<Color>, Option<Border>) {
        let (fill_a, stroke_a) = self.resolve(false);
        let (fill_b, stroke_b) = self.resolve(true);
        (
            lerp_opt_color(fill_a, fill_b, t),
            lerp_opt_border(stroke_a, stroke_b, t),
        )
    }
}
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let [ar, ag, ab, aa] = a.components;
    let [br, bg, bb, ba] = b.components;
    let l = |x: f32, y: f32| x + (y - x) * t;
    Color::new([l(ar, br), l(ag, bg), l(ab, bb), l(aa, ba)])
}
fn lerp_opt_color(a: Option<Color>, b: Option<Color>, t: f32) -> Option<Color> {
    match (a, b) {
        (Some(a), Some(b)) => Some(lerp_color(a, b, t)),
        (None, Some(b)) => Some(b.multiply_alpha(t)),
        (Some(a), None) => Some(a.multiply_alpha(1.0 - t)),
        (None, None) => None,
    }
}
fn lerp_opt_border(a: Option<Border>, b: Option<Border>, t: f32) -> Option<Border> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Border {
            width: a.width + (b.width - a.width) * t,
            color: lerp_color(a.color, b.color, t),
        }),
        (None, Some(b)) => Some(Border {
            width: b.width,
            color: b.color.multiply_alpha(t),
        }),
        (Some(a), None) => Some(Border {
            width: a.width,
            color: a.color.multiply_alpha(1.0 - t),
        }),
        (None, None) => None,
    }
}

#[derive(Clone)]
pub struct ScrollSpec {
    pub x: bool,
    pub y: bool,
}

#[derive(Default)]
pub(crate) struct Appearance {
    pub look: Look,
    pub text: Option<TextSpec>,
    pub custom: Option<CustomFn>,
}
pub struct Overlay<M> {
    pub panel: Box<El<M>>,
    pub dismiss: Option<M>,
    pub placement: Placement,
    pub anchor: Anchor,
}
pub enum Anchor {
    Element,
    Point(f32, f32),
}

pub struct Placement {
    pub side: PlacementSide,
    pub align: PlacementAlign,
}
impl Placement {
    pub fn resolve(
        &self,
        anchor: vello::kurbo::Rect,
        panel: (f32, f32),
        viewport: (f32, f32),
    ) -> (f32, f32) {
        let (mut x, mut y) = (anchor.x0 as f32, anchor.y1 as f32);
        match self.side {
            PlacementSide::Bottom => y = anchor.y1 as f32,
            PlacementSide::Top => y = anchor.y0 as f32 - panel.1,
            PlacementSide::Left => x = anchor.x0 as f32 - panel.0,
            PlacementSide::Right => x = anchor.x1 as f32,
        }
        let vertical = matches!(self.side, PlacementSide::Top | PlacementSide::Bottom);
        match self.align {
            PlacementAlign::Start => {
                if vertical {
                    x = anchor.x0 as f32
                } else {
                    y = anchor.y0 as f32
                }
            }
            PlacementAlign::Center => {
                if vertical {
                    x = (anchor.x0 + anchor.x1) as f32 / 2.0 - panel.0 / 2.0
                } else {
                    y = (anchor.y0 + anchor.y1) as f32 / 2.0 - panel.1 / 2.0
                }
            }
            PlacementAlign::End => {
                if vertical {
                    x = anchor.x1 as f32 - panel.0
                } else {
                    y = anchor.y1 as f32 - panel.1
                }
            }
        }
        if vertical {
            match self.side {
                PlacementSide::Top => {
                    if y < 0.0 {
                        y = anchor.y1 as f32
                    }
                }
                PlacementSide::Bottom => {
                    if y + panel.1 > viewport.1 {
                        y = anchor.y0 as f32 - panel.1
                    }
                }
                _ => {}
            }
            x = x.clamp(0.0, viewport.0 - panel.0);
        } else {
            match self.side {
                PlacementSide::Left => {
                    if x < 0.0 {
                        x = anchor.x1 as f32
                    }
                }
                PlacementSide::Right => {
                    if x + panel.0 > viewport.0 {
                        x = anchor.x0 as f32 - panel.0
                    }
                }
                _ => {}
            }
            y = y.clamp(0.0, viewport.1 - panel.1)
        }
        (x, y)
    }
}
pub enum PlacementSide {
    Top,
    Bottom,
    Left,
    Right,
}
pub enum PlacementAlign {
    Start,
    Center,
    End,
}

pub(crate) struct Binding<M> {
    pub driver: Driver,
    pub duration: f32,
    pub easing: Easing,
    pub on_done: Option<(f32, M)>, // fire M when the transition settles at this value
}

pub(crate) struct Behaviour<M> {
    pub on_click: Option<M>,
    pub input: Option<InputSpec<M>>,
    pub scroll: Option<ScrollSpec>,
    pub on_drag: Option<(Id, Box<dyn Fn(DragEvent) -> M>)>,
    pub on_drop: Option<(Id, Box<dyn Fn(DropEvent) -> M>)>,
    pub overlay: Option<Overlay<M>>,
    pub on_right_click: Option<Box<dyn Fn((f32, f32)) -> M>>,
    pub offset: (f32, f32), // for animation
    pub opacity: f32,
    pub slide: Option<(Binding<M>, (f32, f32))>,
    pub fade: Option<Binding<M>>,
    pub tint: Option<Binding<M>>,
}
impl<M> Behaviour<M> {
    /// Every store-backed transition on this node, paired with the slot it lives under.
    pub fn bindings(&self) -> impl Iterator<Item = (&Binding<M>, Slot)> {
        [
            self.slide.as_ref().map(|(b, _)| (b, Slot::Slide)),
            self.tint.as_ref().map(|b| (b, Slot::Tint)),
            self.fade.as_ref().map(|b| (b, Slot::Fade)),
        ]
        .into_iter()
        .flatten()
    }
}

impl<M> Default for Behaviour<M> {
    fn default() -> Self {
        Self {
            on_click: None,
            input: None,
            scroll: None,
            on_drag: None,
            overlay: None,
            on_right_click: None,
            offset: (0.0, 0.0),
            opacity: 1.0,
            slide: None,
            fade: None,
            tint: None,
            on_drop: None,
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
    e.appearance.text = Some(TextSpec {
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
    e.id = Some(id.into());
    e.behaviour.input = Some(InputSpec {
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
    e.appearance.custom = Some(Box::new(f));
    e
}

/// One node of the view tree. Layout style + paint decoration + optional text/custom content +
/// optional click message + children. Built via the free fns below and the chained setters.
pub struct El<M> {
    pub(crate) id: Option<Id>,
    pub(crate) layout: Style,
    pub(crate) appearance: Appearance,
    pub(crate) behaviour: Behaviour<M>,
    pub(crate) children: Vec<El<M>>,
}
impl<M> El<M> {
    fn new(layout: Style) -> Self {
        El {
            id: None,
            layout,
            behaviour: Behaviour::default(),
            appearance: Appearance::default(),
            children: Vec::new(),
        }
    }

    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }
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
        self.appearance.look.fill = Some(c);
        self
    }
    pub fn stroke(mut self, w: f32, c: Color) -> Self {
        self.appearance.look.stroke = Some(Border { width: w, color: c });
        self
    }
    pub fn radius(mut self, r: f32) -> Self {
        self.appearance.look.radius = r;
        self
    }
    pub fn hover_fill(mut self, c: Color) -> Self {
        self.appearance.look.hover_fill = Some(c);
        self
    }
    pub fn hover_stroke(mut self, w: f32, c: Color) -> Self {
        self.appearance.look.hover_stroke = Some(Border { width: w, color: c });
        self
    }
    pub fn tint(mut self, ms: f32) -> Self {
        self.behaviour.tint = Some(Binding {
            driver: Driver::Hover,
            duration: ms / 1000.0,
            easing: Easing::EaseOut,
            on_done: None,
        });
        self
    }
    pub fn offset(mut self, offset: (f32, f32)) -> Self {
        self.behaviour.offset = offset;
        self
    }

    pub fn slide_in(mut self, (dx, dy): (f32, f32), ms: f32) -> Self {
        self.behaviour.slide = Some((
            Binding {
                driver: Driver::Value(1.0),
                duration: ms / 1000.0,
                easing: Easing::EaseOut,
                on_done: None,
            },
            (dx, dy),
        ));
        self
    }
    /// Drive this element's opacity toward `to` (0..1). The app sets the target from its own
    /// state; the runtime owns the tween.
    pub fn fade(mut self, to: f32, ms: f32) -> Self {
        self.behaviour.fade = Some(Binding {
            driver: Driver::Value(to),
            duration: ms / 1000.0,
            easing: Easing::EaseOut,
            on_done: None,
        });
        self
    }
    pub fn fade_in(self, ms: f32) -> Self {
        self.fade(1.0, ms)
    }

    /// Fires whenever the fade settles — on arrival at 0
    pub fn on_faded_out(mut self, m: M) -> Self {
        if let Some(b) = &mut self.behaviour.fade {
            b.on_done = Some((0.0, m));
        }
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.behaviour.opacity = opacity;
        self
    }

    // ── text styling (no-ops on non-text elements) ──────────────────────
    pub fn font(mut self, family: &'static str) -> Self {
        if let Some(t) = &mut self.appearance.text {
            t.family = family;
        }
        self
    }
    pub fn font_size(mut self, sz: f32) -> Self {
        if let Some(t) = &mut self.appearance.text {
            t.size = sz;
        }
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        if let Some(t) = &mut self.appearance.text {
            t.color = c;
        }
        self
    }

    // ── interaction + nesting ───────────────────────────────────────────
    pub fn on_click(mut self, m: M) -> Self {
        self.behaviour.on_click = Some(m);
        self
    }

    pub fn on_drag(mut self, id: impl Into<Id>, map: impl Fn(DragEvent) -> M + 'static) -> Self {
        self.behaviour.on_drag = Some((id.into(), Box::new(map)));
        self
    }

    pub fn on_drop(mut self, id: impl Into<Id>, map: impl Fn(DropEvent) -> M + 'static) -> Self {
        self.behaviour.on_drop = Some((id.into(), Box::new(map)));
        self
    }

    pub fn on_right_click(mut self, ctx: impl Fn((f32, f32)) -> M + 'static) -> Self {
        self.behaviour.on_right_click = Some(Box::new(ctx));
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

    pub fn scroll_y(mut self) -> Self {
        let s = self
            .behaviour
            .scroll
            .get_or_insert(ScrollSpec { x: false, y: false });
        s.y = true;
        self
    }

    pub fn scroll_x(mut self) -> Self {
        let s = self
            .behaviour
            .scroll
            .get_or_insert(ScrollSpec { y: false, x: false });
        s.x = true;
        self
    }
    pub fn autofocus(mut self) -> Self {
        if let Some(spec) = &mut self.behaviour.input {
            spec.autofocus = true;
        }
        self
    }
    pub fn on_enter(mut self, m: M) -> Self {
        if let Some(spec) = &mut self.behaviour.input {
            spec.on_enter = Some(m)
        }
        self
    }

    pub fn on_esc(mut self, m: M) -> Self {
        if let Some(spec) = &mut self.behaviour.input {
            spec.on_esc = Some(m)
        }
        self
    }

    pub fn overlay(
        mut self,
        panel: El<M>,
        m: Option<M>,
        placement: Placement,
        anchor: Anchor,
    ) -> Self {
        self.behaviour.overlay = Some(Overlay {
            panel: Box::new(panel),
            dismiss: m,
            placement,
            anchor,
        });
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
            appearance,
            behaviour,
            children,
            id,
        } = self;
        let Behaviour {
            on_click,
            input,
            scroll,
            on_drag,
            on_drop,
            overlay,
            on_right_click,
            offset,
            opacity,
            slide,
            fade,
            tint,
        } = behaviour;
        let r_f = Rc::clone(&f);
        let new_drag = on_drag.map(|(id, d)| {
            (
                id,
                Box::new(move |d_e| r_f(d(d_e))) as Box<dyn Fn(DragEvent) -> B>,
            )
        });

        let r_f = Rc::clone(&f);
        let new_drop = on_drop.map(|(id, d)| {
            (
                id,
                Box::new(move |d_e| r_f(d(d_e))) as Box<dyn Fn(DropEvent) -> B>,
            )
        });
        let on_click = on_click.map(|m| f(m));

        let rc = Rc::clone(&f);
        let on_right_click =
            on_right_click.map(|m| Box::new(move |s| rc(m(s))) as Box<dyn Fn((f32, f32)) -> B>);
        let input = match input {
            Some(InputSpec {
                map,
                multiline,
                on_enter,
                on_esc,
                autofocus,
            }) => {
                let r_f = Rc::clone(&f);
                let new_map = map.map(|g| Box::new(move |s| r_f(g(s))) as Box<dyn Fn(String) -> B>);

                Some(InputSpec {
                    multiline,
                    autofocus,
                    on_enter: on_enter.map(|e| f(e)),
                    on_esc: on_esc.map(|e| f(e)),
                    map: new_map,
                })
            }
            None => None,
        };
        let new_overlay = overlay.map(|o| Overlay {
            panel: Box::new((*o.panel).map_rc(Rc::clone(&f))),
            dismiss: o.dismiss.map(|d| f(d)),
            placement: o.placement,
            anchor: o.anchor,
        });

        let new_slide = slide.map(|(binding, (dx, dy))| {
            (
                Binding {
                    driver: binding.driver,
                    easing: binding.easing,
                    on_done: binding.on_done.map(|(t, m)| (t, f(m))),
                    duration: binding.duration,
                },
                (dx, dy),
            )
        });
        let new_fade = fade.map(|fa| Binding {
            driver: fa.driver,
            duration: fa.duration,
            easing: fa.easing,
            on_done: fa.on_done.map(|(t, m)| (t, f(m))),
        });

        let new_tint = tint.map(|t| Binding {
            driver: t.driver,
            duration: t.duration,
            easing: t.easing,
            on_done: t.on_done.map(|(t, m)| (t, f(m))),
        });

        let behaviour = Behaviour {
            on_click,
            input,
            scroll,
            on_drag: new_drag,
            on_drop: new_drop,
            overlay: new_overlay,
            on_right_click,
            offset,
            opacity,
            slide: new_slide,
            fade: new_fade,
            tint: new_tint,
        };
        let mut converted_children: Vec<El<B>> = Vec::new();
        for child in children {
            converted_children.push(child.map_rc(Rc::clone(&f)));
        }
        El {
            layout,
            appearance,
            behaviour,
            children: converted_children,
            id,
        }
    }
}
