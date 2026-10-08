# fika

Group chat over HF radio. Type a message, press send, and every station on the
channel that can hear you gets it. fika is a 500 Hz, 16-tone MFSK mode with
per-symbol tone hopping and LDPC coding, built for ordinary SSB transceivers, a
sound card, and a Raspberry Pi, with no dependence on internet time or GPS.

![fika-tui in software loopback: a message being sent and decoded, the heard list, the waterfall with the hopping tones in lane 1, and the log](docs/images/fika-tui.png)

*The terminal UI transmitting to itself over the speakers: the waterfall shows
the 16 tones hopping inside lane 1 while the same burst decodes at +12 dB.*

## In numbers

Sensitivity is quoted the way FT8 reports it: signal power against the
noise power in a 2500 Hz SSB passband. At −12 dB the signal carries a
sixteenth of the noise power in the receiver; you hear nothing but hiss.

| | fika fast | fika slow |
|---|---|---|
| Decodes down to (AWGN, 50 % of messages, simulated) | **−11.5 dB** | **−18.7 dB** |
| Bandwidth | 500 Hz | 500 Hz |
| Net text rate | ~60 bit/s, about 15 characters per second | ~12 bit/s, about 3 per second |
| 160-character message on air | about 13 s | about 65 s |
| Stations per SSB passband | 4 lanes, one station each, all decoded at once | same |
| Needs a clock, internet or GPS | no | no |

How that compares with modes people actually run, using their commonly
quoted figures:

| Mode | Bandwidth | Text rate | Solid copy down to | Error correction | Interference |
|---|---|---|---|---|---|
| RTTY 45 | 250 Hz | ~6 char/s | about −5 dB | none | a carrier in the shift wrecks it |
| PSK31 | 60 Hz | ~5 char/s | about −10 dB | none | prints garbage during QRM |
| **fika fast** | 500 Hz | ~15 char/s | **−11.5 dB** | LDPC | a carrier costs 1 symbol in 16 |
| Olivia 16/500 | 500 Hz | ~2 char/s | about −13 dB | heavy, rate 1/4 | good |
| **fika slow** | 500 Hz | ~3 char/s | **−18.7 dB** | LDPC | as above |
| JS8Call normal | 50 Hz | ~1.5 char/s | about −21 dB | LDPC | a carrier on the signal kills it |
| FT8 | 50 Hz | 77 bits per 15 s | −21 dB | LDPC | same, but it retries in 15 s |

What fika buys over the keyboard modes is three things at once: every
message is error-corrected, so what you read is what was sent; the tone
hops inside the lane every symbol, so a carrier or a noise burst costs a
few symbols instead of the message; and detection is non-coherent, so
ionospheric phase flutter that makes PSK31 unusable on polar paths costs
fika only its usual fading penalty. What it gives up is spectral
efficiency: PSK31 does about eight times more bits per hertz. Against FT8
and JS8Call it trades the last 2 to 3 dB of sensitivity and their fixed
time slots for free-form text at ten times the speed and no clock at all.

Caveats, honestly: the fika figures are from the simulator in this
repository, about 1 dB behind theory and not yet confirmed on the air.
Under selective fading (CCIR moderate) the fast profile needs about −7 dB,
because a 500 Hz lane can sit in a two-path notch for a whole burst. The
other modes' numbers are the figures their communities quote, give or take
a decibel.

Status: modem, protocol layer, channel simulator, station runtime and a
terminal UI work end to end in simulation and in software loopback on the
speakers. Nothing has been on the air yet.

- [PLAN.md](PLAN.md) — goals, decisions, rejected alternatives, roadmap.
- [docs/SPEC.md](docs/SPEC.md) — the normative protocol specification.
- [docs/DESIGN.md](docs/DESIGN.md) — rationale and open issues.

## Development shell

```sh
direnv allow    # once; loads the nix dev shell from flake.nix
cargo test
just --list     # sweeps, fading, multi-station, loopback recipes
```

