# fika — plan

fika is a new HF digital mode for amateur radio that works like a group chat:
you type a whole message, press send, and everyone on the channel who can hear
you gets it. It is designed for ordinary SSB transceivers (IC-705, FT-891), a
normal sound card, and a Raspberry Pi 4, and it is written in Rust.

The normative protocol lives in [docs/SPEC.md](docs/SPEC.md). The reasoning
behind each decision, the alternatives that were rejected, and the open issues
are in [docs/DESIGN.md](docs/DESIGN.md). This file records the decisions and the
roadmap.

## Goals

- **Chat, not keyboard-to-keyboard.** Nothing goes on air until you press send.
  One send is one burst. Receivers show a message only once it has fully decoded.
- **Tactical.** No dependency on internet time, NTP or GPS. Works with two radios
  and nothing else.
- **Shared channel.** Four 500 Hz lanes fit in one SSB passband. Stations on
  different lanes transmit at the same time and a listener decodes all of them.
- **Interference tolerant.** A carrier or a burst of noise inside the signal
  costs a few symbols, not the message.
- **Ordinary hardware.** Any SSB rig in data mode, any sound card, PTT through
  hamlib's rigctld. Runs comfortably on a Pi 4.
- **Band-plan friendly.** 500 Hz occupied bandwidth, so it fits the IARU
  Region 1 narrow-band digimode segments on every HF band.
- **Modern Rust.** clap, thiserror, anyhow, cpal. Reuse the DSP from
  [rust-radio](../rust-radio/) where it fits.

## Decisions

| Topic | Decision | Why |
|---|---|---|
| Timing | Asynchronous bursts, no time slots | Must work with no internet and jammed GPS. |
| Message model | Store-and-send: one send = one burst, no live typing | SMS / WhatsApp feel. |
| Waveform | Non-coherent MFSK with Gaussian pulse shaping, one tone at a time | Constant envelope, immune to ALC, robust on HF multipath. |
| Lane | 500 Hz wide, 16 tones at 31.25 Hz spacing | Fits every IARU R1 narrow digimode segment. Four lanes stack across a 300–2700 Hz passband and are all decoded from one FFT. |
| Multiple access and QRM | Per-symbol tone permutation inside the lane, derived from a Costas-array pattern, with a random per-transmission pattern phase | Narrow interferers become random erasures the FEC absorbs. Overlapping preambles are separated cleanly. Overlapping data at equal power does not decode; the honest multi-access mechanism is one station per lane plus listen-before-talk, with capture and successive cancellation for overlaps of 6 dB or more. |
| Profiles | Two: **fast** (32 ms symbols, 31.25 baud, 62.5 bit/s coded, about −12.5 dB) and **slow** (160 ms symbols, 6.25 baud, 12.5 bit/s coded, about −19.5 dB). Same tones, only symbol length differs. Receivers always run both on every lane | Local chat and weak-signal DX with a small spec. |
| FEC | LDPC with soft-decision belief propagation on the 16 tone energies | Within about 1 dB of the limit at these block sizes; handles erased symbols naturally. Start with CCSDS codes from `labrador-ldpc`, design a custom QC-LDPC later. |
| Message length | Up to about 240 characters; a burst is 1..N fixed LDPC blocks with sequence numbers | Tweet length bounds airtime per message. |
| Text coding | Compressed UTF-8: adaptive arithmetic coder with a fixed context model, about 2.5 bits per character, shortcuts for callsigns and common phrases | åäö and emoji work; 240 characters is about 600 bits. |
| Addressing | Sender callsign in 28 bits (FT8 packing with escape). Destination is a named group (hashed ID), a callsign, or all. Message ID for dedup. Hop count reserved | Chat rooms plus private side chats. |
| Receipts | Direct messages get an automatic short ACK burst, a tick in the UI, and a few retries with backoff. Group messages are fire-and-forget | Ten listeners would mean ten ACKs; avoid ACK storms. |
| Relay | Specified (dup suppression by message ID and hop count), not built in v1 | Don't paint into a corner. |
| Presence | Passive heard list (callsign, SNR, lane, time) from every decode, plus an optional idle beacon every 10–30 minutes | Cheap "online dots". |
| Channel access | Listen-before-talk with random backoff; sender picks the quietest lane | |
| Rig control | hamlib `rigctld` over TCP for PTT, frequency and mode. Pure-Rust client on our side | Covers IC-705 and FT-891 with no driver work. |
| Software v1 | A single CLI binary | Quickest path to a first QSO. |
| Name | fika | Swedish coffee-break chat. |

