//! Audio as the models take it: 16 kHz mono f32, read from WAV files and resampled. The
//! microphone is the app's (`audio.rs` in the `dictate` crate, cpal).

use std::path::Path;

use anyhow::{Context, Result};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

pub const MODEL_RATE: u32 = 16_000;

/// Mono audio at some sample rate.
pub struct Clip {
    pub samples: Vec<f32>,
    pub rate: u32,
}

impl Clip {
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / self.rate as f32
    }

    /// Loudest sample, 0.0 for digital silence (what macOS delivers without mic permission).
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
    }

    /// Loudness in dBFS (RMS), -inf for silence.
    pub fn rms_db(&self) -> f32 {
        if self.samples.is_empty() {
            return f32::NEG_INFINITY;
        }
        let mean = self.samples.iter().map(|s| s * s).sum::<f32>() / self.samples.len() as f32;
        10.0 * mean.log10()
    }

    pub fn write_wav(&self, path: &Path) -> Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for s in &self.samples {
            writer.write_sample(*s)?;
        }
        writer.finalize()?;
        Ok(())
    }

    /// Resampled to [`MODEL_RATE`].
    pub fn into_model_rate(self) -> Result<Vec<f32>> {
        resample(self.samples, self.rate, MODEL_RATE)
    }
}

/// Reads a WAV or (by its extension) FLAC file, mixed down to mono.
pub fn read(path: &Path) -> Result<Clip> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("flac") => read_flac(path),
        _ => read_wav(path),
    }
}

/// Reads a FLAC file (the eval clips) and mixes it down to mono.
pub fn read_flac(path: &Path) -> Result<Clip> {
    let mut reader =
        claxon::FlacReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    let scale = 1.0 / (1u64 << (info.bits_per_sample - 1)) as f32;
    let interleaved: Vec<f32> = reader
        .samples()
        .map(|s| s.map(|s| s as f32 * scale))
        .collect::<Result<_, _>>()
        .with_context(|| format!("decode {}", path.display()))?;
    Ok(Clip {
        samples: mono(&interleaved, channels),
        rate: info.sample_rate,
    })
}

fn mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Reads a WAV file and mixes it down to mono.
pub fn read_wav(path: &Path) -> Result<Clip> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("open {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|s| s as f32 * scale))
                .collect::<Result<_, _>>()?
        }
    };
    Ok(Clip {
        samples: mono(&interleaved, channels),
        rate: spec.sample_rate,
    })
}

/// Band-limited resampling of a whole mono clip.
pub fn resample(samples: Vec<f32>, from: u32, to: u32) -> Result<Vec<f32>> {
    if from == to || samples.is_empty() {
        return Ok(samples);
    }
    let frames = samples.len();
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, 1024, 1, FixedSync::Input)
        .context("create resampler")?;
    let input = InterleavedSlice::new(&samples, 1, frames).context("resampler input")?;
    let output = resampler
        .process_all(&input, frames, None)
        .context("resample")?;
    Ok(output.take_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_48k_to_16k_and_keeps_a_tone() {
        let tone: Vec<f32> = (0..48_000)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin())
            .collect();
        let out = resample(tone, 48_000, 16_000).unwrap();
        assert!((out.len() as i64 - 16_000).abs() <= 2, "len {}", out.len());
        let rms = (out[1000..15_000].iter().map(|s| s * s).sum::<f32>() / 14_000.0).sqrt();
        assert!(
            (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02,
            "rms {rms}"
        );
    }

    #[test]
    fn levels() {
        let silent = Clip {
            samples: vec![0.0; 100],
            rate: 16_000,
        };
        assert_eq!(silent.peak(), 0.0);
        assert_eq!(silent.rms_db(), f32::NEG_INFINITY);
        let full = Clip {
            samples: vec![1.0, -1.0],
            rate: 16_000,
        };
        assert_eq!(full.peak(), 1.0);
        assert!(full.rms_db().abs() < 1e-6);
    }

    #[test]
    fn same_rate_is_untouched() {
        assert_eq!(
            resample(vec![0.1, 0.2], 16_000, 16_000).unwrap(),
            vec![0.1, 0.2]
        );
    }
}
