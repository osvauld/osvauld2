//! Sound as plain sample maths: generated sounds (`Synth`) and a `Mixer` of voices on buses.
//! Mono, 48 kHz, f32. No device and no clock: whoever owns the output calls `render` — the sound
//! card's callback in a window, the virtual frame clock offscreen — so the same commands always
//! make the same samples. One bus per app; the mixer keeps each bus's event log and recent
//! samples so a test can check what an app actually played (docs/design/sound.md).

mod mixer;
mod synth;

pub use mixer::{Bus, Command, Event, FADE, Mixer, Sound, VoiceId, Why};
pub use synth::{Synth, Wave};

pub const RATE: u32 = 48_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Spec(String),
}

#[cfg(test)]
mod tests;
