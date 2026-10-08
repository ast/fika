//! Streaming receiver: a rolling 12 kHz buffer with periodic preamble
//! detection and block decoding as audio arrives. Also produces the input
//! level, a passband spectrum for the UI, and per-lane busy state for
//! listen-before-talk.

use std::collections::VecDeque;

use fika_modem::energy::EnergyMatrix;
use fika_modem::params::{LANES, PREAMBLE_SYMBOLS, RX_SAMPLE_RATE, TONES};
use fika_modem::{Detection, FrameKind, Profile, Receiver, SyncConfig};
use fika_proto::{Destination, Frame, Message, MessageAssembler};

pub enum RxEvent {
    Detected(Detection),
    Message {
        det: Detection,
        message: Message,
        blocks_ok: usize,
        total: usize,
    },
    Ack {
        det: Detection,
        ack: fika_proto::Ack,
    },
    Beacon {
        det: Detection,
        beacon: fika_proto::Beacon,
    },
    Failed(Detection),
    /// The burst overlapped one of our own transmissions.
    Lost(Detection),
    Level {
        rms_db: f32,
        peak: f32,
    },
    Spectrum(Vec<f32>),
}

/// Per-lane channel state for listen-before-talk, in absolute samples.
#[derive(Clone, Copy, Debug, Default)]
pub struct LaneStatus {
    /// The lane is busy until this absolute sample index.
    pub busy_until: f64,
}

struct Tracked {
    /// Detection with `start_sample` as an absolute sample index.
    det: Detection,
    next_block: usize,
    total: Option<usize>,
    asm: MessageAssembler,
    blocks_ok: usize,
    done: bool,
}

impl Tracked {
    fn sps(&self, fs: u32) -> f64 {
        self.det.profile.samples_per_symbol(fs).unwrap() as f64
    }

    /// Predicted end: known block count, or the maximum until block 0 says.
    fn predicted_end(&self, fs: u32) -> f64 {
        let blocks = self.total.unwrap_or(self.det.kind.max_blocks());
        self.det.start_sample
            + (PREAMBLE_SYMBOLS + blocks * self.det.kind.block_symbols()) as f64 * self.sps(fs)
    }
}

struct Recent {
    lane: usize,
    profile: Profile,
    start: f64,
}

pub struct StreamReceiver {
    fs: u32,
    buf: Vec<f32>,
    /// Absolute index of `buf[0]`.
    base: u64,
    /// Absolute number of samples received so far.
    total: u64,
    rx: Receiver,
    tracked: Vec<Tracked>,
    /// Finished bursts, kept while they can still appear in a detection window.
    recent: Vec<Recent>,
    next_detect_at: u64,
    next_spectrum_at: u64,
    inject: VecDeque<f32>,
    spectrum_fft: EnergyMatrix,
    /// Our own transmit intervals (absolute samples); bursts overlapping
    /// them are reported lost rather than failed.
    tx_intervals: Vec<(f64, f64)>,
    /// Per lane: busy-until from energy, tracked bursts and reservations.
    lane_hot_until: [f64; LANES],
    lane_reserved_until: [f64; LANES],
    pub keep_s: f64,
    /// Our own packed callsign, so ACK reservations skip messages to us.
    pub my_call: u32,
}

const DETECT_EVERY_S: f64 = 1.0;
const SPECTRUM_EVERY_S: f64 = 0.25;
const WINDOW_FAST_S: f64 = 20.0;
const WINDOW_SLOW_S: f64 = 60.0;
/// A lane counts as busy when its hottest tone bin is this far above the
/// passband median ...
const LANE_HOT_DB: f32 = 10.0;
/// ... and within this much of the hottest lane in the passband.
const LANE_RELATIVE_DB: f32 = 12.0;

