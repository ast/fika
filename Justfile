# fika task runner. `just --list` shows recipes.

set shell := ["bash", "-cu"]

default:
    @just --list

build:
    cargo build --release

test:
    cargo test

# Regenerate the text-model prior from crates/fika-proto/corpus/*.txt.
prior:
    cargo run -q -p fika-proto --bin train_prior
    cargo fmt -p fika-proto

# Sensitivity sweeps on AWGN for both profiles.
sweep: build
    target/release/fika sim --profile fast --channel awgn --sweep=-14:-9:1 --trials 50
    target/release/fika sim --profile slow --channel awgn --sweep=-21:-16:1 --trials 20

# Fading channels, fast profile.
fading: build
    target/release/fika sim --profile fast --channel good --sweep=-12:-2:2 --trials 30
    target/release/fika sim --profile fast --channel moderate --sweep=-12:-2:2 --trials 30
    target/release/fika sim --profile fast --channel poor --sweep=-12:-2:2 --trials 30

# Several stations at once: different lanes, then one lane with level spread.
multi: build
    target/release/fika multi --stations 4 --snr=-6 --trials 10
    target/release/fika multi --stations 2 --lane 1 --snr=-4 --spread-db 10 --trials 10 -v

# Encode a message to a WAV and decode it again.
loop text="Hej från fika, 73 de SM6WJM":
    cargo run -q -p fika-cli -- tx --from SM6WJM --to @fika --text "{{text}}" -o /tmp/fika-loop.wav
    cargo run -q -p fika-cli -- rx /tmp/fika-loop.wav

# Run the TUI with a config file (default fika.toml; see fika.example.toml).
tui config="fika.toml":
    cargo run --release -p fika-tui -- -c {{config}}

# TUI without a radio: no input, speakers out, software loopback.
tui-loopback:
    printf '[station]\ncall = "SM6WJM"\ngrid = "JO57"\n[audio]\ninput = "none"\noutput = "default"\nloopback = true\n' > /tmp/fika-loopback.toml
    cargo run --release -p fika-tui -- -c /tmp/fika-loopback.toml

# Channel regression suite: decode rate per scenario against a floor.
channels:
    cargo test --release -p fika-cli --test channels -- --ignored --nocapture


# Live channel on this host: every station plays into and listens to one
# PipeWire virtual sink ("fika-ether"), adds its own band noise at snr dB and
# passes its bursts through the channel model. Open one per terminal:
#   just tui-live SM6WJM
#   just tui-live AD8KM 2 -12 poor
tui-live call="SM6WJM" lane="1" snr="-8" channel="awgn":
    pw-cli ls Node | grep -q 'node.name = "fika-ether"' || \
      pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=fika-ether node.description="fika ether" media.class=Audio/Sink object.linger=true audio.position=[MONO] }' >/dev/null
    printf '[station]\ncall = "{{call}}"\ngrid = "JO57"\n[audio]\nbackend = "pipewire"\ninput = "fika-ether"\noutput = "fika-ether"\nsample_rate = 12000\nloopback = false\n[modem]\nlane = {{lane}}\n[live]\nenabled = true\nsnr_db = {{snr}}\nchannel = "{{channel}}"\n' > /tmp/fika-live-{{call}}.toml
    cargo run --release -p fika-tui -- -c /tmp/fika-live-{{call}}.toml

# Remove the virtual sink again.
live-down:
    pw-cli ls Node | grep -B4 'node.name = "fika-ether"' | grep -oE 'id [0-9]+' | awk '{print $2}' | xargs -r -n1 pw-cli destroy
