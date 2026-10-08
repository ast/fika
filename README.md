# fika

Group chat over HF radio. Type a message, press send, and every station on
the frequency that can hear you reads it, even if two or three of you pressed
send at the same moment. fika is a 64-tone MFSK mode filling a 2.4 kHz SSB
channel, with per-symbol tone hopping and a non-binary LDPC code over GF(64)
that is decoded at symbol level, so overlapping transmissions cost each other
a little information instead of the whole message. It runs on an ordinary SSB
transceiver, a sound card and a Raspberry Pi, and needs no clock, internet
or GPS.

![fika-tui in software loopback: a message being sent and decoded, the heard list, the waterfall with the hopping tones, and the log](docs/images/fika-tui.png)

*The terminal UI transmitting to itself over the speakers. The screenshot is
from the 500 Hz v1 waveform; v2 hops over the whole 300–2700 Hz band.*

## In numbers

Sensitivity is quoted the way FT8 reports it: signal power against the noise
power in a 2500 Hz SSB passband. At −10 dB the signal has a tenth of the
noise power; you hear hiss and nothing else.

| | fika fast | fika slow |
|---|---|---|
| Decodes down to (AWGN, 50 % of messages) | **−10.2 dB** | **−19.3 dB** |
| Bandwidth | 2400 Hz | 2400 Hz |
| Net text rate | ~110 bit/s, about 25 characters per second | ~19 bit/s, about 4 per second |
| 40-character message on air | 4.2 s | 25 s |
| 240-character message on air | 7.7 s | 46 s |
| Stations transmitting at the same time | 3 to 4 of similar strength, all decoded | same code, not yet measured |
| Needs a clock, internet or GPS | no | no |

Simultaneous senders, measured in simulation on the fast profile, same band,
same time:

| Scenario | Bursts decoded |
|---|---|
| 2, 3 or 4 senders of equal power at −6 dB | 100 % |
| 3 senders of equal power at −8 dB | 100 % |
| 2 senders 10 dB apart, the weaker at −8 dB | 95 % |
| 2 senders 20 dB apart, the weaker at −8 dB | 80 % (the strong one always) |
| 4 senders spread over 0–10 dB, the weakest at −6 dB | 78 % |

Against modes people actually run, using their commonly quoted figures:

| Mode | Bandwidth | Text rate | Solid copy down to | Error correction | Overlapping senders |
|---|---|---|---|---|---|
| RTTY 45 | 250 Hz | ~6 char/s | about −5 dB | none | no |
| PSK31 | 60 Hz | ~5 char/s | about −10 dB | none | no |
| Olivia 16/500 | 500 Hz | ~2 char/s | about −13 dB | heavy, rate 1/4 | no |
| **fika fast** | 2400 Hz | ~25 char/s | **−10.2 dB** | GF(64) LDPC | **yes, 3 to 4** |
| JS8Call normal | 50 Hz | ~1.5 char/s | about −21 dB | LDPC | no (time slots) |
| FT8 | 50 Hz | 77 bits per 15 s | −21 dB | LDPC | no (time slots) |
| **fika slow** | 2400 Hz | ~4 char/s | **−19.3 dB** | GF(64) LDPC | yes |

Why it can do that: a receiver that turns tone energies into bit likelihoods
loses half of them when two tones peak in one symbol, which is exactly the
information rate of a rate-1/2 code, so two overlapping bursts both die. A
decoder working on whole symbols over 64 tones loses about one bit of six per
overlapping sender. Add tone hopping across the whole band, so a carrier or a
fading notch costs symbols rather than messages, and non-coherent detection,
so ionospheric phase flutter costs only the usual fading penalty, and you
have a mode built for a crowded, informal channel rather than for spectral
efficiency, where PSK31 does about forty times more bits per hertz.

Caveats, honestly. All fika figures are from the simulator in this
repository, within a decibel of theory, and nothing has been on the air yet.
Each sender needs to be a few decibels above its own threshold for the
others to be tolerable; a crowd at the noise floor does not work. Under
selective fading (CCIR moderate) the fast profile needs about −2 dB. 2400 Hz
is a wide digital mode, for the wide-digimode band segments, not for the
narrow segments next to FT8. The other modes' numbers are what their
communities quote, give or take a decibel.

## Try it in two terminals

```sh
direnv allow          # once; the nix dev shell has everything
just tui-live SM6WJM  # terminal 1
just tui-live AD8KM   # terminal 2
```

Every station plays into and listens to one PipeWire virtual sink,
`fika-ether`, adds its own band noise at the configured SNR (default −8 dB),
and passes its bursts through a channel model on the way out. Type in both
windows and press Enter within the same second: both messages arrive. `just
tui-live OH2ABC -12 poor` joins at −12 dB over a CCIR poor channel. `just
live-down` removes the sink.

## With a radio

`fika-tui` is the station: chat, heard list, a waterfall of the passband and
an input line, configured by a TOML file.

```sh
fika-tui --example-config > fika.toml   # edit call, grid, audio, rig
fika-tui --list-audio                    # device names for [audio]
fika-tui -c fika.toml
fika-tui -c fika.toml --selftest "hej"   # headless smoke test of the audio chain
```

- Set `[audio]` input and output to the rig's sound card; the IC-705 and
  FT-891 appear as "USB Audio CODEC". Use the rig's widest data filter.
- Set `[rig] kind = "rigctld"` and the address of a running `rigctld`. PTT,
  dial frequency and optionally data mode go through hamlib.
