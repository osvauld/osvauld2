# /// script
# requires-python = ">=3.10,<3.13"
# dependencies = ["numpy", "scipy", "soundfile", "pesq", "pystoi", "onnxruntime"]
# ///
"""Score `voice process` modes: mix clean speech with a noise recording, run each mode,
print PESQ / STOI / DNSMOS per mode and noise level.

    uv run scripts/voice_eval.py clean.wav keys.wav [--out DIR]

Both inputs are 48 kHz mono. PESQ and STOI compare against the clean take, so they catch
damage to the voice; DNSMOS needs no reference, so BAK says how much noise is left.
"""
import argparse
import subprocess
import urllib.request
from pathlib import Path

import numpy as np
import onnxruntime as ort
import soundfile as sf
from pesq import pesq
from pystoi import stoi
from scipy.signal import correlate, resample_poly

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target/release/voice"
MODES = ["none", "apm", "rnn", "rnn-gate"]
SNRS = [None, 10, 0]  # dB of speech over noise; None = clean speech, measures pure damage
DNSMOS_URL = "https://github.com/microsoft/DNS-Challenge/raw/master/DNSMOS/DNSMOS/sig_bak_ovr.onnx"
# Microsoft's calibration from raw model output to MOS (dnsmos_local.py, non-personalized).
POLY = {
    "sig": np.poly1d([-0.08397278, 1.22083953, 0.0052439]),
    "bak": np.poly1d([-0.13166888, 1.60915514, -0.39604546]),
    "ovr": np.poly1d([-0.06766283, 1.11546468, 0.04602535]),
}


def load(path):
    audio, rate = sf.read(path, dtype="float32")
    if rate != 48000 or audio.ndim != 1:
        raise SystemExit(f"{path}: need 48 kHz mono")
    return audio


def mix(clean, noise, snr):
    if snr is None:
        return clean.copy()
    noise = np.resize(noise, len(clean))
    gain = np.sqrt(np.mean(clean**2) / (np.mean(noise**2) * 10 ** (snr / 10)))
    return np.clip(clean + gain * noise, -1, 1)


def align(ref, out):
    # Opus and the processors add a few ms of delay; STOI needs the takes lined up.
    a, v = out[: 48000 * 5], ref[: 48000 * 5]
    lag = int(np.argmax(correlate(a, v, "full"))) - (len(v) - 1)
    out = out[int(np.clip(lag, 0, 4800)) :]
    m = min(len(ref), len(out))
    return ref[:m], out[:m]


def dnsmos(sess, audio16):
    seg = int(9.01 * 16000)
    while len(audio16) < seg:
        audio16 = np.concatenate([audio16, audio16])
    name = sess.get_inputs()[0].name
    scores = []
    for start in range(0, len(audio16) - seg + 1, 16000):
        raw = sess.run(None, {name: audio16[None, start : start + seg].astype("float32")})[0][0]
        scores.append([POLY[k](v) for k, v in zip(("sig", "bak", "ovr"), raw)])
    return np.mean(scores, axis=0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("clean")
    ap.add_argument("noise")
    ap.add_argument("--out", default=str(ROOT / "target/voice_eval"))
    args = ap.parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    model = out_dir / "sig_bak_ovr.onnx"
    if not model.exists():
        urllib.request.urlretrieve(DNSMOS_URL, model)
    sess = ort.InferenceSession(str(model))
    subprocess.run(["cargo", "build", "-q", "--release", "-p", "voice"], cwd=ROOT, check=True)

    clean, noise = load(args.clean), load(args.noise)
    print(f"{'snr':>5} {'clean':>8} {'pesq':>5} {'stoi':>5} {'sig':>5} {'bak':>5} {'ovr':>5}")
    for snr in SNRS:
        label = "clean" if snr is None else f"{snr}dB"
        mixed = out_dir / f"in_{label}.wav"
        sf.write(mixed, mix(clean, noise, snr), 48000, subtype="PCM_16")
        for mode in MODES:
            out = out_dir / f"out_{label}_{mode}.wav"
            subprocess.run([BIN, "process", mixed, out, "--clean", mode], check=True)
            ref, got = align(clean, load(out))
            ref16, got16 = resample_poly(ref, 1, 3), resample_poly(got, 1, 3)
            p = pesq(16000, ref16, got16, "wb")
            s = stoi(ref16, got16, 16000)
            sig, bak, ovr = dnsmos(sess, got16)
            print(f"{label:>5} {mode:>8} {p:5.2f} {s:5.2f} {sig:5.2f} {bak:5.2f} {ovr:5.2f}")
    print(f"\nlisten: {out_dir}/out_*.wav")


if __name__ == "__main__":
    main()
