//! Shared burst decoding: detection → blocks → frames.

use fika_modem::{Detection, FrameKind, Receiver};
use fika_proto::{Frame, Message, MessageAssembler};

#[derive(Debug)]
pub enum Decoded {
    Message {
        blocks_ok: usize,
        total: usize,
        message: Option<Message>,
    },
    Ack(fika_proto::Ack),
    Beacon(fika_proto::Beacon),
    /// Block 0 failed LDPC or CRC.
    Failed,
}

pub fn decode_burst(rx: &mut Receiver, samples: &[f32], det: &Detection) -> Decoded {
    let b0 = rx.decode_block(samples, det, 0);
    let Some(bytes) = b0.bytes else {
        return Decoded::Failed;
    };
    match det.kind {
        FrameKind::Short => match Frame::parse(&bytes) {
            Ok(Frame::Ack(a)) => Decoded::Ack(a),
            Ok(Frame::Beacon(b)) => Decoded::Beacon(b),
            _ => Decoded::Failed,
        },
        FrameKind::Long => {
            let mut asm = MessageAssembler::new();
            if asm.push(0, &bytes).is_err() {
                return Decoded::Failed;
            }
            let total = asm.total().unwrap_or(1);
            let mut blocks_ok = 1;
            for k in 1..total {
                let b = rx.decode_block(samples, det, k);
                if let Some(bytes) = b.bytes
                    && asm.push(k, &bytes).is_ok()
                {
                    blocks_ok += 1;
                }
            }
            let message = asm.is_complete().then(|| asm.finish().ok()).flatten();
            Decoded::Message {
                blocks_ok,
                total,
                message,
            }
        }
    }
}

pub fn describe_dest(d: fika_proto::Destination) -> String {
    match d {
        fika_proto::Destination::All => "all".into(),
        fika_proto::Destination::Group(g) => format!("@{g:07X}"),
        fika_proto::Destination::Call(c) => fika_proto::callsign::unpack(c)
            .map(|c| c.to_string())
            .unwrap_or("?".into()),
    }
}
