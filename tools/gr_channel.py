#!/usr/bin/env python3
"""Pass a fika WAV through GNU Radio's gr-channels blocks.

An independent cross-check of crates/fika-channel. gr-channels models
mobile-style fading (sum-of-sinusoids, Jakes/flat Doppler spectrum)
rather than the Gaussian-spectrum Watterson model that is standard for HF,
so expect the two to agree in trend, not in tenths of a decibel.

Needs GNU Radio 3.10 with gr-channels and gr-filter, e.g.

    nix shell nixpkgs#gnuradio -c python3 tools/gr_channel.py in.wav out.wav \
        --delay-ms 1.0 --doppler-hz 0.5 --snr -8

Then decode with `fika rx out.wav`. SNR is in the usual 2500 Hz reference
bandwidth; the wanted signal is assumed to be the 0.5 peak (0.125 power)
that `fika tx` produces.
"""
import argparse
import math
import sys
import wave

import numpy as np

try:
    from gnuradio import blocks, channels, filter as grfilter, gr
except ImportError:  # pragma: no cover
    sys.exit("GNU Radio python bindings not found; try: nix shell nixpkgs#gnuradio")

REF_POWER = 0.5 ** 2 / 2  # fika tx default amplitude


def read_wav(path):
    with wave.open(path, "rb") as w:
        fs = w.getframerate()
        n = w.getnframes()
        raw = w.readframes(n)
        width = w.getsampwidth()
        ch = w.getnchannels()
    if width == 2:
        x = np.frombuffer(raw, dtype="<i2").astype(np.float32) / 32768.0
    elif width == 4:
        x = np.frombuffer(raw, dtype="<f4")
    else:
        sys.exit(f"unsupported sample width {width}")
    return fs, x[::ch]


def write_wav(path, fs, x):
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(fs)
        w.writeframes((np.clip(x, -1, 1) * 32767).astype("<i2").tobytes())


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("input")
    ap.add_argument("output")
    ap.add_argument("--delay-ms", type=float, default=0.0, help="second path delay (0 = single path)")
    ap.add_argument("--doppler-hz", type=float, default=0.0, help="Doppler spread per path (0 = static)")
    ap.add_argument("--snr", type=float, default=None, help="SNR in 2500 Hz, dB (omit for no noise)")
    ap.add_argument("--offset-hz", type=float, default=0.0, help="frequency offset")
    ap.add_argument("--ppm", type=float, default=0.0, help="sample clock error")
    ap.add_argument("--seed", type=int, default=1)
    a = ap.parse_args()

    fs, x = read_wav(a.input)
    tb = gr.top_block()
    src = blocks.vector_source_f(x.tolist(), False)
    # Analytic signal so the fading taps rotate phase properly.
    hilb = grfilter.hilbert_fc(129)
    chain = [src, hilb]
    if a.delay_ms > 0 or a.doppler_hz > 0:
        delays = [0.0] if a.delay_ms == 0 else [0.0, a.delay_ms * 1e-3 * fs]
        mags = [1.0] * len(delays)
        fading = channels.selective_fading_model(
            8,                      # sinusoids per tap
            a.doppler_hz / fs,      # normalised max Doppler
            False, 0.0,             # no line of sight
            a.seed, delays, mags, 8,
        )
        chain.append(fading)
    noise_v = 0.0
    if a.snr is not None:
        # noise power in fs/2 = REF_POWER / snr * (fs/2) / 2500; complex noise
        # voltage per component is sqrt of that.
        total = REF_POWER / (10 ** (a.snr / 10)) * (fs / 2) / 2500.0
        noise_v = math.sqrt(total)
    cm = channels.channel_model(noise_v, a.offset_hz / fs, 1.0 + a.ppm * 1e-6, [1.0], a.seed)
    chain.append(cm)
    to_real = blocks.complex_to_real()
    sink = blocks.vector_sink_f()
    chain += [to_real, sink]
    for u, v in zip(chain, chain[1:]):
        tb.connect(u, v)
    tb.run()
    y = np.array(sink.data(), dtype=np.float32)
    # Hilbert doubles nothing here (hilbert_fc keeps unity gain); the
    # real part of the analytic signal is the original signal.
    write_wav(a.output, fs, y)
    print(f"wrote {a.output}: {len(y) / fs:.1f} s, fs {fs}")


if __name__ == "__main__":
    main()