impl StreamReceiver {
    pub fn new(cfg: SyncConfig) -> Self {
        let mut rx = Receiver::new();
        rx.cfg = cfg;
        Self {
            fs: RX_SAMPLE_RATE,
            buf: Vec::new(),
            base: 0,
            total: 0,
            rx,
            tracked: Vec::new(),
            recent: Vec::new(),
            next_detect_at: (RX_SAMPLE_RATE as f64 * 2.0) as u64,
            next_spectrum_at: 0,
            inject: VecDeque::new(),
            spectrum_fft: EnergyMatrix::new(Profile::Fast),
            tx_intervals: Vec::new(),
            lane_hot_until: [0.0; LANES],
            lane_reserved_until: [0.0; LANES],
            keep_s: 240.0,
            my_call: u32::MAX,
        }
    }

    pub fn fs(&self) -> u32 {
        self.fs
    }

    /// Queue samples to be mixed into the incoming audio at real-time pace
    /// (software loopback of our own transmissions).
    pub fn inject(&mut self, samples: &[f32]) {
        self.inject.extend(samples.iter().copied());
    }

    /// Absolute sample position of "now".
    pub fn position(&self) -> u64 {
        self.total
    }

    /// Record one of our own transmit intervals in absolute samples.
    pub fn note_tx(&mut self, start: f64, end: f64) {
        self.tx_intervals.push((start, end));
        let horizon = self.total as f64 - self.keep_s * self.fs as f64;
        self.tx_intervals.retain(|&(_, e)| e > horizon);
    }

    /// Lane status for listen-before-talk.
    pub fn lane_status(&self) -> [LaneStatus; LANES] {
        let mut out = [LaneStatus::default(); LANES];
        for (lane, slot) in out.iter_mut().enumerate() {
            let mut until = self.lane_hot_until[lane].max(self.lane_reserved_until[lane]);
            for t in self
                .tracked
                .iter()
                .filter(|t| !t.done && t.det.lane == lane)
            {
                until = until.max(t.predicted_end(self.fs));
            }
            slot.busy_until = until;
        }
        out
    }

    /// Append a chunk of 12 kHz audio.
    pub fn push(&mut self, chunk: &[f32]) {
        let start = self.buf.len();
        self.buf.extend_from_slice(chunk);
        if !self.inject.is_empty() {
            for s in &mut self.buf[start..] {
                match self.inject.pop_front() {
                    Some(v) => *s += v,
                    None => break,
                }
            }
        }
        self.total += chunk.len() as u64;
        let keep = (self.keep_s * self.fs as f64) as usize;
        if self.buf.len() > keep + keep / 4 {
            let drop = self.buf.len() - keep;
            self.buf.drain(..drop);
            self.base += drop as u64;
        }
    }

    /// Run detection and decoding that is due. Call after each push.
    pub fn process(&mut self) -> Vec<RxEvent> {
        let mut events = Vec::new();
        if self.total >= self.next_spectrum_at {
            self.next_spectrum_at = self.total + (SPECTRUM_EVERY_S * self.fs as f64) as u64;
            self.level_and_spectrum(&mut events);
        }
        if self.total >= self.next_detect_at {
            self.next_detect_at = self.total + (DETECT_EVERY_S * self.fs as f64) as u64;
            self.detect(&mut events);
        }
        self.decode_due(&mut events);
        for t in self.tracked.iter().filter(|t| t.done) {
            self.recent.push(Recent {
                lane: t.det.lane,
                profile: t.det.profile,
                start: t.det.start_sample,
            });
        }
        self.tracked.retain(|t| !t.done);
        let horizon = self.total as f64 - (WINDOW_SLOW_S + 5.0) * self.fs as f64;
        self.recent.retain(|r| r.start > horizon);
        events
    }

