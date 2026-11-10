//! Clips: animation as data. A track is keyframes for one property of one part; sampling a
//! clip at a time gives the part poses a `Drawing` takes. A key's easing shapes the segment that
//! arrives at it. Before the first key a track holds the first value, after the last it holds the
//! last; a looped clip wraps time, so its last key should match its first.

use std::collections::HashMap;

use runtime::drawing::Pose;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prop {
    X,
    Y,
    Rot,
    Scale,
    ScaleX,
    ScaleY,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    InOut,
    /// Holds the previous key's value until this key's own time.
    Step,
    /// CSS `cubic-bezier(x1, y1, x2, y2)`; build it with [`Easing::bezier`].
    Bezier { x1: f64, y1: f64, x2: f64, y2: f64 },
}

/// Bisection halves `0..=1` this many times: past 53 the interval no longer shrinks.
const BEZIER_STEPS: usize = 60;

impl Easing {
    /// `x1`, `x2` within 0..=1 keep the curve's time monotonic, so `u` has one solution.
    pub fn bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, ClipError> {
        let curve = Easing::Bezier { x1, y1, x2, y2 };
        if curve.valid() { Ok(curve) } else { Err(ClipError::Bezier) }
    }

    fn valid(self) -> bool {
        match self {
            Easing::Bezier { x1, y1, x2, y2 } => {
                (0.0..=1.0).contains(&x1)
                    && (0.0..=1.0).contains(&x2)
                    && y1.is_finite()
                    && y2.is_finite()
            }
            _ => true,
        }
    }

    // Rollback worlds digest what this feeds, so the bezier solve is only `+ − × ÷` and
    // comparisons over a fixed count: no libm, no `mul_add`, the same bits on every machine.
    fn apply(self, u: f64) -> f64 {
        match self {
            Easing::Linear => u,
            Easing::InOut => u * u * (3.0 - 2.0 * u),
            Easing::Step => 0.0,
            Easing::Bezier { .. } if u <= 0.0 => 0.0,
            Easing::Bezier { x1, y1, x2, y2 } => {
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..BEZIER_STEPS {
                    let mid = (lo + hi) * 0.5;
                    if cubic(x1, x2, mid) < u {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                cubic(y1, y2, (lo + hi) * 0.5)
            }
        }
    }
}

/// One coordinate of the bezier from 0 through `p1`, `p2` to 1, at `s`.
fn cubic(p1: f64, p2: f64, s: f64) -> f64 {
    let c = 3.0 * p1;
    let b = 3.0 * (p2 - p1) - c;
    let a = 1.0 - c - b;
    ((a * s + b) * s + c) * s
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub time: f64,
    pub value: f64,
    pub easing: Easing,
}

#[derive(Debug)]
pub struct Track {
    pub part: String,
    pub prop: Prop,
    pub keys: Vec<Key>,
}

/// A part's visibility from `time` on. Booleans don't interpolate, so `show` is a track kind of
/// its own, not an `f64` in `Track`: it always steps, and before its first key holds that key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShowKey {
    pub time: f64,
    pub show: bool,
}

#[derive(Debug)]
pub struct ShowTrack {
    pub part: String,
    pub keys: Vec<ShowKey>,
}

#[derive(Debug)]
pub struct Clip {
    length: f64,
    looped: bool,
    tracks: Vec<Track>,
    shows: Vec<ShowTrack>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ClipError {
    #[error("clip length must be a positive number")]
    Length,
    #[error("track {part}.{prop:?} has no keys")]
    NoKeys { part: String, prop: Prop },
    #[error("track {part}.{prop:?} has a key outside 0..=length or out of order")]
    KeyTime { part: String, prop: Prop },
    #[error("track {part}.{prop:?} is described twice")]
    DuplicateTrack { part: String, prop: Prop },
    #[error("track {part}.{prop:?} has a value that is not a finite number")]
    Value { part: String, prop: Prop },
    #[error("track {part}.{prop:?} has a bezier easing with x1 or x2 outside 0..=1")]
    KeyEasing { part: String, prop: Prop },
    #[error("show track {part} has no keys")]
    NoShowKeys { part: String },
    #[error("show track {part} has a key outside 0..=length or out of order")]
    ShowKeyTime { part: String },
    #[error("show track {part} is described twice")]
    DuplicateShow { part: String },
    #[error("a bezier easing needs x1 and x2 within 0..=1 and finite y1, y2")]
    Bezier,
}

impl Clip {
    pub fn new(length: f64, looped: bool, tracks: Vec<Track>) -> Result<Self, ClipError> {
        if !(length.is_finite() && length > 0.0) {
            return Err(ClipError::Length);
        }
        for (i, t) in tracks.iter().enumerate() {
            let (part, prop) = (t.part.clone(), t.prop);
            if tracks[..i]
                .iter()
                .any(|o| o.part == t.part && o.prop == t.prop)
            {
                return Err(ClipError::DuplicateTrack { part, prop });
            }
            if t.keys.is_empty() {
                return Err(ClipError::NoKeys { part, prop });
            }
            let in_range = |k: &Key| k.time >= 0.0 && k.time <= length; // false for NaN
            let ordered = t.keys.windows(2).all(|w| w[0].time < w[1].time);
            if !(t.keys.iter().all(in_range) && ordered) {
                return Err(ClipError::KeyTime { part, prop });
            }
            if !t.keys.iter().all(|k| k.value.is_finite()) {
                return Err(ClipError::Value { part, prop });
            }
            if !t.keys.iter().all(|k| k.easing.valid()) {
                return Err(ClipError::KeyEasing { part, prop });
            }
        }
        Ok(Self {
            length,
            looped,
            tracks,
            shows: Vec::new(),
        })
    }

