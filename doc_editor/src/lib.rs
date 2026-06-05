//! The `.doc` block editor — Osvauld's native, built-in document app.
//!
//! This crate is the editing **surface** only: a vertical column of typed blocks you
//! type into, navigate with the caret, split/merge with Enter/Backspace, and turn into
//! headings with Markdown shortcuts. The shell owns the chrome around it (tabs,
//! navigation, the app cell); the editor owns only what's inside the cell.
//!
//! Rendering is **egui-native**: each block lays out into an egui [`egui::Galley`],
//! which gives shaping, wrapping, caret geometry, and hit-testing for free. (Richer
//! shaping via cosmic-text is a later upgrade that slots in behind the same block
//! model — the brief's "egui + cosmic-text" north-star.)
//!
//! It depends only on `egui`, so it can be dropped into any `egui::Ui` — today the
//! standalone runner (`examples/standalone.rs`), tomorrow a compositor app-cell.

mod block;
mod editor;
mod model;
mod overlays;
pub mod theme;

pub use editor::DocEditor;
pub use model::{BlockKind, Doc, Run};
