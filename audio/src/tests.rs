use std::sync::Arc;

use super::*;

/// Rising zero crossings per second: the frequency of a periodic signal.
fn rate_of(samples: &[f32]) -> f32 {
    let ups = samples.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    ups as f32 * RATE as f32 / samples.len() as f32
}

fn square(freq: f32) -> Synth {
    Synth { wave: Wave::Square, freq, attack: 0.0, sustain: 1.0, decay: 0.0, volume: 1.0, ..Synth::default() }
}

fn sound(name: &str, s: Synth) -> Sound {
    Sound::synth(name, s).unwrap()
}

fn play(m: &mut Mixer, voice: u64, s: &Sound, volume: f32, pitch: f32) {
    m.apply(Command::Play { bus: 1, voice, sound: s.clone(), volume, pitch });
}

fn run(m: &mut Mixer, seconds: f32) -> Vec<f32> {
    let mut out = vec![0.0; (seconds * RATE as f32) as usize];
    m.render(&mut out);
    out
}

// U-1
#[test]
fn a_spec_renders_the_same_samples_every_time() {
    for wave in [Wave::Square, Wave::Saw, Wave::Sine, Wave::Noise] {
        let s = Synth { wave, slide: -300.0, ..Synth::default() };
        assert_eq!(s.render(), s.render(), "{wave:?}");
        assert!(s.render().iter().any(|v| v.abs() > 0.1), "{wave:?} is silent");
    }
}

#[test]
fn a_square_at_440_crosses_440_times_a_second() {
    let r = rate_of(&square(440.0).render());
    assert!((r - 440.0).abs() <= 1.0, "{r}");
}

#[test]
fn the_envelope_puts_the_peak_and_the_end_where_the_spec_says() {
    let s = Synth { wave: Wave::Sine, freq: 1000.0, attack: 0.1, sustain: 0.2, decay: 0.3, volume: 0.5, ..Synth::default() };
    let out = s.render();
    assert_eq!(out.len(), (0.6 * RATE as f32).round() as usize);
    let peak = |from: f32, to: f32| {
        let (a, b) = ((from * RATE as f32) as usize, (to * RATE as f32) as usize);
        out[a..b].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };
    assert!(peak(0.0, 0.01) < 0.06, "attack starts quiet");
    assert!((peak(0.1, 0.3) - 0.5).abs() < 0.01, "sustain is at volume");
    assert!(peak(0.58, 0.6) < 0.06, "decay ends quiet");
}

#[test]
fn slide_moves_the_pitch() {
    let s = Synth { slide: 400.0, ..square(200.0) };
    let out = s.render(); // one second: 200 Hz up to 600 Hz
    let first = rate_of(&out[..4800]);
    let last = rate_of(&out[out.len() - 4800..]);
    assert!((first - 220.0).abs() < 20.0 && (last - 580.0).abs() < 20.0, "{first} {last}");
}

#[test]
fn bad_specs_name_the_field() {
    let cases = [
        (Synth { freq: 0.0, ..Synth::default() }, "freq"),
        (Synth { freq: 30_000.0, ..Synth::default() }, "freq"),
        (Synth { attack: -1.0, ..Synth::default() }, "attack"),
        (Synth { decay: 99.0, ..Synth::default() }, "decay"),
        (Synth { volume: 2.0, ..Synth::default() }, "volume"),
        (Synth { duty: 1.0, ..Synth::default() }, "duty"),
        (Synth { attack: 0.0, sustain: 0.0, decay: 0.0, ..Synth::default() }, "length"),
    ];
    for (s, field) in cases {
        let e = s.check().unwrap_err().to_string();
        assert!(e.contains(field), "{field}: {e}");
    }
    let looping = Synth { attack: 0.0, sustain: 0.0, decay: 0.0, looping: true, ..Synth::default() };
    assert!(looping.check().is_ok(), "a loop needs no length");
}

// U-2
#[test]
fn the_mixer_sums_voices_at_their_volume() {
    let a = sound("a", Synth { volume: 0.3, ..square(300.0) });
    let b = sound("b", Synth { wave: Wave::Saw, volume: 0.3, ..square(500.0) });
    let solo = |s: &Sound, vol: f32| {
        let mut m = Mixer::new(1.0);
        play(&mut m, 1, s, vol, 1.0);
        run(&mut m, 0.5)
    };
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &a, 1.0, 1.0);
    play(&mut m, 2, &b, 0.5, 1.0);
    let both = run(&mut m, 0.5);
    let (sa, sb) = (solo(&a, 1.0), solo(&b, 0.5));
    for i in 0..both.len() {
        assert!((both[i] - (sa[i] + sb[i])).abs() < 1e-5, "sample {i}");
    }
    assert_eq!(m.capture(1), both, "the bus capture is what it played");
}

