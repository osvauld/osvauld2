use std::str::FromStr;
use std::sync::Arc;

use nnnoiseless::DenoiseState;
use webrtc_audio_processing::Processor;
use webrtc_audio_processing::config::{Config, EchoCanceller, HighPassFilter, NoiseSuppression};

use crate::{APM_FRAME, Error};

// RNNoise's speech probability above which the gate opens.
const VAD_OPEN: f32 = 0.6;
// 10 ms frames the gate stays open after the last speech, so word endings aren't clipped.
const HANGOVER: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Off,
    /// WebRTC's audio processing module (APM): echo + high-pass + its noise suppressor.
    Apm,
    /// APM echo + high-pass, then RNNoise.
    Rnn,
    /// `Rnn` plus a voice gate.
    RnnGate,
}

impl FromStr for Mode {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        Ok(match s {
            "none" => Mode::Off,
            "apm" => Mode::Apm,
            "rnn" => Mode::Rnn,
            "rnn-gate" => Mode::RnnGate,
            _ => return Err(Error::Usage),
        })
    }
}

pub struct Cleaner {
    mode: Mode,
    apm: Arc<Processor>,
    rnn: Box<DenoiseState<'static>>,
    scaled: [f32; APM_FRAME],
    hold: u32,
    gain: f32,
}

impl Cleaner {
    /// `apm` is shared with the speaker side, which feeds it the echo reference.
    pub fn new(mode: Mode, apm: Arc<Processor>) -> Self {
        apm.set_config(Config {
            echo_canceller: Some(EchoCanceller::default()),
            high_pass_filter: Some(HighPassFilter::default()),
            noise_suppression: (mode == Mode::Apm).then(NoiseSuppression::default),
            ..Default::default()
        });
        let rnn = DenoiseState::new();
        Self {
            mode,
            apm,
            rnn,
            scaled: [0.0; APM_FRAME],
            hold: 0,
            gain: 0.0,
        }
    }

    /// Cleans whole 10 ms frames in place; `pcm.len()` must be a multiple of `APM_FRAME`.
    pub fn process(&mut self, pcm: &mut [f32]) -> Result<(), Error> {
        if self.mode == Mode::Off {
            return Ok(());
        }
        for frame in pcm.chunks_exact_mut(APM_FRAME) {
            self.apm.process_capture_frame([&mut *frame])?;
            if self.mode == Mode::Apm {
                continue;
            }
            // RNNoise wants i16-scaled samples.
            frame
                .iter()
                .zip(&mut self.scaled)
                .for_each(|(s, o)| *o = s * 32768.0);
            let vad = self.rnn.process_frame(frame, &self.scaled);
            frame.iter_mut().for_each(|s| *s /= 32768.0);
            if self.mode == Mode::RnnGate {
                self.gate(frame, vad);
            }
        }
        Ok(())
    }

    fn gate(&mut self, frame: &mut [f32], vad: f32) {
        self.hold = if vad > VAD_OPEN {
            HANGOVER
        } else {
            self.hold.saturating_sub(1)
        };
        let target = if self.hold > 0 { 1.0 } else { 0.0 };
        // Ramp across the frame; a hard gate edge is itself a click.
        for (i, s) in frame.iter_mut().enumerate() {
            *s *= self.gain + (target - self.gain) * i as f32 / APM_FRAME as f32;
        }
        self.gain = target;
    }
}