The shell provides the Rust toolchain, ALSA headers for cpal, hamlib (for
`rigctld`), `just` and `sox`.

## Try it

```sh
# Encode a message to a WAV file and decode it again.
cargo run -p fika-cli -- tx --from SM6WJM --to @fika --text "Hej, kaffet är klart ☕ 73" -o fika.wav
cargo run -p fika-cli -- rx fika.wav

# Decode rate against SNR (2500 Hz reference, like FT8 reports) on AWGN
# and on a CCIR moderate Watterson channel.
cargo run --release -p fika-cli -- sim --profile fast --channel awgn --sweep=-14:-9:1 --trials 50
cargo run --release -p fika-cli -- sim --profile fast --channel moderate --sweep=-12:-2:2 --trials 30

# Four stations at once on four lanes, then two in one lane 10 dB apart.
cargo run --release -p fika-cli -- multi --stations 4 --snr=-6
cargo run --release -p fika-cli -- multi --stations 2 --lane 1 --snr=-4 --spread-db 10 -v

# Impairments: ITU-R F.1487 presets (low/mid/high-latitude quiet, moderate,
# disturbed, mid-nvis), interferers inside the lane, lightning static,
# frequency drift and the rig's SSB passband.
cargo run --release -p fika-cli -- sim --channel high-moderate --sweep=-8:0:2
cargo run --release -p fika-cli -- sim --snr=-6 --interferer cw:1100:6 --interferer rtty:1300:-3
cargo run --release -p fika-cli -- sim --snr=-8 --impulsive 5:2:20 --drift 1 --bandpass

# Regression suite: decode rate per scenario must stay above a floor.
just channels
```

The channel models live in `crates/fika-channel`: AWGN calibrated to the
2500 Hz reference, Watterson two-path fading with Gaussian Doppler spectra
(CCIR 520 and all ten ITU-R F.1487 presets), flat Rayleigh, frequency
offset and drift, sample-clock error, steady carriers, keyed CW, RTTY and
PSK31-like interferers, impulsive noise and a 300–2700 Hz passband.
`tools/gr_channel.py` passes a WAV through GNU Radio's gr-channels blocks
as an independent cross-check; it needs GNU Radio installed, for example
`nix shell nixpkgs#gnuradio`, and models mobile-style Jakes fading rather
than the HF Watterson model, so expect agreement in trend only.

## On the air, or just on the speakers

`fika-tui` is the station: a chat window, a heard list, a waterfall of the
passband and an input line. It is configured with a TOML file:

```sh
fika-tui --example-config > fika.toml     # edit call, grid, audio, rig
fika-tui --list-audio                      # device names to use in [audio]
fika-tui -c fika.toml
```

- **With a radio.** Set `[audio]` input and output to the rig's sound card
  (the IC-705 and FT-891 appear as "USB Audio CODEC") and `[rig] kind =
  "rigctld"` with the address of a running `rigctld`. PTT, dial frequency
  and optionally data mode go through hamlib.
- **Without a radio.** Set `input = "none"`, `output = "default"` and
  `loopback = true`. Bursts play on the speakers, and the same samples are
  fed into the receiver at playback pace, so you hear the modem and watch
  your own message decode. With a microphone as input you can also decode
  another computer across the room acoustically. `just tui-loopback` does
  this.

In the TUI, type and press Enter to send. `/to @group`, `/to CALL` or
`/to all` changes the destination, `/lane 0..3` and `/profile fast|slow`
the waveform, `/beacon` sends a beacon, F1 shows help. Direct messages
request an acknowledgement and show a delivery status.

Crates: `fika-modem` (physical layer), `fika-proto` (frames, callsigns,
groups, text coding), `fika-channel` (AWGN, Watterson, clock error,
interferers), `fika-cli` (the `fika` binary), `fika-station` (audio, rig,
streaming receiver, transmit queue), `fika-tui` (the terminal UI).

Copyright 2026 Albin Stigö SM6WJM.
