//! The station: owns audio and threads, exposes a poll-based API to a UI.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rand::Rng;

use fika_modem::resample::StreamDecimator;
use fika_modem::{Burst, FrameKind, Profile, SyncConfig, Transmitter};
use fika_proto::{Ack, Beacon, Destination, Message, callsign, group};

use crate::audio::{AudioEngine, OutputHandle};
use crate::config::{Config, RigKind};
use crate::event::{ChatLine, StationEvent};
use crate::heard::HeardList;
use crate::rig::{NoRig, Rig};
use crate::rigctld::Rigctld;
use crate::stream_rx::{RxEvent, StreamReceiver};
use crate::time::epoch_secs;

struct TxJob {
    label: String,
    burst: Burst,
    lane: usize,
    profile: Profile,
}

enum RxCommand {
    Inject(Vec<f32>),
}

pub struct RigStatus {
    pub name: String,
    pub connected: bool,
    pub freq_hz: Option<u64>,
    pub ptt: bool,
}

pub struct Station {
    pub cfg: Config,
    _audio: Option<AudioEngine>,
    events: Receiver<StationEvent>,
    tx_jobs: Sender<TxJob>,
    pub heard: HeardList,
    pub chat: Vec<ChatLine>,
    pub log: VecDeque<String>,
    pub rig: RigStatus,
    pub my_call: u32,
    pub dest: Destination,
    pub dest_label: String,
    pub lane: usize,
    pub profile: Profile,
    pub level_db: f32,
    pub peak: f32,
    pub spectrum: VecDeque<Vec<f32>>,
    pub tx_busy: Option<(String, Instant, f64)>,
    pub audio_names: (String, String),
    pending_acks: Vec<(Instant, Ack, Profile)>,
}

impl Station {
    pub fn start(cfg: Config) -> Result<Self> {
        cfg.validate()?;
        let (event_tx, events) = mpsc::channel::<StationEvent>();
        let (input_tx, input_rx) = mpsc::channel::<Vec<f32>>();
        let (rx_cmd_tx, rx_cmd_rx) = mpsc::channel::<RxCommand>();
        let (tx_jobs, tx_jobs_rx) = mpsc::channel::<TxJob>();

        let (audio, output) =
            AudioEngine::open(&cfg.audio, input_tx.clone()).context("open audio devices")?;
        let audio_names = (audio.input_name.clone(), audio.output_name.clone());
        let has_input = !cfg.audio.input.eq_ignore_ascii_case("none");

        // Receiver thread.
        {
            let ev = event_tx.clone();
            let rate = cfg.audio.sample_rate;
            let sync = SyncConfig {
                threshold: cfg.modem.threshold,
                ..Default::default()
            };
            thread::Builder::new()
                .name("fika-rx".into())
                .spawn(move || rx_thread(rate, has_input, sync, input_rx, rx_cmd_rx, ev))?;
        }

        // Transmit / rig thread.
        {
            let ev = event_tx.clone();
            let rig_cfg = cfg.rig.clone();
            let loopback = cfg.audio.loopback;
            let level = cfg.audio.tx_level;
            thread::Builder::new()
                .name("fika-tx".into())
                .spawn(move || {
                    tx_thread(rig_cfg, output, loopback, level, tx_jobs_rx, rx_cmd_tx, ev)
                })?;
        }

        let my_call = callsign::pack(&cfg.station.call);
        let (dest, dest_label) = match cfg.station.groups.first() {
            Some(g) => (Destination::Group(group::group_id(g)), format!("@{g}")),
            None => (Destination::All, "all".into()),
        };
        let profile: Profile = cfg.modem.profile.parse().map_err(anyhow::Error::msg)?;
        let rig_name = match cfg.rig.kind {
            RigKind::None => "no rig".to_string(),
            RigKind::Rigctld => format!("rigctld {}", cfg.rig.host),
        };
        let mut st = Self {
            lane: cfg.modem.lane,
            profile,
            cfg,
            _audio: Some(audio),
            events,
            tx_jobs,
            heard: HeardList::default(),
            chat: Vec::new(),
            log: VecDeque::new(),
            rig: RigStatus {
                name: rig_name,
                connected: false,
                freq_hz: None,
                ptt: false,
            },
            my_call,
            dest,
            dest_label,
            level_db: -100.0,
            peak: 0.0,
            spectrum: VecDeque::new(),
            tx_busy: None,
            audio_names,
            pending_acks: Vec::new(),
        };
        st.push_log(format!(
            "audio in: {} / out: {} @ {} Hz{}",
            st.audio_names.0,
            st.audio_names.1,
            st.cfg.audio.sample_rate,
            if st.cfg.audio.loopback {
                ", loopback on"
            } else {
                ""
            }
        ));
        Ok(st)
    }

