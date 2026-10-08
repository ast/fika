//! Two stations in one process sharing a PipeWire virtual sink. Skipped
//! when PipeWire or pw-cli is not available (CI, Pi without a session).

#![cfg(feature = "pipewire")]

use std::process::Command;
use std::time::{Duration, Instant};

use fika_station::{Config, Station, StationEvent};

fn config(call: &str, sink: &str) -> Config {
    toml::from_str(&format!(
        r#"
[station]
call = "{call}"
[audio]
backend = "pipewire"
input = "{sink}"
output = "{sink}"
sample_rate = 12000
loopback = false
[live]
enabled = true
snr_db = 0.0
channel = "awgn"
"#
    ))
    .unwrap()
}

fn destroy_sink(sink: &str) {
    if let Ok(o) = Command::new("pw-cli").args(["ls", "Node"]).output() {
        let text = String::from_utf8_lossy(&o.stdout);
        let mut last_id = None;
        for line in text.lines() {
            if let Some(id) = line.trim().strip_prefix("id ") {
                last_id = id.split(',').next().map(|s| s.trim().to_string());
            }
            if line.contains(&format!("node.name = \"{sink}\""))
                && let Some(id) = &last_id
            {
                let _ = Command::new("pw-cli").args(["destroy", id]).output();
            }
        }
    }
}

#[test]
fn message_crosses_the_ether_between_two_stations() {
    let sink = format!("fika-ether-test-{}", std::process::id());
    let created = Command::new("pw-cli")
        .args([
            "create-node",
            "adapter",
            &format!(
                "{{ factory.name=support.null-audio-sink node.name={sink} media.class=Audio/Sink object.linger=true audio.position=[MONO] }}"
            ),
        ])
        .output();
    match created {
        Ok(o) if o.status.success() => {}
        _ => {
            eprintln!("SKIP: pw-cli could not create a virtual sink (no PipeWire session?)");
            return;
        }
    }
    let sink2 = sink.clone();
    let result = std::panic::catch_unwind(move || {
        let mut a = match Station::start(config("SM6WJM", &sink2)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("SKIP: {e:#}");
                return;
            }
        };
        let mut b = Station::start(config("AD8KM", &sink2)).unwrap();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(2) {
            a.poll();
            b.poll();
            std::thread::sleep(Duration::from_millis(50));
        }
        a.send_text("över etern, hej AD8KM").unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut got = None;
        while Instant::now() < deadline && got.is_none() {
            a.poll();
            for ev in b.poll() {
                if let StationEvent::Message { message, det, .. } = ev {
                    eprintln!("B decoded at {:+.1} dB", det.snr_db());
                    got = Some(message.text);
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            got.as_deref(),
            Some("över etern, hej AD8KM"),
            "B should decode A's message"
        );
        // And back: B answers, A decodes it (B's own false slow candidates
        // from A's burst must not blind it, nor A's from its own).
        b.send_text("hej SM6WJM, hör dig fint").unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut back = None;
        while Instant::now() < deadline && back.is_none() {
            b.poll();
            for ev in a.poll() {
                if let StationEvent::Message { message, det, .. } = ev {
                    eprintln!("A decoded at {:+.1} dB", det.snr_db());
                    back = Some(message.text);
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            back.as_deref(),
            Some("hej SM6WJM, hör dig fint"),
            "A should decode B's reply"
        );
        assert!(
            a.chat.iter().all(|c| c.mine || c.from != "SM6WJM"),
            "A heard itself while keyed"
        );
    });
    destroy_sink(&sink);
    if let Err(p) = result {
        std::panic::resume_unwind(p);
    }
}
