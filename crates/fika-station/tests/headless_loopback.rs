//! Whole station without any audio device: input "none", output "none",
//! loopback on. A sent message must come back decoded through the
//! streaming receiver in roughly real time.

use std::time::{Duration, Instant};

use fika_station::{Config, Station, StationEvent};

#[test]
fn message_round_trips_through_software_loopback() {
    let mut cfg: Config = toml::from_str(
        r#"
[station]
call = "SM6WJM"
grid = "JO57"
groups = ["fika"]
[audio]
input = "none"
output = "none"
sample_rate = 48000
loopback = true
[modem]
lane = 2
profile = "fast"
"#,
    )
    .unwrap();
    cfg.rig.tx_delay_ms = 10;
    let mut st = Station::start(cfg).expect("station starts without audio devices");
    st.send_text("headless loopback, hej!").unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut got = None;
    while Instant::now() < deadline && got.is_none() {
        for ev in st.poll() {
            if let StationEvent::Message { message, .. } = ev {
                got = Some(message);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let m = got.expect("decoded own burst via loopback");
    assert_eq!(m.text, "headless loopback, hej!");
    assert_eq!(st.chat.len(), 2, "own line plus decoded line");
}

/// Listen before talk: a foreign burst is already on our lane when we
/// press send, so our transmission must wait for it to end.
#[test]
fn transmission_waits_for_a_busy_lane() {
    use fika_modem::{Burst, FrameKind, Profile, Transmitter};
    use fika_proto::{Destination, Message, callsign};

    let mut cfg: Config = toml::from_str(
        "[station]\ncall = \"SM6WJM\"\n[audio]\ninput = \"none\"\noutput = \"none\"\nloopback = true\n[modem]\nlane = 2\n",
    )
    .unwrap();
    cfg.rig.tx_delay_ms = 10;
    let mut st = Station::start(cfg).unwrap();

    let foreign = Message {
        sender: callsign::pack("AD8KM"),
        dest: Destination::All,
        msg_id: 9,
        ack_req: false,
        text: "already talking on lane two for a while".into(),
    };
    let burst = Burst::new(FrameKind::Long, 3, foreign.to_blocks().unwrap()).unwrap();
    let foreign_airtime = burst.airtime_s(Profile::Fast);
    let audio = Transmitter::new(12_000)
        .render(&burst, 2, Profile::Fast, 0.0)
        .unwrap();
    // Let the receiver start its clock, then put the foreign burst on air.
    std::thread::sleep(Duration::from_millis(600));
    let t0 = Instant::now();
    st.inject_audio(audio);
    std::thread::sleep(Duration::from_millis(1500));
    st.send_text("me too").unwrap();

    let deadline = Instant::now() + Duration::from_secs(40);
    let mut tx_started_at = None;
    let mut waited = false;
    let mut foreign_decoded = false;
    while Instant::now() < deadline && tx_started_at.is_none() {
        for ev in st.poll() {
            match ev {
                StationEvent::TxWaiting { .. } => waited = true,
                StationEvent::TxStarted { .. } => tx_started_at = Some(t0.elapsed().as_secs_f64()),
                StationEvent::Message { message, .. } if message.sender == foreign.sender => {
                    foreign_decoded = true
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let started = tx_started_at.expect("our burst eventually goes out");
    assert!(waited, "listen-before-talk should have reported waiting");
    assert!(
        started >= foreign_airtime - 0.5,
        "transmitted at {started:.1} s, before the foreign burst ended at {foreign_airtime:.1} s"
    );
    assert!(foreign_decoded, "the foreign burst should still decode");
}

/// Escape hatch: aborting mid-burst releases PTT at once and marks the
/// message failed.
#[test]
fn abort_stops_transmission_and_marks_message_failed() {
    let mut cfg: Config = toml::from_str(
        "[station]\ncall = \"SM6WJM\"\n[audio]\ninput = \"none\"\noutput = \"none\"\nloopback = true\n",
    )
    .unwrap();
    cfg.rig.tx_delay_ms = 10;
    let mut st = Station::start(cfg).unwrap();
    let long = "x".repeat(60) + " " + &"y".repeat(60) + " " + &"z".repeat(60);
    st.send_text(&long).unwrap();
    let t0 = Instant::now();
    let mut started = false;
    while t0.elapsed() < Duration::from_secs(5) && !started {
        for ev in st.poll() {
            if let StationEvent::TxStarted { airtime_s, .. } = ev {
                assert!(airtime_s > 8.0, "want a long burst, got {airtime_s}");
                started = true;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(started);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(st.abort_tx());
    let t_abort = Instant::now();
    let mut aborted = None;
    while t_abort.elapsed() < Duration::from_secs(3) && aborted.is_none() {
        for ev in st.poll() {
            if let StationEvent::TxAborted { .. } = ev {
                aborted = Some(t_abort.elapsed());
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let dt = aborted.expect("TxAborted event");
    assert!(dt < Duration::from_millis(500), "abort took {dt:?}");
    assert!(!st.rig.ptt);
    assert_eq!(
        st.chat.last().unwrap().status.as_deref(),
        Some("failed (aborted)")
    );
}