    fn level_and_spectrum(&mut self, events: &mut Vec<RxEvent>) {
        let n = (self.fs as f64 * SPECTRUM_EVERY_S) as usize;
        if self.buf.len() < n.max(self.spectrum_fft.window) {
            return;
        }
        let tail = &self.buf[self.buf.len() - n..];
        let rms = (tail.iter().map(|v| v * v).sum::<f32>() / n as f32).sqrt();
        let peak = tail.iter().fold(0f32, |m, v| m.max(v.abs()));
        events.push(RxEvent::Level {
            rms_db: 20.0 * rms.max(1e-6).log10(),
            peak,
        });
        // One Hann-windowed frame over the last symbol period. The
        // detector needs the rectangular window for tone orthogonality;
        // this measurement does not, and Hann keeps a strong lane from
        // leaking into its neighbours.
        let w = self.spectrum_fft.window;
        let slice = &self.buf[self.buf.len() - w..];
        let windowed: Vec<f32> = slice
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                v * (0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / w as f32).cos())
            })
            .collect();
        self.spectrum_fft.compute(&windowed);
        let bin_hz = self.spectrum_fft.bin_hz();
        let lo = (300.0 / bin_hz) as usize;
        let hi = (2700.0 / bin_hz) as usize;
        let row: Vec<f32> = (lo..hi)
            .map(|b| 10.0 * self.spectrum_fft.raw_at(0, b).max(1e-12).log10())
            .collect();
        // Lane energy for listen-before-talk: the hottest tone bin of a
        // lane must stand LANE_HOT_DB above the passband median and within
        // LANE_RELATIVE_DB of the hottest lane, so neighbour leakage in a
        // quiet passband does not count.
        let mut sorted = row.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let floor = sorted[sorted.len() / 2];
        let peaks: Vec<f32> = (0..LANES)
            .map(|lane| {
                (0..TONES)
                    .map(|k| {
                        10.0 * self
                            .spectrum_fft
                            .raw_at(0, self.spectrum_fft.tone_bin(lane, k))
                            .max(1e-12)
                            .log10()
                    })
                    .fold(f32::MIN, f32::max)
            })
            .collect();
        let hottest = peaks.iter().copied().fold(f32::MIN, f32::max);
        for (lane, &peak_db) in peaks.iter().enumerate() {
            if peak_db - floor > LANE_HOT_DB && peak_db > hottest - LANE_RELATIVE_DB {
                self.lane_hot_until[lane] = self.total as f64 + self.fs as f64;
            }
        }
        events.push(RxEvent::Spectrum(row));
    }

    fn detect(&mut self, events: &mut Vec<RxEvent>) {
        for profile in Profile::ALL {
            let win_s = match profile {
                Profile::Fast => WINDOW_FAST_S,
                Profile::Slow => WINDOW_SLOW_S,
            };
            let win = ((win_s * self.fs as f64) as usize).min(self.buf.len());
            if win < self.fs as usize * 3 {
                continue;
            }
            let start = self.buf.len() - win;
            let slice = &self.buf[start..];
            let mut m = EnergyMatrix::new(profile);
            m.compute(slice);
            let sps = profile.samples_per_symbol(self.fs).unwrap() as f64;
            let window_abs = (self.base + start as u64) as f64;
            for mut det in fika_modem::sync::detect(&m, &self.rx.cfg) {
                // Refine timing on the slice, then make the start absolute.
                let mut tmp = det.clone();
                fika_modem::Demodulator::new(self.fs).refine_timing(slice, &mut tmp);
                det.start_sample = tmp.start_sample + window_abs;
                let same = |lane: usize, p: Profile, s: f64| {
                    lane == det.lane && p == det.profile && (s - det.start_sample).abs() < 2.0 * sps
                };
                if self
                    .tracked
                    .iter()
                    .any(|t| same(t.det.lane, t.det.profile, t.det.start_sample))
                    || self.recent.iter().any(|r| same(r.lane, r.profile, r.start))
                {
                    continue;
                }
                // The other profile's detector sees a burst as a smeared
                // pattern; drop candidates overlapping a tracked burst of
                // the other profile on the same lane.
                let preamble_end = det.start_sample + PREAMBLE_SYMBOLS as f64 * sps;
                let shadowed = self.tracked.iter().any(|t| {
                    !t.done
                        && t.det.lane == det.lane
                        && t.det.profile != det.profile
                        && det.start_sample < t.predicted_end(self.fs)
                        && preamble_end > t.det.start_sample
                });
                if shadowed {
                    continue;
                }
                // Ignore bursts whose first block already ended before the
                // window began; they were handled by an earlier pass or lost.
                let first_block_end =
                    det.start_sample + (PREAMBLE_SYMBOLS + det.kind.block_symbols()) as f64 * sps;
                if first_block_end < window_abs + sps * 4.0 {
                    continue;
                }
                events.push(RxEvent::Detected(det.clone()));
                self.tracked.push(Tracked {
                    det,
                    next_block: 0,
                    total: None,
                    asm: MessageAssembler::new(),
                    blocks_ok: 0,
                    done: false,
                });
            }
        }
    }

    fn decode_due(&mut self, events: &mut Vec<RxEvent>) {
        let fs = self.fs;
        let base = self.base;
        let total = self.total;
        let mut reservations: Vec<(usize, f64)> = Vec::new();
        let my_call = self.my_call;
        let tx_intervals = self.tx_intervals.clone();
        let overlaps_tx = |a: f64, b: f64| tx_intervals.iter().any(|&(s, e)| a < e && b > s);
        for t in self.tracked.iter_mut() {
            if t.done {
                continue;
            }
            let sps = t.sps(fs);
            let k = t.next_block;
            let block_start = t.det.start_sample
                + (PREAMBLE_SYMBOLS + k * t.det.kind.block_symbols()) as f64 * sps;
            let block_end = block_start + t.det.kind.block_symbols() as f64 * sps;
            if block_end + sps > total as f64 {
                continue;
            }
            if t.det.start_sample < base as f64 {
                t.done = true;
                continue;
            }
            if overlaps_tx(block_start, block_end) {
                t.done = true;
                events.push(RxEvent::Lost(t.det.clone()));
                continue;
            }
            let mut rel = t.det.clone();
            rel.start_sample -= base as f64;
            let result = self.rx.decode_block(&self.buf, &rel, k);
            t.next_block += 1;
            match (k, t.det.kind, result.bytes) {
                (0, FrameKind::Short, Some(bytes)) => {
                    t.done = true;
                    match Frame::parse(&bytes) {
                        Ok(Frame::Ack(ack)) => events.push(RxEvent::Ack {
                            det: t.det.clone(),
                            ack,
                        }),
                        Ok(Frame::Beacon(beacon)) => events.push(RxEvent::Beacon {
                            det: t.det.clone(),
                            beacon,
                        }),
                        _ => events.push(RxEvent::Failed(t.det.clone())),
                    }
                }
                (0, FrameKind::Long, Some(bytes)) => match Frame::parse(&bytes) {
                    Ok(Frame::Block0(b0)) => {
                        // SPEC §13: a direct message with ack_req to someone
                        // else reserves the lane for its ACK window.
                        if b0.ack_req && b0.dest != Destination::Call(my_call) {
                            let end = t.det.start_sample
                                + (PREAMBLE_SYMBOLS
                                    + b0.total as usize * t.det.kind.block_symbols())
                                    as f64
                                    * sps;
                            let ack_air =
                                (PREAMBLE_SYMBOLS + FrameKind::Short.block_symbols()) as f64 * sps;
                            reservations.push((
                                t.det.lane,
                                end + (2.0 + 1.0) * fs as f64 + ack_air + 2.0 * sps,
                            ));
                        }
                        let _ = t.asm.push(0, &bytes);
                        t.blocks_ok = 1;
                        t.total = t.asm.total();
                        if t.asm.is_complete() {
                            Self::finish(t, events);
                        }
                    }
                    _ => {
                        t.done = true;
                        events.push(RxEvent::Failed(t.det.clone()));
                    }
                },
                (0, _, None) => {
                    t.done = true;
                    events.push(RxEvent::Failed(t.det.clone()));
                }
                (k, FrameKind::Long, bytes) => {
                    if let Some(bytes) = bytes
                        && t.asm.push(k, &bytes).is_ok()
                    {
                        t.blocks_ok += 1;
                    }
                    let total = t.total.unwrap_or(1);
                    if t.next_block >= total {
                        Self::finish(t, events);
                    }
                }
                _ => t.done = true,
            }
        }
        for (lane, until) in reservations {
            self.lane_reserved_until[lane] = self.lane_reserved_until[lane].max(until);
        }
    }

    fn finish(t: &mut Tracked, events: &mut Vec<RxEvent>) {
        t.done = true;
        let total = t.total.unwrap_or(1);
        match t.asm.finish() {
            Ok(message) => events.push(RxEvent::Message {
                det: t.det.clone(),
                message,
                blocks_ok: t.blocks_ok,
                total,
            }),
            Err(_) => events.push(RxEvent::Failed(t.det.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_modem::{Burst, Transmitter};
    use fika_proto::callsign;

    fn burst_audio(text: &str, lane: usize) -> (Message, Vec<f32>) {
        let msg = Message {
            sender: callsign::pack("SM6WJM"),
            dest: Destination::All,
            msg_id: 77,
            ack_req: false,
            text: text.into(),
        };
        let burst = Burst::new(FrameKind::Long, 5, msg.to_blocks().unwrap()).unwrap();
        let audio = Transmitter::new(12_000)
            .render(&burst, lane, Profile::Fast, 0.0)
            .unwrap();
        (msg, audio)
    }

    #[test]
    fn decodes_once_and_only_once_fed_in_chunks() {
        let (msg, audio) = burst_audio("streaming test, hej hej", 2);
        let mut srx = StreamReceiver::new(SyncConfig::default());
        let mut got = Vec::new();
        let chunk = vec![0f32; 1200];
        // 3 s of silence, inject the burst, then keep feeding 30 s more.
        for i in 0..330 {
            if i == 30 {
                srx.inject(&audio);
            }
            srx.push(&chunk);
            for ev in srx.process() {
                if let RxEvent::Message { message, .. } = ev {
                    got.push(message);
                }
            }
        }
        assert_eq!(got.len(), 1, "decoded {} times", got.len());
        assert_eq!(got[0], msg);
    }

    #[test]
    fn lane_goes_busy_during_a_burst_and_burst_overlapping_tx_is_lost() {
        let (_, audio) = burst_audio("busy lane", 1);
        let mut srx = StreamReceiver::new(SyncConfig::default());
        let chunk = vec![0f32; 1200];
        let mut busy_seen = false;
        for i in 0..100 {
            if i == 20 {
                srx.inject(&audio);
            }
            srx.push(&chunk);
            let _ = srx.process();
            if i == 40 {
                let st = srx.lane_status();

                busy_seen = st[1].busy_until > srx.position() as f64;
                assert!(
                    st[0].busy_until <= srx.position() as f64,
                    "lane 0 should be idle"
                );
            }
        }
        assert!(busy_seen, "lane 1 busy while the burst is on");

        // Second burst, but we "transmit" over its block 0: reported lost.
        let (_, audio) = burst_audio("cut by tx", 3);
        let mut lost = false;
        let start = srx.position() as f64 + 12_000.0;
        srx.note_tx(start + 24_000.0, start + 36_000.0);
        for i in 0..120 {
            if i == 10 {
                srx.inject(&audio);
            }
            srx.push(&chunk);
            for ev in srx.process() {
                if let RxEvent::Lost(_) = ev {
                    lost = true;
                }
            }
        }
        assert!(lost);
    }
}
