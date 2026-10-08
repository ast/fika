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
