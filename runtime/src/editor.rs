//! The per-field editing engine: one `parley::PlainEditor` per editable `id` (created on first
//! sight), plus which id holds keyboard focus. This is the behavior layer — it reads `InputSpec`
//! and `Placed` from the description layer (`el`/`layout`), never the reverse. Owned by the `Runner`.

use crate::id::Id;
use crate::state::Store;
use parley::style::StyleProperty;
use parley::{BoundingBox, LineHeight, PlainEditor};
use vello::kurbo::Insets;
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

use crate::scroll::{Axis, Scroll};
use crate::text::{self, TextEngine};

/// Editable-field state that persists across frames. The view tree is rebuilt every frame, so the
/// caret/buffer can't live on it — they live here, keyed by the input's stable `id`.
pub(crate) struct Field {
    editor: PlainEditor<[u8; 4]>,
    multiline: bool,
    id: Id,
    caret_dirty: bool,
}

pub(crate) struct KeepInView {
    pub axis: Axis,
    pub near: f32,
    pub far: f32,
    pub inner: f32,
    pub content: f32,
}

impl Field {
    pub fn new(size: f32, id: Id, family: &'static str, value: &str, multiline: bool) -> Self {
        let mut e = PlainEditor::new(size);
        e.edit_styles()
            .insert(StyleProperty::FontFamily(text::resolve_family(family)));
        e.edit_styles()
            .insert(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.6)));

        e.set_text(value);
        Field {
            editor: e,
            multiline: multiline,
            id,
            caret_dirty: true,
        }
    }

    pub fn sync(
        &mut self,
        width: f32,
        height: f32,
        pad: Insets,
        text: &mut TextEngine,
    ) -> Option<KeepInView> {
        let w = if self.multiline {
            Some(width - pad.x0 as f32 - pad.x1 as f32)
        } else {
            None
        };
        self.editor.set_width(w);
        let (font_cx, layout_cx) = text.contexts();
        self.editor.refresh_layout(font_cx, layout_cx);
        if self.caret_dirty {
            return self.scroll_request(width, height, pad);
        }
        None
    }
    fn scroll_request(&mut self, width: f32, height: f32, pad: Insets) -> Option<KeepInView> {
        let c = self.editor.cursor_geometry(1.5)?;
        if self.multiline {
            let text_h = self.editor.try_layout().map_or(0.0, |l| l.height());
            let inner = height - pad.y0 as f32 - pad.y1 as f32;
            return Some(KeepInView {
                inner,
                content: text_h,
                axis: Axis::Y,
                near: c.y0 as f32,
                far: c.y1 as f32,
            });
        } else {
            let text_w = self.editor.try_layout().map_or(0.0, |l| l.full_width());
            let inner = width - pad.x0 as f32 - pad.x1 as f32;
            let content = text_w.max(c.x1 as f32); // include the caret's far edge at the eol

            return Some(KeepInView {
                inner,
                content,
                axis: Axis::X,
                near: c.x0 as f32,
                far: c.x1 as f32,
            });
        }
    }
    pub fn on_key(
        &mut self,
        mods: ModifiersState,
        text: &mut TextEngine,
        key: &Key,
        event_txt: &Option<SmolStr>,
    ) {
        let multiline = self.multiline;
        let shift = mods.shift_key();
        let word = mods.control_key();
        let (font_cx, layout_cx) = text.contexts();
        let mut drv = self.editor.driver(font_cx, layout_cx);
        use NamedKey::*;
        match &key {
            Key::Named(ArrowLeft) => match (shift, word) {
                (false, false) => drv.move_left(),
                (false, true) => drv.move_word_left(),
                (true, false) => drv.select_left(),
                (true, true) => drv.select_word_left(),
            },
            Key::Named(ArrowRight) => match (shift, word) {
                (false, false) => drv.move_right(),
                (false, true) => drv.move_word_right(),
                (true, false) => drv.select_right(),
                (true, true) => drv.select_word_right(),
            },
            Key::Named(ArrowUp) => {
                if shift {
                    drv.select_up()
                } else {
                    drv.move_up()
                }
            }
            Key::Named(ArrowDown) => {
                if shift {
                    drv.select_down()
                } else {
                    drv.move_down()
                }
            }
            Key::Named(Home) => match (shift, word) {
                (false, false) => drv.move_to_line_start(),
                (false, true) => drv.move_to_text_start(),
                (true, false) => drv.select_to_line_start(),
                (true, true) => drv.select_to_text_start(),
            },
            Key::Named(End) => match (shift, word) {
                (false, false) => drv.move_to_line_end(),
                (false, true) => drv.move_to_text_end(),
                (true, false) => drv.select_to_line_end(),
                (true, true) => drv.select_to_text_end(),
            },
            Key::Named(NamedKey::Backspace) => drv.backdelete(),
            Key::Named(NamedKey::Delete) => drv.delete(),
            Key::Named(Enter) => {
                if multiline {
                    drv.insert_or_replace_selection("\n")
                } else {
                    return;
                }
            }
            Key::Character(c) if word && c.as_str() == "a" => drv.select_all(),
            _ => match &event_txt {
                Some(t) if !t.chars().any(char::is_control) => drv.insert_or_replace_selection(t),
                _ => return (),
            },
        }

        self.caret_dirty = true;
    }
    pub fn click_at(&mut self, x: f32, y: f32, text: &mut TextEngine) {
        let (font_cx, layout_cx) = text.contexts();
        self.editor.driver(font_cx, layout_cx).move_to_point(x, y);
        self.caret_dirty = true;
    }

    pub fn caret_to_end(&mut self, text: &mut TextEngine) {
        let (font_cx, layout_cx) = text.contexts();
        self.editor.driver(font_cx, layout_cx).move_to_text_end();
        self.caret_dirty = true;
    }

    pub fn extend_to(&mut self, x: f32, y: f32, text: &mut TextEngine) {
        let (font_cx, layout_cx) = text.contexts();
        self.editor
            .driver(font_cx, layout_cx)
            .extend_selection_to_point(x, y);
        self.caret_dirty = true;
    }

    pub fn layout_of(&self) -> Option<&parley::Layout<[u8; 4]>> {
        self.editor.try_layout()
    }

    pub fn selection_geometry(&self) -> Vec<(BoundingBox, usize)> {
        self.editor.selection_geometry()
    }

    pub fn text_of(&self) -> &str {
        self.editor.raw_text()
    }

    pub fn cursor_geometry(&self, size: f32) -> Option<BoundingBox> {
        self.editor.cursor_geometry(size)
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    pub fn on_ime(&mut self, s: &str, c: Option<(usize, usize)>, text: &mut TextEngine) {
        self.caret_dirty = true;
        let (font_cx, layout_cx) = text.contexts();
        let mut driver = self.editor.driver(font_cx, layout_cx);
        if s.is_empty() {
            driver.clear_compose();
        } else {
            driver.set_compose(s, c);
        }
    }
    pub fn on_ime_disabled(&mut self, text: &mut TextEngine) {
        self.caret_dirty = true;
        let (font_cx, layout_cx) = text.contexts();
        let mut driver = self.editor.driver(font_cx, layout_cx);
        driver.clear_compose();
    }

    pub fn on_ime_commit(&mut self, s: &str, text: &mut TextEngine) {
        let (font_cx, layout_cx) = text.contexts();
        self.editor
            .driver(font_cx, layout_cx)
            .insert_or_replace_selection(s);
        self.caret_dirty = true;
    }
}

