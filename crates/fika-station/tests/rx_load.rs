//! Receiver thread load on a live-noise ether: does it keep up with real
//! time? Prints lag and the slowest processing step each second.
//! `cargo test --release -p fika-station --test rx_load -- --ignored --nocapture`

#![cfg(feature = "pipewire")]

use std::process::Command;
use std::time::{Duration, Instant};

use fika_station::{Config, Station, StationEvent};

#[test]
#[ignore = "30 s load probe, needs PipeWire"]
fn receiver_keeps_up_with_live_noise() {
    let sink = format!("fika-load-{}", std::process::id());
    let ok = Command::new("pw-cli")
        .args([
            "create-node",
            "adapter",
            &format!("{{ factory.name=support.null-audio-sink node.name={sink} media.class=Audio/Sink object.linger=true audio.position=[MONO] }}"),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("SKIP: no PipeWire");
        return;
    }
    let cfg: Config = toml::from_str(&format!(
        "[station]\ncall = \"SM6WJM\"\n[audio]\nbackend = \"pipewire\"\ninput = \"{sink}\"\noutput = \"{sink}\"\nsample_rate = 12000\n[live]\nenabled = true\nsnr_db = -8.0\n"
    ))
    .unwrap();
    let mut st = Station::start(cfg).unwrap();
    let t0 = Instant::now();
    let mut worst_lag = 0f32;
    let mut sent = 0;
    while t0.elapsed() < Duration::from_secs(30) {
        if sent == 0 && t0.elapsed() > Duration::from_secs(5)
            || sent == 1 && t0.elapsed() > Duration::from_secs(16)
        {
            st.send_text("load probe, hej hej").unwrap();
            sent += 1;
        }
        for ev in st.poll() {
            let t = t0.elapsed().as_secs_f64();
            match ev {
                StationEvent::RxHealth { lag_s, max_step_ms } => {
                    worst_lag = worst_lag.max(lag_s);
                    eprintln!("[{t:5.1}s] lag {lag_s:5.2} s  slowest step {max_step_ms:6.1} ms");
                }
                StationEvent::Message { message, det, .. } => {
                    eprintln!(
                        "[{t:5.1}s] decoded {:?} at {:+.1} dB",
                        message.text,
                        det.snr_db()
                    )
                }
                StationEvent::Detected { det, .. } => {
                    eprintln!(
                        "[{t:5.1}s] detected {} {} phase {}",
                        det.profile,
                        det.kind.name(),
                        det.phase
                    )
                }
                StationEvent::BurstFailed { det } => {
                    eprintln!("[{t:5.1}s] failed {} phase {}", det.profile, det.phase)
                }
                StationEvent::TxStarted { .. } => eprintln!("[{t:5.1}s] tx started"),
                StationEvent::TxFinished => eprintln!("[{t:5.1}s] tx finished"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = Command::new("sh")
        .args(["-c", &format!("pw-cli ls Node | grep -B4 'node.name = \"{sink}\"' | grep -oE 'id [0-9]+' | awk '{{print $2}}' | xargs -r -n1 pw-cli destroy")])
        .output();
    eprintln!("worst lag {worst_lag:.2} s");
    assert!(worst_lag < 1.0, "receiver fell {worst_lag:.1} s behind");
}