    pub fn push_log(&mut self, s: String) {
        self.log
            .push_back(format!("{} {s}", crate::time::hms(epoch_secs())));
        while self.log.len() > 200 {
            self.log.pop_front();
        }
    }

    /// Parse a destination: "all", "@group" or a callsign.
    pub fn set_dest(&mut self, s: &str) {
        let s = s.trim();
        if s.eq_ignore_ascii_case("all") {
            self.dest = Destination::All;
            self.dest_label = "all".into();
        } else if let Some(g) = s.strip_prefix('@').or_else(|| s.strip_prefix('#')) {
            self.dest = Destination::Group(group::group_id(g));
            self.dest_label = format!("@{g}");
        } else {
            self.dest = Destination::Call(callsign::pack(s));
            self.dest_label = s.to_uppercase();
        }
    }

    /// Payload size preview for the composer: (bits, blocks, airtime s).
    pub fn preview(&self, text: &str) -> Option<(usize, usize, f64)> {
        let m = Message {
            sender: self.my_call,
            dest: self.dest,
            msg_id: 0,
            ack_req: false,
            text: text.to_string(),
        };
        let (bits, _) = m.payload();
        let blocks = m.blocks_needed().ok()?;
        let symbols =
            fika_modem::params::PREAMBLE_SYMBOLS + blocks * FrameKind::Long.block_symbols();
        Some((bits.len(), blocks, symbols as f64 * self.profile.symbol_s()))
    }

    pub fn send_text(&mut self, text: &str) -> Result<()> {
        let msg_id: u16 = rand::rng().random();
        let ack_req = matches!(self.dest, Destination::Call(_));
        let message = Message {
            sender: self.my_call,
            dest: self.dest,
            msg_id,
            ack_req,
            text: text.to_string(),
        };
        let blocks = message.to_blocks()?;
        let burst = Burst::new(FrameKind::Long, rand::rng().random_range(0..16), blocks)?;
        self.chat.push(ChatLine {
            epoch: epoch_secs(),
            from: self.cfg.station.call.clone(),
            to: self.dest_label.clone(),
            text: text.to_string(),
            snr_db: None,
            mine: true,
            status: Some(if ack_req {
                "sent, awaiting ack".into()
            } else {
                "sent".into()
            }),
            msg_id,
        });
        self.tx_jobs.send(TxJob {
            label: format!("message {msg_id:04X}"),
            burst,
            lane: self.lane,
            profile: self.profile,
        })?;
        Ok(())
    }

    pub fn send_beacon(&mut self) -> Result<()> {
        let tags: Vec<u16> = self
            .cfg
            .station
            .groups
            .iter()
            .map(|g| group::group_tag(g))
            .collect();
        let beacon = Beacon {
            sender: self.my_call,
            grid: self.cfg.station.grid.as_deref().and_then(grid_to_index),
            group_tags: [
                tags.first().copied().unwrap_or(0),
                tags.get(1).copied().unwrap_or(0),
            ],
            status: 0,
        };
        let burst = Burst::new(
            FrameKind::Short,
            rand::rng().random_range(0..16),
            vec![beacon.pack()],
        )?;
        self.tx_jobs.send(TxJob {
            label: "beacon".into(),
            burst,
            lane: self.lane,
            profile: self.profile,
        })?;
        Ok(())
    }

