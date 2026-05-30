//! The wire contract between an app (guest) and the shell (host).
//!
//! Compiled guest and host can't share a live Rust value, so they exchange
//! *bytes*: a `RawInput` crosses in (the app's pointer/keyboard events in its own
//! (0,0)-based coordinates), a [`Surface`] crosses out (GPU-ready triangles plus
//! the texture uploads they reference). This crate is the schema for those bytes
//! and the only thing both sides link — pure data and its postcard
//! (de)serialization, no host or guest logic.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// One clipped mesh — the serializable shape of an `egui::ClippedPrimitive`.
///
/// egui's `ClippedPrimitive` can't cross the wire: its `Primitive::Callback`
/// variant holds a `dyn Any` GPU paint callback with no serialization. That only
/// comes from host-side GPU painting; a sandboxed guest emits pure geometry
/// (`Primitive::Mesh`, which *is* serde), so carrying just the mesh is lossless.
#[derive(Clone, Serialize, Deserialize)]
pub struct WirePrimitive {
    pub clip_rect: egui::Rect,
    pub mesh: egui::Mesh,
}

/// One frame's drawing from an app's egui context: GPU-ready triangles plus the
/// texture uploads they reference.
///
/// NOTE: `textures_delta` is *incremental* — the font atlas ships only in the
/// first surface — so the host must apply every surface's deltas in order, never
/// skipping one (see the compositor's drain loop).
#[derive(Clone, Serialize, Deserialize)]
pub struct Surface {
    pub primitives: Vec<WirePrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
    /// egui's repaint signal for this frame: `Duration::ZERO` = animating (wants
    /// the next frame now), `Duration::MAX` = idle. The host schedules the cell's
    /// redraws from this instead of polling, so a static app costs nothing.
    pub repaint_after: Duration,
}

impl Surface {
    /// Build a wire surface from a tessellated egui frame (the guest side).
    /// Any `Callback` primitive is dropped — a guest never produces one (see
    /// [`WirePrimitive`]), so this is lossless for app geometry. `repaint_after`
    /// is the guest context's `repaint_delay` for this frame (see the field).
    pub fn from_tessellated(
        primitives: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
        pixels_per_point: f32,
        repaint_after: Duration,
    ) -> Self {
        let primitives = primitives
            .into_iter()
            .filter_map(|p| match p.primitive {
                egui::epaint::Primitive::Mesh(mesh) => Some(WirePrimitive { clip_rect: p.clip_rect, mesh }),
                egui::epaint::Primitive::Callback(_) => None,
            })
            .collect();
        Surface { primitives, textures_delta, pixels_per_point, repaint_after }
    }

    /// Rebuild egui's `ClippedPrimitive` list for the renderer (the host side).
    pub fn to_clipped_primitives(&self) -> Vec<egui::ClippedPrimitive> {
        self.primitives
            .iter()
            .map(|p| egui::ClippedPrimitive {
                clip_rect: p.clip_rect,
                primitive: egui::epaint::Primitive::Mesh(p.mesh.clone()),
            })
            .collect()
    }
}

/// Encode a frame's input for the trip into the guest.
pub fn encode_input(input: &egui::RawInput) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(input)
}

/// Decode a frame's input on the guest side.
pub fn decode_input(bytes: &[u8]) -> Result<egui::RawInput, postcard::Error> {
    postcard::from_bytes(bytes)
}

/// Encode the drawn surface for the trip back to the host.
pub fn encode_surface(surface: &Surface) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(surface)
}

/// Decode the drawn surface on the host side.
pub fn decode_surface(bytes: &[u8]) -> Result<Surface, postcard::Error> {
    postcard::from_bytes(bytes)
}

#[cfg(test)]
mod tests;
