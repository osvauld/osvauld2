use std::collections::HashMap;

use block_doc::{BlockDoc, BlockId};
use egui::Color32;

mod edit;
mod layout;
mod paint;
mod selection;

/// Colours the host supplies so the editor matches the surrounding shell.
#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Color32,
    pub gutter_bg: Color32,
    pub gutter_fg: Color32,
    pub fg: Color32,
    pub punct: Color32,
    pub rule: Color32,
    /// Selection band fill — use a translucent colour; paints behind the text.
    pub selection: Color32,
    pub caret: Color32,
}

/// Retained view state — the [`BlockDoc`] is owned by the host and handed in each frame.
#[derive(Default)]
pub struct Editor {
    hl: HashMap<BlockId, layout::CachedHl>,
    sel: Option<selection::Selection>,
    // Desired column preserved across Up/Down so short lines don't drift the caret
    preferred_x: Option<f32>,
    // Blink phase is measured from the last move so the caret is solid right after moving
    blink_origin: f64,
    // Set on every edit; host clears it with take_dirty when persisting
    dirty: bool,
}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the surface *lost focus* this frame — the host's cue to persist.
    pub fn show(&mut self, ui: &mut egui::Ui, doc: &BlockDoc, theme: &Theme) -> bool {
        paint::show(self, ui, doc, theme)
    }

    /// Take (and clear) the unsaved-edits flag — call on blur/file-switch to decide whether to write.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}