## Rejected alternatives

**Time slots like FT8 and JS8Call.** Slotting makes multi-signal decoding cheap
and gives free channel discipline, but it needs a clock good to about a second,
which means NTP or GPS. That fails exactly when a tactical mode is needed.

**OFDM with PSK or QAM, as in VARA and ARDOP.** Many more bits per hertz, but a
high crest factor means backing power off 6 to 10 dB, coherent tracking is
fragile at low SNR, and decoding several overlapping signals in one channel is
much harder. Good for point-to-point file transfer, wrong for a shared chat lane.

**Serial-tone 8-PSK with an adaptive equalizer, as in MIL-STD-188-110.** The
military answer: fast, interference tolerant, asynchronous bursts. But it fills
the whole 3 kHz channel so stations must take strict turns, and the equalizer
plus carrier and symbol tracking is a much larger modem to build before the
first chat happens. May return later as a wide profile behind the same framing.

**Narrow signals on a frequency grid without hopping.** Most stations per
channel and the politest neighbour, but a co-slot interferer or a same-slot
collision kills the message outright.

**A 2400 Hz, 64-tone hopping signal.** About 1 dB more sensitive and twice as
fast per sender, and more fade diversity. Rejected because it is confined to the
small wide-digimode segments and lands on every narrow signal nearby. The 500 Hz
lane is usable almost everywhere and four lanes recover the aggregate capacity.

**Polar and convolutional codes.** Polar is marginally better under 256 bits
but has no mature Rust crate and list decoding is more work. Convolutional with
Viterbi is 1 to 2 dB worse than LDPC.

**Daemon plus web UI, CAT drivers via sidebridge.** Both are good later steps
and neither blocks the modem. v1 is one CLI binary and rigctld.

## Roadmap

1. **v0 — specification.** `docs/SPEC.md` and `docs/DESIGN.md`. This phase.
2. **v1 — modem and simulator.** Rust workspace: `fika-modem` (GFSK synthesis,
   FFT energy matrix, Costas sync, soft demodulation, LDPC), `fika-sim` (AWGN
   and Watterson HF channel, sensitivity curves, overlap and interference
   tests). Numbers in the spec get validated here before any radio is keyed.
3. **v2 — protocol and CLI.** Text coder, framing, addressing, ACK and retry,
   heard list, LBT, rigctld client, cpal audio in and out, `fika` CLI.
4. **v3 — on air.** Cross-compile for aarch64, run on `shack` with the IC-705,
   first QSOs, tune thresholds against real paths.
5. **Later.** Relay, beacon scheduling refinements, web UI for a phone over
   Wi-Fi, wide profile for the wide-digimode segments, custom QC-LDPC code.

## Reuse from rust-radio

- `doublemap` — lock-free ring buffers for the audio path.
- `filters::ringbuffer::RingBuffer` and the `DelayLine` trait.
- `filters::design` — `windowed_sinc_lp`, `fred_harris_taps`, `ssb_bandpass`
  for the receive front end and decimation from 48 kHz.
- `filters::rotate::ComplexRotator` — lane shifting.
- `signals::ComplexOscillator` — tone synthesis reference.
- `civlink/src/audio/audio_capture.rs` and `sideband/src/main.rs` — cpal
  device selection and stream setup.
- `pool` — allocation-free buffers on the real-time path.

## Development

```sh
direnv allow        # once; loads the nix dev shell from flake.nix
nix develop         # or enter it by hand
```

The shell provides the Rust toolchain, ALSA headers for cpal, hamlib (for
`rigctld`), `just` and `sox`.