    fn send_ack(&mut self, ack: Ack, profile: Profile) -> Result<()> {
        let burst = Burst::new(FrameKind::Short, (ack.msg_id % 16) as u8, vec![ack.pack()])?;
        self.tx_jobs.send(TxJob {
            label: format!("ack {:04X}", ack.msg_id),
            burst,
            lane: self.lane,
            profile,
        })?;
        Ok(())
    }

    /// Drain events, update state. Returns the events for the UI log.
    pub fn poll(&mut self) -> Vec<StationEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            self.apply(&ev);
            out.push(ev);
        }
        let now = Instant::now();
        let due: Vec<(Ack, Profile)> = self
            .pending_acks
            .iter()
            .filter(|(t, _, _)| *t <= now)
            .map(|(_, a, p)| (a.clone(), *p))
            .collect();
        self.pending_acks.retain(|(t, _, _)| *t > now);
        for (ack, profile) in due {
            if let Err(e) = self.send_ack(ack, profile) {
                self.push_log(format!("ack failed: {e}"));
            }
        }
        if let Some((_, started, airtime)) = self.tx_busy
            && started.elapsed().as_secs_f64() > airtime + 5.0
        {
            self.tx_busy = None;
        }
        out
    }

    fn apply(&mut self, ev: &StationEvent) {
        match ev {
            StationEvent::Log(s) => self.push_log(s.clone()),
            StationEvent::Level { rms_db, peak } => {
                self.level_db = *rms_db;
                self.peak = *peak;
            }
            StationEvent::Spectrum(row) => {
                self.spectrum.push_back(row.clone());
                while self.spectrum.len() > 64 {
                    self.spectrum.pop_front();
                }
            }
            StationEvent::Detected { det, .. } => {
                self.push_log(format!(
                    "burst lane {} {} {} {:+.1} dB offset {:+.0} Hz",
                    det.lane,
                    det.profile,
                    det.kind.name(),
                    det.snr_db(),
                    det.freq_offset_hz
                ));
            }
            StationEvent::Message {
                det,
                message,
                blocks_ok,
                total,
                epoch,
            } => {
                let from = callsign::unpack(message.sender)
                    .map(|c| c.to_string())
                    .unwrap_or("?".into());
                self.heard.update(
                    message.sender,
                    from.clone(),
                    det.snr_db(),
                    det.lane,
                    det.profile,
                    *epoch,
                );
                let to = self.describe_dest(message.dest);
                self.chat.push(ChatLine {
                    epoch: *epoch,
                    from,
                    to,
                    text: message.text.clone(),
                    snr_db: Some(det.snr_db()),
                    mine: false,
                    status: None,
                    msg_id: message.msg_id,
                });
                self.push_log(format!(
                    "message {:04X} decoded, {blocks_ok}/{total} blocks",
                    message.msg_id
                ));
                if message.ack_req && message.dest == Destination::Call(self.my_call) {
                    let ack = Ack {
                        sender: self.my_call,
                        dest: message.sender,
                        msg_id: message.msg_id,
                        snr_db: det.snr_db().round().clamp(-32.0, 31.0) as i8,
                    };
                    self.pending_acks.push((
                        Instant::now() + Duration::from_millis(500),
                        ack,
                        det.profile,
                    ));
                }
            }
            StationEvent::Ack { det, ack, epoch } => {
                let from = callsign::unpack(ack.sender)
                    .map(|c| c.to_string())
                    .unwrap_or("?".into());
                self.heard.update(
                    ack.sender,
                    from.clone(),
                    det.snr_db(),
                    det.lane,
                    det.profile,
                    *epoch,
                );
                if ack.dest == self.my_call {
                    for line in self.chat.iter_mut().rev() {
                        if line.mine && line.msg_id == ack.msg_id {
                            line.status =
                                Some(format!("delivered to {from}, {:+} dB there", ack.snr_db));
                            break;
                        }
                    }
                }
                self.push_log(format!(
                    "ack from {from} for {:04X} ({:+} dB)",
                    ack.msg_id, ack.snr_db
                ));
            }
            StationEvent::Beacon { det, beacon, epoch } => {
                let from = callsign::unpack(beacon.sender)
                    .map(|c| c.to_string())
                    .unwrap_or("?".into());
                self.heard.update(
                    beacon.sender,
                    from.clone(),
                    det.snr_db(),
                    det.lane,
                    det.profile,
                    *epoch,
                );
                if let Some(g) = beacon.grid.and_then(index_to_grid) {
                    self.heard.set_grid(beacon.sender, g);
                }
                self.push_log(format!("beacon from {from}"));
            }
            StationEvent::BurstFailed { det } => {
                self.push_log(format!(
                    "burst lane {} {} failed to decode",
                    det.lane, det.profile
                ));
            }
            StationEvent::TxStarted { label, airtime_s } => {
                self.tx_busy = Some((label.clone(), Instant::now(), *airtime_s));
                self.rig.ptt = true;
                self.push_log(format!("tx {label}, {airtime_s:.1} s"));
            }
            StationEvent::TxFinished => {
                self.tx_busy = None;
                self.rig.ptt = false;
            }
            StationEvent::Rig {
                connected,
                freq_hz,
                ptt,
            } => {
                self.rig.connected = *connected;
                self.rig.freq_hz = *freq_hz;
                self.rig.ptt = *ptt;
            }
        }
    }

    pub fn describe_dest(&self, d: Destination) -> String {
        match d {
            Destination::All => "all".into(),
            Destination::Group(g) => self
                .cfg
                .station
                .groups
                .iter()
                .find(|name| group::group_id(name) == g)
                .map(|n| format!("@{n}"))
                .unwrap_or_else(|| format!("@{g:07X}")),
            Destination::Call(c) if c == self.my_call => "me".into(),
            Destination::Call(c) => callsign::unpack(c)
                .map(|c| c.to_string())
                .unwrap_or("?".into()),
        }
    }
}

