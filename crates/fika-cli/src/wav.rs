use anyhow::{Context, Result, bail};
use std::path::Path;

use fika_modem::params::RX_SAMPLE_RATE;
use fika_modem::resample::decimate;

pub fn write(path: &Path, samples: &[f32], fs: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: fs,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .with_context(|| format!("create {}", path.display()))?;
    for &s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

/// Read a WAV, take the first channel, and bring it to the 12 kHz receiver rate.
pub fn read_for_rx(path: &Path) -> Result<Vec<f32>> {
    let mut r = hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let mono: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u32 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .step_by(ch)
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()?
        }
        hound::SampleFormat::Float => r.samples::<f32>().step_by(ch).collect::<Result<_, _>>()?,
    };
    let factor = spec.sample_rate / RX_SAMPLE_RATE;
    if factor == 0 || factor * RX_SAMPLE_RATE != spec.sample_rate {
        bail!(
            "sample rate {} Hz is not a multiple of {} Hz",
            spec.sample_rate,
            RX_SAMPLE_RATE
        );
    }
    Ok(decimate(&mono, factor as usize))
}
