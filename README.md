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

Crates: `fika-modem` (physical layer), `fika-proto` (frames, callsigns,
groups, text coding), `fika-channel` (AWGN, Watterson, clock error,
interferers), `fika-cli` (the `fika` binary).

Copyright 2026 Albin Stigö SM6WJM.
