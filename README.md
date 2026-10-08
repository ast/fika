# fika

Group chat over HF radio. Type a message, press send, and every station on the
channel that can hear you gets it. fika is a 500 Hz, 16-tone MFSK mode with
per-symbol tone hopping and LDPC coding, built for ordinary SSB transceivers, a
sound card, and a Raspberry Pi, with no dependence on internet time or GPS.

Status: modem, protocol layer, channel simulator and a test CLI work end to
end in simulation. Nothing has been on the air yet.

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
```

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
