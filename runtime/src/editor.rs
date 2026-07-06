//! The per-field editing engine: one `parley::PlainEditor` per editable `id` (created on first
//! sight), plus which id holds keyboard focus. This is the behavior layer — it reads `InputSpec`
//! and `Placed` from the description layer (`el`/`layout`), never the reverse. Owned by the `Runner`.

use std::collections::HashMap;

use parley::style::StyleProperty;
use parley::{BoundingBox, LineHeight, PlainEditor};
use vello::kurbo::Insets;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

use crate::scroll::{self, Axis, Scrolls};
use crate::text::{self, TextEngine};

/// Editable-field state that persists across frames. The view tree is rebuilt every frame, so the
/// caret/buffer can't live on it — they live here, keyed by the input's stable `id`.
struct Field {
    editor: PlainEditor<[u8; 4]>,
    multiline: bool,
    id: &'static str,
    caret_dirty: bool,
}

pub(crate) struct Editors {
    map: HashMap<&'static str, Field>,
    focused: Option<&'static str>,
}

impl Field {
    pub fn new(
        size: f32,
        id: &'static str,
        family: &'static str,
        value: &str,
        multiline: bool,
    ) -> Self {
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
        scrolls: &mut Scrolls,
    ) {
        let w = if self.multiline {
            Some(width - pad.x0 as f32 - pad.x1 as f32)
        } else {
            None
        };
        self.editor.set_width(w);
        let (font_cx, layout_cx) = text.contexts();
        self.editor.refresh_layout(font_cx, layout_cx);
        if self.caret_dirty {
            self.scroll_to_caret(width, height, pad, scrolls);
            self.caret_dirty = false;
        }
    }
    fn scroll_to_caret(&mut self, width: f32, height: f32, pad: Insets, scrolls: &mut Scrolls) {
        let Some(c) = self.editor.cursor_geometry(1.5) else {
            return;
        };
        if self.multiline {
            let text_h = self.editor.try_layout().map_or(0.0, |l| l.height());
            let inner = height - pad.y0 as f32 - pad.y1 as f32;
            scrolls.keep_in_view(self.id, Axis::Y, c.y0 as f32, c.y1 as f32, inner, text_h);
        } else {
            let text_w = self.editor.try_layout().map_or(0.0, |l| l.full_width());
            let inner = width - pad.x0 as f32 - pad.x1 as f32;
            let content = text_w.max(c.x1 as f32); // include the caret's far edge at the eol
            scrolls.keep_in_view(self.id, Axis::X, c.x0 as f32, c.x1 as f32, inner, content);
        }
    }
    pub fn on_key(
        &mut self,
        mods: ModifiersState,
        text: &mut TextEngine,
        key: &Key,
        event_txt: &Option<SmolStr>,
    ) -> () {
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
}

impl Editors {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            focused: None,
        }
    }

    pub fn sync(
        &mut self,
        id: &'static str,
        value: &str,
        width: f32,
        height: f32,
        family: &'static str,
        size: f32,
        multiline: bool,
        text: &mut TextEngine,
        pad: Insets,
        scrolls: &mut Scrolls,
    ) {
        let field = self
            .map
            .entry(id)
            .or_insert_with(|| Field::new(size, id, family, value, multiline));
        field.sync(width, height, pad, text, scrolls);
    }
    pub fn focus(&mut self, id: &'static str) {
        self.focused = Some(id);
    }

    pub fn layout_of(&self, id: &str) -> Option<&parley::Layout<[u8; 4]>> {
        self.map.get(id).and_then(|f| f.editor.try_layout())
    }

    pub fn blur(&mut self) {
        self.focused = None;
    }

    pub fn is_multiline(&self, id: &str) -> bool {
        self.map.get(id).map(|f| f.multiline).unwrap_or(false)
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focused == Some(id)
    }

    pub fn text_of(&self, id: &str) -> Option<&str> {
        self.map.get(id).map(|e| e.editor.raw_text())
    }

    pub fn cursor_geometry(&self, id: &str, size: f32) -> Option<BoundingBox> {
        self.map
            .get(id)
            .and_then(|e| e.editor.cursor_geometry(size))
    }
    pub fn selection_geometry(&self, id: &str) -> Vec<(BoundingBox, usize)> {
        self.map
            .get(id)
            .map(|e| e.editor.selection_geometry())
            .unwrap_or_default()
    }
    pub fn on_key(
        &mut self,
        event: &KeyEvent,
        mods: ModifiersState,
        text: &mut TextEngine,
    ) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let Some(id) = self.focused else {
            return false;
        };
        let Some(e) = self.map.get_mut(id) else {
            return false;
        };
        e.on_key(mods, text, &event.logical_key, &event.text);
        true
    }

    pub fn click_at(&mut self, id: &str, x: f32, y: f32, text: &mut TextEngine) {
        if let Some(field) = self.map.get_mut(id) {
            let (font_cx, layout_cx) = text.contexts();
            field.editor.driver(font_cx, layout_cx).move_to_point(x, y);
            field.caret_dirty = true;
        }
    }
    pub fn extend_to(&mut self, id: &str, x: f32, y: f32, text: &mut TextEngine) {
        if let Some(field) = self.map.get_mut(id) {
            let (font_cx, layout_cx) = text.contexts();
            field
                .editor
                .driver(font_cx, layout_cx)
                .extend_selection_to_point(x, y);
            field.caret_dirty = true;
        }
    }

    pub fn focused_id(&self) -> Option<&'static str> {
        self.focused
    }

    pub fn on_ime(&mut self, s: &str, c: Option<(usize, usize)>, text: &mut TextEngine) {
        if let Some(id) = self.focused {
            if let Some(field) = self.map.get_mut(id) {
                field.caret_dirty = true;
                let (font_cx, layout_cx) = text.contexts();
                let mut driver = field.editor.driver(font_cx, layout_cx);
                if s.is_empty() {
                    driver.clear_compose();
                } else {
                    driver.set_compose(s, c);
                }
            }
        };
    }
    pub fn on_ime_disabled(&mut self, text: &mut TextEngine) {
        if let Some(id) = self.focused {
            if let Some(field) = self.map.get_mut(id) {
                field.caret_dirty = true;
                let (font_cx, layout_cx) = text.contexts();
                let mut driver = field.editor.driver(font_cx, layout_cx);
                driver.clear_compose();
            }
        }
    }

    pub fn on_ime_commit(&mut self, s: &str, text: &mut TextEngine) {
        if let Some(id) = self.focused {
            if let Some(field) = self.map.get_mut(id) {
                let (font_cx, layout_cx) = text.contexts();
                field
                    .editor
                    .driver(font_cx, layout_cx)
                    .insert_or_replace_selection(s);
                field.caret_dirty = true;
            }
        };
    }
}
