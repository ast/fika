//! Native PipeWire backend: a capture stream and a playback stream that
//! target nodes by name. Naming a sink as the capture target captures its
//! monitor, which is how the live channel's virtual "ether" sink works.
//! Runs its own main loop on a dedicated thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;

use anyhow::{Context, Result, anyhow};
use pipewire as pw;
use pw::spa::param::audio::{AudioFormat, AudioInfoRaw};
use pw::spa::pod::Pod;
use ringbuf::traits::Consumer;

pub struct PwEngine {
    _thread: thread::JoinHandle<()>,
}

struct Playback {
    consumer: ringbuf::HeapCons<f32>,
    played: Arc<AtomicU64>,
    mute: Arc<AtomicBool>,
    rate: u32,
}

fn format_pod(rate: u32) -> Vec<u8> {
    let mut info = AudioInfoRaw::new();
    info.set_format(AudioFormat::F32LE);
    info.set_rate(rate);
    info.set_channels(1);
    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .expect("serialize format")
    .0
    .into_inner()
}

fn target_props(
    category: &str,
    target: &str,
    node_name: &str,
    capture_sink: bool,
) -> pw::properties::Properties {
    let mut props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => category,
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::APP_NAME => "fika",
        *pw::keys::NODE_NAME => node_name,
        *pw::keys::AUDIO_CHANNELS => "1",
        "node.latency" => "480/12000",
    };
    if !target.eq_ignore_ascii_case("default") {
        props.insert("target.object", target);
    }
    if capture_sink {
        props.insert("stream.capture.sink", "true");
    }
    props
}

impl PwEngine {
    /// Open streams. `input`/`output` are node names, "default" or "none".
    /// Capture chunks (mono f32 at `rate`) go to `input_tx`; playback drains
    /// `consumer`, counting into `played` and discarding while `mute`.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        rate: u32,
        input: &str,
        output: &str,
        node_name: &str,
        input_tx: Sender<Vec<f32>>,
        consumer: Option<ringbuf::HeapCons<f32>>,
        played: Arc<AtomicU64>,
        mute: Arc<AtomicBool>,
    ) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let input = input.to_string();
        let output = output.to_string();
        let node_name = node_name.to_string();
        let thread = thread::Builder::new()
            .name("fika-pw".into())
            .spawn(move || {
                let result =
                    (|| -> Result<(pw::main_loop::MainLoop, Vec<Box<dyn std::any::Any>>)> {
                        pw::init();
                        let mainloop =
                            pw::main_loop::MainLoop::new(None).context("pipewire main loop")?;
                        let context =
                            pw::context::Context::new(&mainloop).context("pipewire context")?;
                        let core = context
                            .connect(None)
                            .context("connect to PipeWire (is it running?)")?;
                        let mut keep: Vec<Box<dyn std::any::Any>> = Vec::new();
                        let fmt = format_pod(rate);

                        if !input.eq_ignore_ascii_case("none") {
                            let props =
                                target_props("Capture", &input, &format!("{node_name}-rx"), true);
                            let stream = pw::stream::Stream::new(&core, "fika capture", props)?;
                            let tx = input_tx.clone();
                            let listener = stream
                                .add_local_listener_with_user_data(tx)
                                .process(|stream, tx| {
                                    if let Some(mut buffer) = stream.dequeue_buffer() {
                                        let datas = buffer.datas_mut();
                                        if let Some(d) = datas.first_mut() {
                                            let chunk = d.chunk();
                                            let (off, size) =
                                                (chunk.offset() as usize, chunk.size() as usize);
                                            if let Some(bytes) = d.data() {
                                                let end = (off + size).min(bytes.len());
                                                let samples: Vec<f32> = bytes[off..end]
                                                    .chunks_exact(4)
                                                    .map(|c| {
                                                        f32::from_le_bytes([c[0], c[1], c[2], c[3]])
                                                    })
                                                    .collect();
                                                let _ = tx.send(samples);
                                            }
                                        }
                                    }
                                })
                                .register()?;
                            let mut params = [Pod::from_bytes(&fmt).unwrap()];
                            stream.connect(
                                pw::spa::utils::Direction::Input,
                                None,
                                pw::stream::StreamFlags::AUTOCONNECT
                                    | pw::stream::StreamFlags::MAP_BUFFERS
                                    | pw::stream::StreamFlags::RT_PROCESS,
                                &mut params,
                            )?;
                            keep.push(Box::new(listener));
                            keep.push(Box::new(stream));
                        }

                        if let Some(consumer) = consumer
                            && !output.eq_ignore_ascii_case("none")
                        {
                            let props = target_props(
                                "Playback",
                                &output,
                                &format!("{node_name}-tx"),
                                false,
                            );
                            let stream = pw::stream::Stream::new(&core, "fika playback", props)?;
                            let pb = Playback {
                                consumer,
                                played: played.clone(),
                                mute: mute.clone(),
                                rate,
                            };
                            let listener = stream
                                .add_local_listener_with_user_data(pb)
                                .process(|stream, pb| {
                                    if let Some(mut buffer) = stream.dequeue_buffer() {
                                        // pipewire-rs 0.8 has no `requested()`; provide
                                        // one 40 ms quantum per callback, which the
                                        // adapter buffers as needed.
                                        let requested = (pb.rate / 25) as usize;
                                        let datas = buffer.datas_mut();
                                        let Some(d) = datas.first_mut() else { return };
                                        let mut n = 0u64;
                                        if pb.mute.load(Ordering::Relaxed) {
                                            while pb.consumer.try_pop().is_some() {
                                                n += 1;
                                            }
                                        }
                                        let frames = if let Some(bytes) = d.data() {
                                            let cap = bytes.len() / 4;
                                            let frames = if requested > 0 {
                                                requested.min(cap)
                                            } else {
                                                cap.min(480)
                                            };
                                            for i in 0..frames {
                                                let v = match pb.consumer.try_pop() {
                                                    Some(v) => {
                                                        n += 1;
                                                        v
                                                    }
                                                    None => 0.0,
                                                };
                                                bytes[i * 4..i * 4 + 4]
                                                    .copy_from_slice(&v.to_le_bytes());
                                            }
                                            frames
                                        } else {
                                            0
                                        };
                                        let chunk = d.chunk_mut();
                                        *chunk.offset_mut() = 0;
                                        *chunk.stride_mut() = 4;
                                        *chunk.size_mut() = (frames * 4) as u32;
                                        pb.played.fetch_add(n, Ordering::Relaxed);
                                    }
                                })
                                .register()?;
                            let mut params = [Pod::from_bytes(&fmt).unwrap()];
                            stream.connect(
                                pw::spa::utils::Direction::Output,
                                None,
                                pw::stream::StreamFlags::AUTOCONNECT
                                    | pw::stream::StreamFlags::MAP_BUFFERS
                                    | pw::stream::StreamFlags::RT_PROCESS,
                                &mut params,
                            )?;
                            keep.push(Box::new(listener));
                            keep.push(Box::new(stream));
                        }
                        keep.push(Box::new(context));
                        keep.push(Box::new(core));
                        Ok((mainloop, keep))
                    })();
                match result {
                    Ok((mainloop, _keep)) => {
                        let _ = ready_tx.send(Ok(()));
                        mainloop.run();
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })?;
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| anyhow!("PipeWire backend did not start"))??;
        Ok(Self { _thread: thread })
    }
}
