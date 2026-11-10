//! The shell's one mixer (docs/design/sound.md §1.2, §1.6). Apps send it commands over a channel,
//! one bus each. Who renders it depends on the mode, and nothing else does:
//! - offscreen, the virtual frame clock: `advance_to` drains the commands and renders exactly up to
//!   the frame's time, so a capture after N frames is the same every run;
//! - windowed, the sound card's callback, or — with no usable device (or on Android, not wired
//!   yet) — a thread that renders on the wall clock into nothing, so the log still moves.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use audio::{Bus, Command, Event, Mixer, RATE, Why};

/// Seconds of each app's recent mix kept for `SoundCapture`.
const CAPTURE_SECONDS: f32 = 10.0;

pub struct SoundHost {
    mixer: Arc<Mutex<Mixer>>,
    tx: Sender<Command>,
    /// Offscreen only: the commands, drained by `advance_to`. Windowed, the renderer owns them.
    rx: Option<Mutex<Receiver<Command>>>,
}

impl SoundHost {
    pub fn new(offscreen: bool) -> SoundHost {
        let mixer = Arc::new(Mutex::new(Mixer::new(CAPTURE_SECONDS)));
        let (tx, rx) = channel();
        let rx = if offscreen {
            Some(Mutex::new(rx))
        } else {
            let m = mixer.clone();
            if let Err(e) = std::thread::Builder::new().name("sound".into()).spawn(move || output(m, rx)) {
                eprintln!("sound: no output thread ({e}); apps play silently");
            }
            None
        };
        SoundHost { mixer, tx, rx }
    }

    pub fn sender(&self) -> Sender<Command> {
        self.tx.clone()
    }

    /// Offscreen: apply what the apps asked for, then render up to `secs` on the virtual clock.
    pub fn advance_to(&self, secs: f64) {
        let Some(rx) = &self.rx else { return };
        let mut m = self.mixer.lock().unwrap();
        for c in rx.lock().unwrap().try_iter() {
            m.apply(c);
        }
        let target = (secs * RATE as f64).round() as u64;
        let mut buf = vec![0.0; 4800];
        while m.now() < target {
            let n = ((target - m.now()) as usize).min(buf.len());
            m.render(&mut buf[..n]);
        }
    }

    pub fn log_json(&self, bus: Bus) -> serde_json::Value {
        let secs = |at: u64| at as f64 / RATE as f64;
        let events: Vec<_> = self
            .mixer
            .lock()
            .unwrap()
            .log(bus)
            .into_iter()
            .map(|e| match e {
                Event::Start { voice, name, at, volume, pitch } => serde_json::json!({
                    "event": "start", "voice": voice, "name": &*name, "at": secs(at),
                    "volume": volume, "pitch": pitch,
                }),
                Event::Stop { voice, name, at, why } => serde_json::json!({
                    "event": "stop", "voice": voice, "name": &*name, "at": secs(at),
                    "why": match why {
                        Why::Ended => "ended",
                        Why::Stopped => "stopped",
                        Why::Replaced => "replaced",
                        Why::Closed => "closed",
                    },
                }),
            })
            .collect();
        serde_json::json!(events)
    }

    /// The last `seconds` of the bus's mix as a mono 16-bit 48 kHz WAV, and the mixer's clock.
    pub fn capture_wav(&self, bus: Bus, seconds: f32) -> (Vec<u8>, f64) {
        let m = self.mixer.lock().unwrap();
        let all = m.capture(bus);
        // Before an app's first sound its bus holds nothing yet: that time was silence.
        let n = ((seconds.max(0.0) * RATE as f32) as usize).min(m.now() as usize);
        let kept = n.min(all.len());
        let mut out = vec![0.0; n - kept];
        out.extend_from_slice(&all[all.len() - kept..]);
        (wav(&out), m.now() as f64 / RATE as f64)
    }
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// The windowed renderer: the sound card if there is one, else the wall clock.
fn output(mixer: Arc<Mutex<Mixer>>, rx: Receiver<Command>) {
    #[cfg(not(target_os = "android"))]
    let rx = match device::play(mixer.clone(), rx) {
        Ok(stream) => {
            // The stream plays as long as it lives; this thread only keeps it.
            let _stream = stream;
            loop {
                std::thread::park();
            }
        }
        Err((e, rx)) => {
            eprintln!("sound: no output device ({e}); apps play silently");
            rx
        }
    };
    let start = std::time::Instant::now();
    let mut buf = vec![0.0; 4800];
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut m = mixer.lock().unwrap();
        for c in rx.try_iter() {
            m.apply(c);
        }
        let target = (start.elapsed().as_secs_f64() * RATE as f64) as u64;
        while m.now() < target {
            let n = ((target - m.now()) as usize).min(buf.len());
            m.render(&mut buf[..n]);
        }
    }
}

#[cfg(not(target_os = "android"))]
mod device {
    use std::sync::mpsc::Receiver;
    use std::sync::{Arc, Mutex};

    use audio::{Command, Mixer, RATE};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    /// Mono if the device takes it, else stereo with both sides the same.
    pub(super) fn play(
        mixer: Arc<Mutex<Mixer>>,
        rx: Receiver<Command>,
    ) -> Result<cpal::Stream, (String, Receiver<Command>)> {
        let Some(device) = cpal::default_host().default_output_device() else {
            return Err(("no default output".into(), rx));
        };
        let rx = Arc::new(Mutex::new(rx));
        let mut last = String::new();
        for channels in [1u16, 2] {
            let config = cpal::StreamConfig {
                channels,
                sample_rate: RATE,
                buffer_size: cpal::BufferSize::Default,
            };
            let (m, r) = (mixer.clone(), rx.clone());
            let mut mono = Vec::new();
            let stream = device.build_output_stream(
                config,
                move |out: &mut [f32], _: &_| {
                    let mut m = m.lock().unwrap();
                    for c in r.lock().unwrap().try_iter() {
                        m.apply(c);
                    }
                    mono.resize(out.len() / channels as usize, 0.0);
                    m.render(&mut mono);
                    for (frame, s) in out.chunks_mut(channels as usize).zip(&mono) {
                        frame.fill(*s);
                    }
                },
                |e| eprintln!("sound: {e}"),
                None,
            );
            match stream.map_err(|e| e.to_string()).and_then(|s| s.play().map(|_| s).map_err(|e| e.to_string())) {
                Ok(s) => return Ok(s),
                Err(e) => last = e,
            }
        }
        // Both tries failed, so nothing else holds the receiver now.
        let rx = Arc::try_unwrap(rx).ok().map(|m| m.into_inner().unwrap());
        match rx {
            Some(rx) => Err((last, rx)),
            None => Err((format!("{last} (and the commands were lost)"), std::sync::mpsc::channel().1)),
        }
    }
}