pub(crate) struct Focus {
    focused: Option<Id>,
}

impl Focus {
    pub fn focused_field<'s>(&self, store: &'s mut Store) -> Option<&'s mut Field> {
        let id = self.focused.as_ref()?;
        store.get_mut::<Field>(id)
    }
    pub fn new() -> Self {
        Self { focused: None }
    }
    pub fn set(&mut self, id: Id) {
        self.focused = Some(id);
    }
    pub fn blur(&mut self) {
        self.focused = None;
    }
    pub fn get(&self) -> Option<&Id> {
        self.focused.as_ref()
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focused.as_deref() == Some(id)
    }
    pub fn clear_if_gone(&mut self, store: &Store) {
        if let Some(id) = &self.focused {
            if store.get::<Field>(id).is_none() {
                self.focused = None;
            }
        }
    }
}

pub fn sync(
    id: &Id,
    value: &str,
    width: f32,
    height: f32,
    family: &'static str,
    size: f32,
    multiline: bool,
    text: &mut TextEngine,
    pad: Insets,
    store: &mut Store,
) {
    let field = store.get_or_with::<Field>(id, || {
        Field::new(size, id.clone(), family, value, multiline)
    });
    let content_dim = field.sync(width, height, pad, text);
    if let Some(view) = content_dim {
        field.caret_dirty = false;
        let scroll = store.get_or::<Scroll>(id);
        scroll.keep_in_view(view);
    }
}
