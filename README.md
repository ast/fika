# fika

Group chat over HF radio. Type a message, press send, and every station on the
channel that can hear you gets it. fika is a 500 Hz, 16-tone MFSK mode with
per-symbol tone hopping and LDPC coding, built for ordinary SSB transceivers, a
sound card, and a Raspberry Pi, with no dependence on internet time or GPS.

Status: specification. No code yet.

- [PLAN.md](PLAN.md) — goals, decisions, rejected alternatives, roadmap.
- [docs/SPEC.md](docs/SPEC.md) — the normative protocol specification.
- [docs/DESIGN.md](docs/DESIGN.md) — rationale and open issues.

## Development shell

```sh
direnv allow    # once; loads the nix dev shell from flake.nix
```

The shell provides the Rust toolchain, ALSA headers for cpal, hamlib (for
`rigctld`), `just` and `sox`.

Copyright 2026 Albin Stigö SM6WJM.
