//! Voice POC: mic → Opus → iroh datagram → Opus → speaker, one binary per peer.
//! `voice listen` prints the `voice dial …` line for the other peer.
//! Datagrams are `[seq: u32 BE][level: u8][opus frame]`, 20 ms mono at 48 kHz; datagrams,
//! not a stream, because a retransmitted frame arrives too late to play.
//! Outgoing audio is cleaned per `--clean` (see `clean`); `voice process` runs the same
//! cleaning plus an Opus round-trip on a WAV file, so modes can be scored offline.

mod clean;
mod net;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use clean::{Cleaner, Mode};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use opus::{Application, Channels, Decoder, Encoder};
use webrtc_audio_processing::Processor;

const RATE: u32 = 48_000;
const FRAME: usize = 960;
// WebRTC and RNNoise both work in 10 ms frames.
const APM_FRAME: usize = 480;
const HEADER: usize = 5;
// Past this the speaker is lagging the wire; drop the oldest audio, latency beats completeness.
const MAX_BUFFERED: usize = FRAME * 5;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "usage: voice listen | voice dial <ticket> | voice process <in.wav> <out.wav>\n\
         \x20      all take --clean none|apm|rnn|rnn-gate (default apm)"
    )]
    Usage,
    #[error("{0}: need 48 kHz mono WAV")]
    WavFormat(String),
    #[error(transparent)]
    Wav(#[from] hound::Error),
    #[error("endpoint closed before a call arrived")]
    Closed,
    #[error("no default {0} device")]
    NoDevice(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Opus(#[from] opus::Error),
    #[error(transparent)]
    Audio(#[from] cpal::Error),
    #[error(transparent)]
    Bind(#[from] iroh::endpoint::BindError),
    #[error(transparent)]
    Connect(#[from] iroh::endpoint::ConnectError),
    #[error(transparent)]
    Connecting(#[from] iroh::endpoint::ConnectingError),
    #[error(transparent)]
    Connection(#[from] iroh::endpoint::ConnectionError),
    #[error(transparent)]
    Datagram(#[from] iroh::endpoint::SendDatagramError),
    #[error("audio processing: {0:?}")]
    Apm(webrtc_audio_processing::Error),
}

impl From<webrtc_audio_processing::Error> for Error {
    fn from(e: webrtc_audio_processing::Error) -> Self {
        Error::Apm(e)
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Error> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.iter().position(|a| a == "--clean") {
        Some(i) => {
            let mode = args.get(i + 1).ok_or(Error::Usage)?.parse()?;
            args.drain(i..i + 2);
            mode
        }
        None => Mode::Apm,
    };
    if let [cmd, input, output] = &args[..]
        && cmd == "process"
    {
        return process_file(input, output, mode);
    }
    let (endpoint, conn) = match &args[..] {
        [cmd] if cmd == "listen" => net::listen().await?,
        [cmd, ticket] if cmd == "dial" => {
            net::dial(ticket.parse().map_err(|_| Error::Usage)?).await?
        }
        _ => return Err(Error::Usage),
    };

    let apm = Arc::new(Processor::new(RATE)?);
    let mut cleaner = Cleaner::new(mode, apm.clone());

    let jitter = Arc::new(Mutex::new(VecDeque::<f32>::new()));
    let (mic_tx, mic_rx) = mpsc::channel::<Vec<f32>>();

    let host = cpal::default_host();
    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: RATE,
        buffer_size: cpal::BufferSize::Default,
    };
    let input = host
        .default_input_device()
        .ok_or(Error::NoDevice("input"))?;
    let output = host
        .default_output_device()
        .ok_or(Error::NoDevice("output"))?;

    let mic = input.build_input_stream(
        config,
        move |data: &[f32], _: &_| drop(mic_tx.send(data.to_vec())),
        |e| eprintln!("mic: {e}"),
        None,
    )?;
    let play_buf = jitter.clone();
    let render_apm = apm.clone();
    // The echo canceller must hear exactly what the speaker plays, so it is fed from here
    // rather than from the receive thread, whose audio still sits in the jitter buffer.
    let mut played = Vec::with_capacity(APM_FRAME * 8);
    let speaker = output.build_output_stream(
        config,
        move |out: &mut [f32], _: &_| {
            let mut buf = play_buf.lock().unwrap();
            out.iter_mut()
                .for_each(|s| *s = buf.pop_front().unwrap_or(0.0));
            drop(buf);
            played.extend_from_slice(out);
            while played.len() >= APM_FRAME {
                if let Err(e) = render_apm.analyze_render_frame([&played[..APM_FRAME]]) {
                    eprintln!("render apm: {e:?}");
                }
                played.drain(..APM_FRAME);
            }
        },
        |e| eprintln!("speaker: {e}"),
        None,
    )?;

    let tx_conn = conn.clone();
    let mut encoder = Encoder::new(RATE, Channels::Mono, Application::Voip)?;
    thread::spawn(move || -> Result<(), Error> {
        let (mut pcm, mut seq, mut packet) = (Vec::new(), 0u32, [0u8; 1500]);
        for chunk in mic_rx {
            pcm.extend(chunk);
            while pcm.len() >= FRAME {
                let mut frame: Vec<f32> = pcm.drain(..FRAME).collect();
                cleaner.process(&mut frame)?;
                packet[..4].copy_from_slice(&seq.to_be_bytes());
                packet[4] = level(&frame);
                let n = encoder.encode_float(&frame, &mut packet[HEADER..])?;
                tx_conn.send_datagram(packet[..HEADER + n].to_vec().into())?;
                seq = seq.wrapping_add(1);
            }
        }
        Ok(())
    });

    let mut decoder = Decoder::new(RATE, Channels::Mono)?;
    let rx_conn = conn.clone();
    tokio::spawn(async move {
        let (mut pcm, mut next) = ([0f32; FRAME], None::<u32>);
        while let Ok(packet) = rx_conn.read_datagram().await {
            if packet.len() < HEADER {
                continue;
            }
            let head = &packet[..4];
            let seq = u32::from_be_bytes(head.try_into().unwrap());
            let mut frames = Vec::new();
            match next.map(|want| seq.wrapping_sub(want)) {
                // A late or duplicate packet: its slot has already been played or concealed.
                Some(gap) if gap > u32::MAX / 2 => continue,
                // Conceal at most a few lost frames; a longer gap is silence, not a smear.
                Some(gap) => (0..gap.min(3)).for_each(|_| frames.push(&[][..])),
                None => {}
            }
            frames.push(&packet[HEADER..]);
            let mut buf = jitter.lock().unwrap();
            for f in frames {
                let got = decoder.decode_float(f, &mut pcm, false)?;
                buf.extend(&pcm[..got]);
            }
            let excess = buf.len().saturating_sub(MAX_BUFFERED);
            buf.drain(..excess);
            next = Some(seq.wrapping_add(1));
        }
        Ok::<_, Error>(())
    });

    mic.play()?;
    speaker.play()?;
    eprintln!("voice: in call, clean {mode:?}  (ctrl-c to quit)");
    tokio::select! {
        why = conn.closed() => eprintln!("call ended: {why}"),
        _ = tokio::signal::ctrl_c() => {
            conn.close(0u32.into(), b"hangup");
            eprintln!("hung up");
        }
    }
    // Without this the close frame may never leave, and the peer waits out the idle timeout.
    endpoint.close().await;
    Ok(())
}

// What the peer would hear: clean, Opus-encode, decode. A trailing partial frame is dropped.
fn process_file(input: &str, output: &str, mode: Mode) -> Result<(), Error> {
    let mut reader = hound::WavReader::open(input)?;
    let spec = reader.spec();
    if spec.sample_rate != RATE || spec.channels != 1 {
        return Err(Error::WavFormat(input.into()));
    }
    let mut pcm: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = (1u32 << (spec.bits_per_sample - 1)) as f32;
            let ints = reader.samples::<i32>().collect::<Result<Vec<_>, _>>()?;
            ints.into_iter().map(|s| s as f32 / scale).collect()
        }
    };
    pcm.truncate(pcm.len() / FRAME * FRAME);

    let mut cleaner = Cleaner::new(mode, Arc::new(Processor::new(RATE)?));
    let mut encoder = Encoder::new(RATE, Channels::Mono, Application::Voip)?;
    let mut decoder = Decoder::new(RATE, Channels::Mono)?;
    let (mut packet, mut out) = ([0u8; 1500], [0f32; FRAME]);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output, spec)?;
    for frame in pcm.chunks_exact_mut(FRAME) {
        cleaner.process(frame)?;
        let n = encoder.encode_float(frame, &mut packet)?;
        let got = decoder.decode_float(&packet[..n], &mut out, false)?;
        for s in &out[..got] {
            writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
    }
    writer.finalize()?;
    Ok(())
}

// RFC 6464-style dBov, 0 = loudest, 127 = silence; lets a relay pick active speakers
// without decoding.
fn level(frame: &[f32]) -> u8 {
    let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
    (-20.0 * rms.max(1e-7).log10()).clamp(0.0, 127.0) as u8
}
