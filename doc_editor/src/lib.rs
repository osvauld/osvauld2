//! The `.doc` block editor — Osvauld's native, built-in document app.
//!
//! The editing surface only: a vertical column of typed blocks. The shell owns the chrome
//! (tabs, navigation, app cell); the editor owns what's inside the cell. Rendering is
//! egui-native — each block lays out into a [`egui::Galley`] for shaping/wrapping/caret/hit-test
//! (richer cosmic-text shaping slots in behind the same block model later). Depends only on
//! `egui`, so it drops into any `egui::Ui`.

mod block;
mod editor;
mod model;
mod overlays;
pub mod pdf;
mod scene;
pub mod theme;

pub use editor::DocEditor;
pub use loro::TreeID;
pub use model::{BlockKind, Doc};
pub use pdf::{export_pdf, PdfFonts};
/// The styled-run read shape, shared with the Lua-app renderer (a block's text comes back as
/// these via [`Doc::runs`]).
pub use rich_text::Run;