    /// The clip with its `show` tracks, one per part.
    pub fn with_shows(mut self, shows: Vec<ShowTrack>) -> Result<Self, ClipError> {
        for (i, t) in shows.iter().enumerate() {
            let part = t.part.clone();
            if shows[..i].iter().any(|o| o.part == t.part) {
                return Err(ClipError::DuplicateShow { part });
            }
            if t.keys.is_empty() {
                return Err(ClipError::NoShowKeys { part });
            }
            let in_range = |k: &ShowKey| k.time >= 0.0 && k.time <= self.length; // false for NaN
            let ordered = t.keys.windows(2).all(|w| w[0].time < w[1].time);
            if !(t.keys.iter().all(in_range) && ordered) {
                return Err(ClipError::ShowKeyTime { part });
            }
        }
        self.shows = shows;
        Ok(self)
    }

    /// The part names this clip moves — checked against a drawing when the two meet.
    /// Whether a once clip has reached its end `time` seconds in; a looped clip never does.
    pub fn length(&self) -> f64 {
        self.length
    }

    pub fn looped(&self) -> bool {
        self.looped
    }

    pub fn done(&self, time: f64) -> bool {
        !self.looped && time >= self.length
    }

    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.tracks
            .iter()
            .map(|t| t.part.as_str())
            .chain(self.shows.iter().map(|t| t.part.as_str()))
    }

    pub fn sample(&self, time: f64) -> HashMap<&str, Pose> {
        let t = if self.looped {
            time.rem_euclid(self.length)
        } else {
            time.clamp(0.0, self.length)
        };
        let mut poses: HashMap<&str, Pose> = HashMap::new();
        for track in &self.tracks {
            let v = value_at(&track.keys, t);
            let pose = poses.entry(track.part.as_str()).or_default();
            match track.prop {
                Prop::X => pose.x = v,
                Prop::Y => pose.y = v,
                Prop::Rot => pose.rot = v,
                Prop::Scale => pose.scale = v,
                Prop::ScaleX => pose.scale_x = v,
                Prop::ScaleY => pose.scale_y = v,
            }
        }
        for track in &self.shows {
            let next = track.keys.partition_point(|k| k.time <= t);
            let key = track.keys[next.saturating_sub(1)]; // before the first key: the first
            poses.entry(track.part.as_str()).or_default().show = key.show;
        }
        poses
    }
}

fn value_at(keys: &[Key], t: f64) -> f64 {
    let next = keys.partition_point(|k| k.time <= t);
    match next {
        0 => keys[0].value,
        n if n == keys.len() => keys[n - 1].value,
        n => {
            let (a, b) = (keys[n - 1], keys[n]);
            if b.easing == Easing::Step {
                return a.value;
            }
            let u = b.easing.apply((t - a.time) / (b.time - a.time));
            a.value + (b.value - a.value) * u
        }
    }
}

#[cfg(test)]
mod tests;
