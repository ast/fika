//! Streaming receiver: a rolling 12 kHz buffer with periodic preamble
//! detection and block decoding as audio arrives. Also produces the input
//! level and a passband spectrum for the UI.

use std::collections::VecDeque;

use fika_modem::energy::EnergyMatrix;
use fika_modem::params::{PREAMBLE_SYMBOLS, RX_SAMPLE_RATE};
use fika_modem::{Detection, FrameKind, Profile, Receiver, SyncConfig};
use fika_proto::{Frame, Message, MessageAssembler};

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
    Level {
        rms_db: f32,
        peak: f32,
    },
    Spectrum(Vec<f32>),
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

pub struct StreamReceiver {
    fs: u32,
    buf: Vec<f32>,
    /// Absolute index of `buf[0]`.
    base: u64,
    /// Absolute number of samples received so far.
    total: u64,
    rx: Receiver,
    tracked: Vec<Tracked>,
    next_detect_at: u64,
    next_spectrum_at: u64,
    inject: VecDeque<f32>,
    spectrum_fft: EnergyMatrix,
    pub keep_s: f64,
}

const DETECT_EVERY_S: f64 = 1.0;
const SPECTRUM_EVERY_S: f64 = 0.25;
const WINDOW_FAST_S: f64 = 20.0;
const WINDOW_SLOW_S: f64 = 60.0;

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
            next_detect_at: (RX_SAMPLE_RATE as f64 * 2.0) as u64,
            next_spectrum_at: 0,
            inject: VecDeque::new(),
            spectrum_fft: EnergyMatrix::new(Profile::Fast),
            keep_s: 240.0,
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
        self.tracked.retain(|t| !t.done);
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
        // One frame of the fast energy matrix over the last symbol period.
        let w = self.spectrum_fft.window;
        let slice = &self.buf[self.buf.len() - w..];
        self.spectrum_fft.compute(slice);
        let bin_hz = self.spectrum_fft.bin_hz();
        let lo = (300.0 / bin_hz) as usize;
        let hi = (2700.0 / bin_hz) as usize;
        let row: Vec<f32> = (lo..hi)
            .map(|b| 10.0 * self.spectrum_fft.raw_at(0, b).max(1e-12).log10())
            .collect();
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
            for mut det in fika_modem::sync::detect(&m, &self.rx.cfg) {
                // Refine timing on the slice, then make the start absolute.
                let mut tmp = det.clone();
                fika_modem::Demodulator::new(self.fs).refine_timing(slice, &mut tmp);
                det.start_sample = tmp.start_sample + (self.base + start as u64) as f64;
                let known = self.tracked.iter().any(|t| {
                    t.det.lane == det.lane
                        && t.det.profile == det.profile
                        && (t.det.start_sample - det.start_sample).abs() < 2.0 * sps
                });
                if known {
                    continue;
                }
                // Ignore bursts whose first block already ended before the
                // window began; they were handled by an earlier pass or lost.
                let first_block_end =
                    det.start_sample + (PREAMBLE_SYMBOLS + det.kind.block_symbols()) as f64 * sps;
                if first_block_end < (self.base + start as u64) as f64 + sps * 4.0 {
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
        let buf = &self.buf;
        for t in self.tracked.iter_mut() {
            if t.done {
                continue;
            }
            let sps = t.det.profile.samples_per_symbol(fs).unwrap() as f64;
            let k = t.next_block;
            let block_end = t.det.start_sample
                + (PREAMBLE_SYMBOLS + (k + 1) * t.det.kind.block_symbols()) as f64 * sps;
            if block_end + sps > total as f64 {
                continue;
            }
            if t.det.start_sample < base as f64 {
                t.done = true;
                continue;
            }
            let mut rel = t.det.clone();
            rel.start_sample -= base as f64;
            let result = self.rx.decode_block(buf, &rel, k);
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
                (0, FrameKind::Long, Some(bytes)) => match t.asm.push(0, &bytes) {
                    Ok(()) => {
                        t.blocks_ok = 1;
                        t.total = t.asm.total();
                        if t.asm.is_complete() {
                            Self::finish(t, events);
                        }
                    }
                    Err(_) => {
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
    use fika_proto::{Destination, callsign};

    #[test]
    fn decodes_a_burst_fed_in_chunks_with_injection() {
        let msg = Message {
            sender: callsign::pack("SM6WJM"),
            dest: Destination::All,
            msg_id: 77,
            ack_req: false,
            text: "streaming test, hej hej".into(),
        };
        let burst = Burst::new(FrameKind::Long, 5, msg.to_blocks().unwrap()).unwrap();
        let audio = Transmitter::new(12_000)
            .render(&burst, 2, Profile::Fast, 0.0)
            .unwrap();
        let mut srx = StreamReceiver::new(SyncConfig::default());
        let mut got = None;
        // 3 s of silence, then inject the burst while feeding silence chunks.
        let chunk = vec![0f32; 1200];
        for i in 0..200 {
            if i == 30 {
                srx.inject(&audio);
            }
            srx.push(&chunk);
            for ev in srx.process() {
                if let RxEvent::Message { message, .. } = ev {
                    got = Some(message);
                }
            }
        }
        assert_eq!(got.expect("message decoded"), msg);
    }
}