#[test]
fn pitch_two_doubles_the_frequency() {
    let s = sound("s", square(300.0));
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &s, 1.0, 2.0);
    let r = rate_of(&run(&mut m, 0.5));
    assert!((r - 600.0).abs() <= 2.0, "{r}");
}

#[test]
fn set_changes_pitch_and_volume_smoothly() {
    let s = sound("s", Synth { looping: true, ..square(300.0) });
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &s, 0.5, 1.0);
    run(&mut m, 0.2);
    m.apply(Command::Set { bus: 1, voice: 1, volume: 0.25, pitch: 1.5 });
    let out = run(&mut m, 0.5);
    let r = rate_of(&out[4800..]);
    assert!((r - 450.0).abs() <= 2.0, "{r}");
    let tail = out[out.len() - 4800..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!((tail - 0.25).abs() < 0.01, "{tail}");
}

#[test]
fn the_limiter_keeps_the_output_inside_one() {
    let s = sound("loud", square(200.0));
    let mut m = Mixer::new(1.0);
    for v in 1..=6 {
        play(&mut m, v, &s, 1.0, 1.0 + v as f32 * 0.01);
    }
    let out = run(&mut m, 0.5);
    assert!(out.iter().all(|v| v.abs() <= 1.0), "{}", out.iter().fold(0.0f32, |m, v| m.max(v.abs())));
    assert!(out.iter().any(|v| v.abs() > 0.9), "still loud");
}

#[test]
fn a_stopped_voice_fades_instead_of_clicking() {
    let s = sound("s", Synth { wave: Wave::Sine, looping: true, ..square(100.0) });
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &s, 1.0, 1.0);
    let before = run(&mut m, 0.1);
    m.apply(Command::Stop { bus: 1, voice: 1 });
    let after = run(&mut m, 0.05);
    let step = |w: &[f32]| (w[1] - w[0]).abs();
    let normal = before.windows(2).map(step).fold(0.0f32, f32::max);
    let join = [*before.last().unwrap(), after[0]];
    assert!(step(&join) <= normal * 1.5, "no jump at the stop");
    let fade = (FADE * RATE as f32) as usize;
    assert!(after[fade..].iter().all(|v| *v == 0.0), "silent after the fade");
    assert!(after[..fade / 2].iter().any(|v| v.abs() > 0.1), "but not cut off");
}

#[test]
fn the_log_says_what_started_and_stopped() {
    let blip = sound("blip", Synth { attack: 0.0, sustain: 0.05, decay: 0.0, ..Synth::default() });
    let hum = sound("hum", Synth { looping: true, ..Synth::default() });
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &blip, 0.8, 1.0);
    play(&mut m, 2, &hum, 0.5, 1.2);
    run(&mut m, 0.1);
    m.apply(Command::Stop { bus: 1, voice: 2 });
    run(&mut m, 0.1);
    let name = |n: &str| Arc::<str>::from(n);
    assert_eq!(
        m.log(1),
        vec![
            Event::Start { voice: 1, name: name("blip"), at: 0, volume: 0.8, pitch: 1.0 },
            Event::Start { voice: 2, name: name("hum"), at: 0, volume: 0.5, pitch: 1.2 },
            Event::Stop { voice: 1, name: name("blip"), at: 2400, why: Why::Ended },
            Event::Stop { voice: 2, name: name("hum"), at: 4800, why: Why::Stopped },
        ]
    );
}

#[test]
fn closing_a_bus_stops_its_voices_and_keeps_its_log() {
    let hum = sound("hum", Synth { looping: true, ..Synth::default() });
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &hum, 1.0, 1.0);
    m.apply(Command::Play { bus: 2, voice: 1, sound: hum.clone(), volume: 1.0, pitch: 1.0 });
    run(&mut m, 0.05);
    m.apply(Command::Close { bus: 1 });
    run(&mut m, 0.1);
    assert!(matches!(m.log(1).last(), Some(Event::Stop { why: Why::Closed, .. })));
    assert_eq!(m.log(2).len(), 1, "the other app plays on");
    let tail = m.capture(1);
    assert!(tail[tail.len() - 100..].iter().all(|v| *v == 0.0));
}

#[test]
fn replaying_a_voice_id_replaces_it() {
    let hum = sound("hum", Synth { looping: true, ..Synth::default() });
    let mut m = Mixer::new(1.0);
    play(&mut m, 1, &hum, 1.0, 1.0);
    play(&mut m, 1, &hum, 1.0, 1.0);
    run(&mut m, 0.05);
    assert_eq!(m.voices(1), 1);
}