/// 4-character Maidenhead to the 15-bit FT8 grid index.
pub fn grid_to_index(grid: &str) -> Option<u16> {
    let b = grid.trim().to_ascii_uppercase().into_bytes();
    if b.len() < 4 {
        return None;
    }
    let f1 = b[0].checked_sub(b'A')? as u16;
    let f2 = b[1].checked_sub(b'A')? as u16;
    let s1 = b[2].checked_sub(b'0')? as u16;
    let s2 = b[3].checked_sub(b'0')? as u16;
    if f1 > 17 || f2 > 17 || s1 > 9 || s2 > 9 {
        return None;
    }
    Some(((f1 * 18 + f2) * 10 + s1) * 10 + s2)
}

pub fn index_to_grid(i: u16) -> Option<String> {
    if i >= 32400 {
        return None;
    }
    let s2 = i % 10;
    let s1 = (i / 10) % 10;
    let f2 = (i / 100) % 18;
    let f1 = i / 1800;
    Some(format!(
        "{}{}{}{}",
        (b'A' + f1 as u8) as char,
        (b'A' + f2 as u8) as char,
        s1,
        s2
    ))
}

fn rx_thread(
    rate: u32,
    has_input: bool,
    sync: SyncConfig,
    input: Receiver<Vec<f32>>,
    cmds: Receiver<RxCommand>,
    ev: Sender<StationEvent>,
) {
    let factor = (rate / fika_modem::params::RX_SAMPLE_RATE) as usize;
    let mut dec = StreamDecimator::new(factor);
    let mut srx = StreamReceiver::new(sync);
    let mut out12 = Vec::new();
    let started = Instant::now();
    let mut synthetic_sent = 0u64;
    loop {
        while let Ok(cmd) = cmds.try_recv() {
            match cmd {
                RxCommand::Inject(s) => srx.inject(&s),
            }
        }
        if has_input {
            match input.recv_timeout(Duration::from_millis(100)) {
                Ok(chunk) => {
                    out12.clear();
                    dec.process(&chunk, &mut out12);
                    srx.push(&out12);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            // No input device: advance a silent stream on the wall clock.
            thread::sleep(Duration::from_millis(50));
            let due = (started.elapsed().as_secs_f64() * srx.fs() as f64) as u64;
            if due > synthetic_sent {
                let n = (due - synthetic_sent) as usize;
                srx.push(&vec![0f32; n]);
                synthetic_sent = due;
            }
        }
        let now = epoch_secs();
        for e in srx.process() {
            let sev = match e {
                RxEvent::Detected(det) => StationEvent::Detected { det, epoch: now },
                RxEvent::Message {
                    det,
                    message,
                    blocks_ok,
                    total,
                } => StationEvent::Message {
                    det,
                    message,
                    blocks_ok,
                    total,
                    epoch: now,
                },
                RxEvent::Ack { det, ack } => StationEvent::Ack {
                    det,
                    ack,
                    epoch: now,
                },
                RxEvent::Beacon { det, beacon } => StationEvent::Beacon {
                    det,
                    beacon,
                    epoch: now,
                },
                RxEvent::Failed(det) => StationEvent::BurstFailed { det },
                RxEvent::Level { rms_db, peak } => StationEvent::Level { rms_db, peak },
                RxEvent::Spectrum(row) => StationEvent::Spectrum(row),
            };
            if ev.send(sev).is_err() {
                return;
            }
        }
    }
}

fn tx_thread(
    rig_cfg: crate::config::RigCfg,
    mut output: OutputHandle,
    loopback: bool,
    level: f32,
    jobs: Receiver<TxJob>,
    rx_cmds: Sender<RxCommand>,
    ev: Sender<StationEvent>,
) {
    let mut rig: Box<dyn Rig> = match rig_cfg.kind {
        RigKind::None => Box::new(NoRig),
        RigKind::Rigctld => Box::new(Rigctld::new(&rig_cfg.host)),
    };
    if rig_cfg.set_data_mode
        && let Err(e) = rig.set_data_mode()
    {
        let _ = ev.send(StationEvent::Log(format!("rig: {e}")));
    }
    let mut tx_dev = Transmitter::new(output.rate);
    tx_dev.amplitude = level;
    let mut tx_12k = Transmitter::new(fika_modem::params::RX_SAMPLE_RATE);
    tx_12k.amplitude = level;
    let mut last_poll = Instant::now() - Duration::from_secs(10);
    loop {
        if last_poll.elapsed() > Duration::from_secs(2) {
            last_poll = Instant::now();
            let freq = rig.frequency().unwrap_or(None);
            let _ = ev.send(StationEvent::Rig {
                connected: rig.connected(),
                freq_hz: freq,
                ptt: false,
            });
        }
        let job = match jobs.recv_timeout(Duration::from_millis(200)) {
            Ok(j) => j,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let airtime = job.burst.airtime_s(job.profile);
        let audio = match tx_dev.render(&job.burst, job.lane, job.profile, 0.0) {
            Ok(a) => a,
            Err(e) => {
                let _ = ev.send(StationEvent::Log(format!("tx render failed: {e}")));
                continue;
            }
        };
        if let Err(e) = rig.ptt(true) {
            let _ = ev.send(StationEvent::Log(format!("PTT on failed: {e}")));
            continue;
        }
        let _ = ev.send(StationEvent::TxStarted {
            label: job.label.clone(),
            airtime_s: airtime,
        });
        thread::sleep(Duration::from_millis(rig_cfg.tx_delay_ms));
        if loopback && let Ok(a12) = tx_12k.render(&job.burst, job.lane, job.profile, 0.0) {
            let _ = rx_cmds.send(RxCommand::Inject(a12));
        }
        output.play_blocking(&audio);
        thread::sleep(Duration::from_millis(rig_cfg.tx_tail_ms));
        if let Err(e) = rig.ptt(false) {
            let _ = ev.send(StationEvent::Log(format!("PTT off failed: {e}")));
        }
        let _ = ev.send(StationEvent::TxFinished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_roundtrip() {
        for g in ["JO57", "FN31", "AA00", "RR99"] {
            assert_eq!(index_to_grid(grid_to_index(g).unwrap()).unwrap(), g);
        }
        assert!(grid_to_index("ZZ11").is_none());
    }
}
