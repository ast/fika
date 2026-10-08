//! cpal input and output. Input chunks go to an mpsc channel; output is a
//! lock-free ring the transmit thread fills and the callback drains.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, Sample, SampleFormat, SampleRate, SizedSample, StreamConfig};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};

use crate::config::AudioCfg;

pub type OutProducer = ringbuf::HeapProd<f32>;

/// Live audio streams. Must stay on the thread that created it.
pub struct AudioEngine {
    _input: Option<cpal::Stream>,
    _output: Option<cpal::Stream>,
    #[cfg(feature = "pipewire")]
    _pw: Option<crate::audio_pw::PwEngine>,
    pub rate: u32,
    pub input_name: String,
    pub output_name: String,
    /// Samples the output callback has actually played from the ring.
    pub played: Arc<AtomicU64>,
}

/// Everything the transmit thread needs to play audio.
pub struct OutputHandle {
    pub producer: Option<OutProducer>,
    pub played: Arc<AtomicU64>,
    pub rate: u32,
    /// While set, the output callback discards the ring instead of playing it.
    pub mute: Arc<AtomicBool>,
}

pub fn list_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            out.push(format!("input:  {}", d.name().unwrap_or_default()));
        }
    }
    if let Ok(devs) = host.output_devices() {
        for d in devs {
            out.push(format!("output: {}", d.name().unwrap_or_default()));
        }
    }
    out
}

fn find_device(host: &cpal::Host, spec: &str, input: bool) -> Result<Option<Device>> {
    if spec.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if spec.eq_ignore_ascii_case("default") {
        let d = if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        };
        return d.map(Some).context("no default audio device");
    }
    let devs: Vec<Device> = if input {
        host.input_devices()?.collect()
    } else {
        host.output_devices()?.collect()
    };
    let needle = spec.to_lowercase();
    for d in devs {
        if d.name()
            .map(|n| n.to_lowercase().contains(&needle))
            .unwrap_or(false)
        {
            return Ok(Some(d));
        }
    }
    bail!(
        "no {} device matching '{spec}'",
        if input { "input" } else { "output" }
    )
}

fn pick_format(device: &Device, input: bool, rate: u32) -> Result<(SampleFormat, u16)> {
    let mut best: Option<(SampleFormat, u16)> = None;
    let mut consider = |fmt: SampleFormat, ch: u16| {
        let rank = |f: SampleFormat| match f {
            SampleFormat::F32 => 0,
            SampleFormat::I16 => 1,
            SampleFormat::I32 => 2,
            _ => 9,
        };
        if best.is_none_or(|(bf, bc)| (rank(fmt), ch) < (rank(bf), bc)) {
            best = Some((fmt, ch));
        }
    };
    if input {
        for c in device.supported_input_configs()? {
            if c.min_sample_rate().0 <= rate && rate <= c.max_sample_rate().0 {
                consider(c.sample_format(), c.channels());
            }
        }
    } else {
        for c in device.supported_output_configs()? {
            if c.min_sample_rate().0 <= rate && rate <= c.max_sample_rate().0 {
                consider(c.sample_format(), c.channels());
            }
        }
    }
    best.with_context(|| format!("device does not support {rate} Hz"))
}

fn build_input<T>(
    device: &Device,
    config: &StreamConfig,
    tx: Sender<Vec<f32>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|fr| f32::from_sample(fr[0]))
                .collect();
            let _ = tx.send(mono);
        },
        |e| eprintln!("audio input error: {e}"),
        None,
    )?;
    Ok(stream)
}

fn build_output<T>(
    device: &Device,
    config: &StreamConfig,
    mut consumer: ringbuf::HeapCons<f32>,
    played: Arc<AtomicU64>,
    mute: Arc<AtomicBool>,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let mut n = 0u64;
            if mute.load(Ordering::Relaxed) {
                // Abort: throw away everything queued, output silence.
                while consumer.try_pop().is_some() {
                    n += 1;
                }
            }
            for frame in data.chunks_mut(channels) {
                let v = match consumer.try_pop() {
                    Some(v) => {
                        n += 1;
                        v
                    }
                    None => 0.0,
                };
                for s in frame.iter_mut() {
                    *s = T::from_sample(v);
                }
            }
            played.fetch_add(n, Ordering::Relaxed);
        },
        |e| eprintln!("audio output error: {e}"),
        None,
    )?;
    Ok(stream)
}

impl AudioEngine {
    /// Open the configured devices. Input chunks (mono, device rate) are sent
    /// on `input_tx`. Returns the engine and the output handle for the
    /// transmit thread.
    pub fn open(cfg: &AudioCfg, input_tx: Sender<Vec<f32>>) -> Result<(Self, OutputHandle)> {
        Self::open_named(cfg, input_tx, "fika")
    }