- Without a radio: `input = "none"`, `output = "default"`, `loopback =
  true` plays bursts on the speakers and feeds the same samples back into
  the receiver, so you hear the modem and watch your own message decode.
  `just tui-loopback` does this.

In the TUI, type and press Enter to send. `/to @group`, `/to CALL` or `/to
all` changes the destination, `/profile fast|slow` the speed, `/beacon` sends
a beacon, Esc or Ctrl-G aborts a transmission, F1 shows help. The composer
has Emacs keybindings. Half duplex applies: a station hears nothing while it
is keyed, and the header shows `rx lag` if the receiver ever falls behind.

## Simulate

```sh
# A message to a WAV file and back.
cargo run -p fika-cli -- tx --from SM6WJM --to @fika --text "Hej, kaffet är klart ☕ 73" -o fika.wav
cargo run -p fika-cli -- rx fika.wav

# Decode rate against SNR on AWGN and on a CCIR moderate Watterson channel.
cargo run --release -p fika-cli -- sim --profile fast --channel awgn --sweep=-13:-9:1 --trials 50
cargo run --release -p fika-cli -- sim --profile fast --channel moderate --sweep=-8:0:2 --trials 30

# Several stations at once in the same band: equal power, then 10 dB apart.
cargo run --release -p fika-cli -- multi --stations 3 --snr=-6
cargo run --release -p fika-cli -- multi --stations 2 --snr=-8 --spread-db 10 -v

# Impairments: ITU-R F.1487 presets, in-band interferers, lightning static,
# drift, the rig's passband.
cargo run --release -p fika-cli -- sim --channel high-moderate --sweep=-8:0:2
cargo run --release -p fika-cli -- sim --snr=-6 --interferer cw:1100:6 --interferer rtty:1300:-3
cargo run --release -p fika-cli -- sim --snr=-8 --impulsive 5:2:20 --drift 1 --bandpass

just channels   # regression suite: decode rate per scenario against a floor
just sweep      # sensitivity on AWGN, both profiles
```

The channel models in `crates/fika-channel` cover AWGN calibrated to the
2500 Hz reference, Watterson two-path fading with Gaussian Doppler spectra
(CCIR 520 and all ten ITU-R F.1487 presets), flat Rayleigh, frequency offset
and drift, sample-clock error, carriers, keyed CW, RTTY and PSK31-like
interferers, impulsive noise and a 300–2700 Hz passband.
`tools/gr_channel.py` passes a WAV through GNU Radio's gr-channels blocks as
an independent cross-check; it needs GNU Radio (`nix shell
nixpkgs#gnuradio`) and models mobile-style fading, so expect agreement in
trend only.

## How it works

- **Waveform.** 64 tones at 37.5 Hz spacing, 318.75 to 2681.25 Hz, one tone
  at a time with Gaussian shaping, so the envelope is constant and ALC does
  not care. Fast symbols are 26.67 ms, slow ones 160 ms. Each symbol carries
  one GF(64) code symbol, and which tone carries which value is permuted
  every symbol by an order-64 Costas array with a random per-burst phase.
- **Burst.** A 24-symbol preamble (16 sync symbols from a Costas array, 8
  that identify the phase), then one to eight blocks of 4 pilots and 128
  coded symbols. A block carries 384 bits; block 0 holds the header (sender,
  destination, message id) and 282 payload bits, later blocks 349 each.
- **Code.** A rate-1/2 non-binary LDPC over GF(64), variable degree 2, check
  degree 4, built by progressive edge growth with cycle-aware labels from an
  in-crate deterministic generator, decoded by sum-product with
  Hadamard-transformed check nodes. 5 to 20 ms per block on a Pi 4.
- **Receiver.** Asynchronous: a two-dimensional Costas correlation over an
  energy matrix finds every burst start, including overlapping ones, then
  per-bin noise normalisation and an interference-aware likelihood feed the
  decoder. No time slots, no external clock.
- **Text.** UTF-8 with an order-1 adaptive range coder and a prior trained
  on chat text, about 4 bits per character today with a small corpus; groups
  by hashed name, callsigns packed FT8-style, optional ACKs on direct
  messages, beacons for presence.

The normative details are in [docs/SPEC.md](docs/SPEC.md); the reasoning,
rejected alternatives, measurements and open issues in
[docs/DESIGN.md](docs/DESIGN.md); decisions and roadmap in [PLAN.md](PLAN.md).

## Repository

| Crate | What |
|---|---|
| `fika-nb` | GF(64) arithmetic, LDPC code construction, decoder, likelihoods, multi-user gate simulations |
| `fika-modem` | Tone plan, Costas sequences, GFSK synthesis, energy matrix, preamble detector, demodulator |
| `fika-proto` | Frames, CRC, callsigns, groups, text coder, message assembly |
| `fika-channel` | AWGN, Watterson fading, clock error, interferers, impulsive noise |
| `fika-cli` | `fika tx`, `rx`, `sim`, `multi` and the channel regression suite |
| `fika-station` | Config, cpal and PipeWire audio, rigctld, streaming receiver, transmit queue, live channel |
| `fika-tui` | The terminal UI |

```sh
cargo test                     # unit and integration tests, about a minute
cargo build --no-default-features -p fika-tui   # ALSA only, e.g. for cross builds
```

Copyright 2026 Albin Stigö SM6WJM. MIT licence.
