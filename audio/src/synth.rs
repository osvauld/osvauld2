use crate::{Error, RATE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Square,
    Saw,
    Sine,
    /// A new random level each period, so noise has a pitch too (sfxr's noise).
    Noise,
}

/// A generated sound: one oscillator under an attack/sustain/decay envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Synth {
    pub wave: Wave,
    /// Hz at the start.
    pub freq: f32,
    /// Hz per second; negative falls. The pitch never goes below 1 Hz.
    pub slide: f32,
    /// Seconds.
    pub attack: f32,
    pub sustain: f32,
    pub decay: f32,
    pub volume: f32,
    /// Square only: the share of each period spent high.
    pub duty: f32,
    /// Holds at full after the attack until the voice is stopped; `sustain` and `decay` unused.
    pub looping: bool,
}

impl Default for Synth {
    fn default() -> Self {
        Synth {
            wave: Wave::Square,
            freq: 440.0,
            slide: 0.0,
            attack: 0.0,
            sustain: 0.1,
            decay: 0.1,
            volume: 0.5,
            duty: 0.5,
            looping: false,
        }
    }
}

const MAX_SECONDS: f32 = 10.0;

impl Synth {
    pub fn check(&self) -> Result<(), Error> {
        let bad = |field: &str, why: String| Err(Error::Spec(format!("{field} {why}")));
        if !(self.freq > 0.0 && self.freq <= 20_000.0) {
            return bad("freq", format!("must be above 0 and at most 20000 Hz, got {}", self.freq));
        }
        if !self.slide.is_finite() {
            return bad("slide", "must be a number".into());
        }
        for (field, v) in [("attack", self.attack), ("sustain", self.sustain), ("decay", self.decay)] {
            if !(0.0..=MAX_SECONDS).contains(&v) {
                return bad(field, format!("must be 0 to {MAX_SECONDS} seconds, got {v}"));
            }
        }
        if !(0.0..=1.0).contains(&self.volume) {
            return bad("volume", format!("must be 0 to 1, got {}", self.volume));
        }
        if !(self.duty > 0.0 && self.duty < 1.0) {
            return bad("duty", format!("must be between 0 and 1, got {}", self.duty));
        }
        if !self.looping && self.samples() == Some(0) {
            return bad("length", "is zero: give attack, sustain or decay some seconds".into());
        }
        Ok(())
    }

    /// Length in samples; `None` for a loop.
    pub fn samples(&self) -> Option<usize> {
        (!self.looping).then(|| ((self.attack + self.sustain + self.decay) * RATE as f32).round() as usize)
    }

    /// The whole sound at pitch 1. A loop renders its attack and one second held.
    pub fn render(&self) -> Vec<f32> {
        let n = self.samples().unwrap_or(((self.attack + 1.0) * RATE as f32) as usize);
        let mut osc = Osc::default();
        (0..n).map_while(|_| osc.next(self, 1.0)).collect()
    }
}

/// One playing copy of a synth.
pub(crate) struct Osc {
    phase: f64,
    t: usize,
    noise: u32,
    level: f32,
}

impl Default for Osc {
    fn default() -> Self {
        // A fixed seed: the same spec makes the same samples every time it plays.
        Osc { phase: 0.0, t: 0, noise: 0x9E37_79B9, level: 0.0 }
    }
}

impl Osc {
    /// The next sample, or `None` once the envelope has ended.
    pub(crate) fn next(&mut self, s: &Synth, pitch: f32) -> Option<f32> {
        let env = self.envelope(s)?;
        let secs = self.t as f32 / RATE as f32;
        let freq = (s.freq + s.slide * secs).max(1.0) * pitch;
        let p = self.phase as f32;
        let v = match s.wave {
            Wave::Square => if p < s.duty { 1.0 } else { -1.0 },
            Wave::Saw => 2.0 * p - 1.0,
            Wave::Sine => (p * std::f32::consts::TAU).sin(),
            Wave::Noise => self.level,
        };
        self.phase += freq as f64 / RATE as f64;
        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            // xorshift32: cheap, and deterministic from the seed.
            self.noise ^= self.noise << 13;
            self.noise ^= self.noise >> 17;
            self.noise ^= self.noise << 5;
            self.level = self.noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
        }
        self.t += 1;
        Some(v * env * s.volume)
    }

    fn envelope(&self, s: &Synth) -> Option<f32> {
        let t = self.t as f32 / RATE as f32;
        if t < s.attack {
            return Some(t / s.attack);
        }
        if s.looping {
            return Some(1.0);
        }
        let held = s.attack + s.sustain;
        if self.t >= s.samples()? {
            None
        } else if t < held {
            Some(1.0)
        } else {
            Some((1.0 - (t - held) / s.decay).max(0.0))
        }
    }
}