    /// As `open`, with the PipeWire node name prefix for this station.
    pub fn open_named(
        cfg: &AudioCfg,
        input_tx: Sender<Vec<f32>>,
        node_name: &str,
    ) -> Result<(Self, OutputHandle)> {
        let rate = cfg.sample_rate;
        let played = Arc::new(AtomicU64::new(0));
        let mute = Arc::new(AtomicBool::new(false));

        if cfg.backend == "pipewire" {
            #[cfg(feature = "pipewire")]
            {
                let (producer, consumer) = if cfg.output.eq_ignore_ascii_case("none") {
                    (None, None)
                } else {
                    let (p, c) = HeapRb::<f32>::new(rate as usize * 4).split();
                    (Some(p), Some(c))
                };
                let pw = crate::audio_pw::PwEngine::open(
                    rate,
                    &cfg.input,
                    &cfg.output,
                    node_name,
                    input_tx,
                    consumer,
                    played.clone(),
                    mute.clone(),
                )?;
                return Ok((
                    Self {
                        _input: None,
                        _output: None,
                        _pw: Some(pw),
                        rate,
                        input_name: format!("pipewire:{}", cfg.input),
                        output_name: format!("pipewire:{}", cfg.output),
                        played: played.clone(),
                    },
                    OutputHandle {
                        producer,
                        played,
                        rate,
                        mute,
                    },
                ));
            }
            #[cfg(not(feature = "pipewire"))]
            bail!("built without the pipewire feature; use audio.backend = \"alsa\"");
        }
        let _ = node_name;
        let host = cpal::default_host();

        let mut input_name = "none".to_string();
        let input = match find_device(&host, &cfg.input, true)? {
            None => None,
            Some(dev) => {
                input_name = dev.name().unwrap_or_default();
                let (fmt, channels) = pick_format(&dev, true, rate)?;
                let config = StreamConfig {
                    channels,
                    sample_rate: SampleRate(rate),
                    buffer_size: cpal::BufferSize::Default,
                };
                let stream = match fmt {
                    SampleFormat::F32 => build_input::<f32>(&dev, &config, input_tx)?,
                    SampleFormat::I16 => build_input::<i16>(&dev, &config, input_tx)?,
                    SampleFormat::I32 => build_input::<i32>(&dev, &config, input_tx)?,
                    other => bail!("unsupported input sample format {other:?}"),
                };
                stream.play()?;
                Some(stream)
            }
        };

        let mut output_name = "none".to_string();
        let mut producer = None;
        let output = match find_device(&host, &cfg.output, false)? {
            None => None,
            Some(dev) => {
                output_name = dev.name().unwrap_or_default();
                let (fmt, channels) = pick_format(&dev, false, rate)?;
                let config = StreamConfig {
                    channels,
                    sample_rate: SampleRate(rate),
                    buffer_size: cpal::BufferSize::Default,
                };
                let rb = HeapRb::<f32>::new(rate as usize * 4);
                let (prod, cons) = rb.split();
                producer = Some(prod);
                let stream = match fmt {
                    SampleFormat::F32 => {
                        build_output::<f32>(&dev, &config, cons, played.clone(), mute.clone())?
                    }
                    SampleFormat::I16 => {
                        build_output::<i16>(&dev, &config, cons, played.clone(), mute.clone())?
                    }
                    SampleFormat::I32 => {
                        build_output::<i32>(&dev, &config, cons, played.clone(), mute.clone())?
                    }
                    other => bail!("unsupported output sample format {other:?}"),
                };
                stream.play()?;
                Some(stream)
            }
        };

        Ok((
            Self {
                _input: input,
                _output: output,
                #[cfg(feature = "pipewire")]
                _pw: None,
                rate,
                input_name,
                output_name,
                played: played.clone(),
            },
            OutputHandle {
                producer,
                played,
                rate,
                mute,
            },
        ))
    }
}

impl OutputHandle {
    /// Blocking: push all samples into the ring, then wait until played.
    /// Returns false if `abort` was raised; the ring is then drained and
    /// silence follows within one callback period.
    pub fn play_blocking(&mut self, samples: &[f32], abort: &AtomicBool) -> bool {
        let tick = std::time::Duration::from_millis(20);
        let Some(prod) = self.producer.as_mut() else {
            // No output device: pretend to play in real time.
            let end = std::time::Instant::now()
                + std::time::Duration::from_secs_f64(samples.len() as f64 / self.rate as f64);
            while std::time::Instant::now() < end {
                if abort.load(Ordering::Relaxed) {
                    return false;
                }
                std::thread::sleep(tick);
            }
            return true;
        };
        let target = self.played.load(Ordering::Relaxed) + samples.len() as u64;
        let mut i = 0;
        let mut ok = true;
        while i < samples.len() {
            if abort.load(Ordering::Relaxed) {
                ok = false;
                break;
            }
            i += prod.push_slice(&samples[i..]);
            if i < samples.len() {
                std::thread::sleep(tick);
            }
        }
        while ok && self.played.load(Ordering::Relaxed) < target {
            if abort.load(Ordering::Relaxed) {
                ok = false;
                break;
            }
            std::thread::sleep(tick);
        }
        if !ok {
            self.mute.store(true, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(60));
            self.mute.store(false, Ordering::Relaxed);
        }
        ok
    }
}
