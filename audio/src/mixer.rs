use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::synth::{Osc, Synth};
use crate::{Error, RATE};

pub type Bus = u64;
pub type VoiceId = u64;

/// Seconds a stopped voice takes to fall silent. Cutting it dead clicks.
pub const FADE: f32 = 0.01;
/// Seconds a volume or pitch change takes to settle, so a per-frame `Set` doesn't zipper.
const GLIDE: f32 = 0.005;
const LOG_LEN: usize = 256;

/// Something to play, named for the log.
#[derive(Clone, Debug)]
pub struct Sound {
    name: Arc<str>,
    synth: Arc<Synth>,
}

impl Sound {
    pub fn synth(name: &str, synth: Synth) -> Result<Sound, Error> {
        synth.check()?;
        Ok(Sound { name: name.into(), synth: Arc::new(synth) })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn looping(&self) -> bool {
        self.synth.looping
    }
}

pub enum Command {
    /// Starts `voice`, replacing a voice already playing under that id.
    Play { bus: Bus, voice: VoiceId, sound: Sound, volume: f32, pitch: f32 },
    Set { bus: Bus, voice: VoiceId, volume: f32, pitch: f32 },
    Stop { bus: Bus, voice: VoiceId },
    /// The app went away: everything on the bus fades out. Its log stays readable.
    Close { bus: Bus },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Ended,
    Stopped,
    Replaced,
    Closed,
}

/// `at` counts samples since the mixer began.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Start { voice: VoiceId, name: Arc<str>, at: u64, volume: f32, pitch: f32 },
    Stop { voice: VoiceId, name: Arc<str>, at: u64, why: Why },
}

struct Voice {
    id: VoiceId,
    sound: Sound,
    osc: Osc,
    volume: f32,
    pitch: f32,
    want_volume: f32,
    want_pitch: f32,
    /// Samples left in the fade-out once stopped.
    fading: Option<u32>,
}

#[derive(Default)]
struct BusState {
    voices: Vec<Voice>,
    log: VecDeque<Event>,
    capture: VecDeque<f32>,
}

pub struct Mixer {
    buses: BTreeMap<Bus, BusState>,
    now: u64,
    capture_len: usize,
    /// The limiter's envelope: the recent peak, falling back slowly.
    peak: f32,
}

impl Mixer {
    /// `capture_seconds` of each bus's recent output are kept for `capture`.
    pub fn new(capture_seconds: f32) -> Mixer {
        Mixer { buses: BTreeMap::new(), now: 0, capture_len: (capture_seconds * RATE as f32) as usize, peak: 1.0 }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn apply(&mut self, c: Command) {
        let now = self.now;
        match c {
            Command::Play { bus, voice, sound, volume, pitch } => {
                let b = self.buses.entry(bus).or_default();
                stop(b, voice, now, Why::Replaced);
                log(b, Event::Start { voice, name: sound.name.clone(), at: now, volume, pitch });
                b.voices.push(Voice {
                    id: voice,
                    sound,
                    osc: Osc::default(),
                    volume,
                    pitch,
                    want_volume: volume,
                    want_pitch: pitch,
                    fading: None,
                });
            }
            Command::Set { bus, voice, volume, pitch } => {
                let Some(b) = self.buses.get_mut(&bus) else { return };
                for v in b.voices.iter_mut().filter(|v| v.id == voice && v.fading.is_none()) {
                    v.want_volume = volume;
                    v.want_pitch = pitch;
                }
            }
            Command::Stop { bus, voice } => {
                if let Some(b) = self.buses.get_mut(&bus) {
                    stop(b, voice, now, Why::Stopped);
                }
            }
            Command::Close { bus } => {
                if let Some(b) = self.buses.get_mut(&bus) {
                    let ids: Vec<VoiceId> = b.voices.iter().filter(|v| v.fading.is_none()).map(|v| v.id).collect();
                    for id in ids {
                        stop(b, id, now, Why::Closed);
                    }
                }
            }
        }
    }

    /// Fills `out` with the next samples of the whole mix, limited to ±1.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let glide = 1.0 - (-1.0 / (GLIDE * RATE as f32)).exp();
        let fade_len = (FADE * RATE as f32) as u32;
        for b in self.buses.values_mut() {
            let mut ended = Vec::new();
            for (i, slot) in out.iter_mut().enumerate() {
                let mut sum = 0.0;
                for v in b.voices.iter_mut() {
                    if v.osc_done() {
                        continue;
                    }
                    v.volume += (v.want_volume - v.volume) * glide;
                    v.pitch += (v.want_pitch - v.pitch) * glide;
                    let Some(s) = v.osc.next(&v.sound.synth, v.pitch) else {
                        v.fading = Some(0);
                        ended.push((v.id, v.sound.name.clone(), self.now + i as u64));
                        continue;
                    };
                    let fade = match &mut v.fading {
                        Some(left) => {
                            *left = left.saturating_sub(1);
                            *left as f32 / fade_len as f32
                        }
                        None => 1.0,
                    };
                    sum += s * v.volume * fade;
                }
                *slot += sum;
                b.capture.push_back(sum);
            }
            for (voice, name, at) in ended {
                log(b, Event::Stop { voice, name, at, why: Why::Ended });
            }
            b.voices.retain(|v| !v.osc_done());
            let over = b.capture.len().saturating_sub(self.capture_len);
            b.capture.drain(..over);
        }
        // Peak limiter: instant attack, ~100 ms release. Nothing leaves past ±1.
        let release = (-1.0 / (0.1 * RATE as f32)).exp();
        for s in out.iter_mut() {
            self.peak = (self.peak * release).max(s.abs()).max(1.0);
            *s /= self.peak;
        }
        self.now += out.len() as u64;
    }

    pub fn log(&self, bus: Bus) -> Vec<Event> {
        self.buses.get(&bus).map(|b| b.log.iter().cloned().collect()).unwrap_or_default()
    }

    /// The bus's recent output, before the limiter, oldest first.
    pub fn capture(&self, bus: Bus) -> Vec<f32> {
        self.buses.get(&bus).map(|b| b.capture.iter().copied().collect()).unwrap_or_default()
    }

    /// Voices playing on the bus, not counting ones fading out.
    pub fn voices(&self, bus: Bus) -> usize {
        self.buses.get(&bus).map_or(0, |b| b.voices.iter().filter(|v| v.fading.is_none()).count())
    }
}

impl Voice {
    fn osc_done(&self) -> bool {
        self.fading == Some(0)
    }
}

fn stop(b: &mut BusState, voice: VoiceId, at: u64, why: Why) {
    let fade = (FADE * RATE as f32) as u32;
    let mut names = Vec::new();
    for v in b.voices.iter_mut().filter(|v| v.id == voice && v.fading.is_none()) {
        v.fading = Some(fade);
        names.push(v.sound.name.clone());
    }
    for name in names {
        log(b, Event::Stop { voice, name, at, why });
    }
}

fn log(b: &mut BusState, e: Event) {
    if b.log.len() == LOG_LEN {
        b.log.pop_front();
    }
    b.log.push_back(e);
}
