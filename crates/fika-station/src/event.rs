use fika_modem::Detection;
use fika_proto::{Ack, Beacon, Message};

/// One line in the chat view.
#[derive(Clone, Debug)]
pub struct ChatLine {
    pub epoch: f64,
    pub from: String,
    pub to: String,
    pub text: String,
    pub snr_db: Option<f32>,
    pub mine: bool,
    /// For own direct messages: "sent", "delivered", "no ack".
    pub status: Option<String>,
    pub msg_id: u16,
}

#[derive(Clone, Debug)]
pub enum StationEvent {
    Log(String),
    /// Input level of the last chunk: RMS in dBFS and peak 0..1.
    Level {
        rms_db: f32,
        peak: f32,
    },
    /// Passband spectrum in dB relative to baseline, 300..2700 Hz.
    Spectrum(Vec<f32>),
    Detected {
        det: Detection,
        epoch: f64,
    },
    Message {
        det: Detection,
        message: Message,
        blocks_ok: usize,
        total: usize,
        epoch: f64,
    },
    Ack {
        det: Detection,
        ack: Ack,
        epoch: f64,
    },
    Beacon {
        det: Detection,
        beacon: Beacon,
        epoch: f64,
    },
    BurstFailed {
        det: Detection,
    },
    TxStarted {
        label: String,
        airtime_s: f64,
    },
    TxFinished,
    Rig {
        connected: bool,
        freq_hz: Option<u64>,
        ptt: bool,
    },
}
