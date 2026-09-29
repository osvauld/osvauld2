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
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    InOut,
}

impl Easing {
    fn apply(self, u: f64) -> f64 {
        match self {
            Easing::Linear => u,
            Easing::InOut => u * u * (3.0 - 2.0 * u),
        }
    }
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

#[derive(Debug)]
pub struct Clip {
    length: f64,
    looped: bool,
    tracks: Vec<Track>,
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
        }
        Ok(Self {
            length,
            looped,
            tracks,
        })
    }

    /// The part names this clip moves — checked against a drawing when the two meet.
    /// Whether a once clip has reached its end `time` seconds in; a looped clip never does.
    pub fn done(&self, time: f64) -> bool {
        !self.looped && time >= self.length
    }

    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.tracks.iter().map(|t| t.part.as_str())
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
            }
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
            let u = b.easing.apply((t - a.time) / (b.time - a.time));
            a.value + (b.value - a.value) * u
        }
    }
}

#[cfg(test)]
mod tests;
