//! Microphone capture; WAV files and resampling to the 16 kHz mono f32 that Parakeet expects are
//! `dictate-core`'s (`core/src/audio.rs`).

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

pub use dictate_core::audio::{Clip, MODEL_RATE, read_wav, resample};

/// The microphone chosen in the settings (its name); empty = the Mac's default input.
static CHOSEN: Mutex<String> = Mutex::new(String::new());

/// Records from this microphone from now on (settings loaded or changed).
pub fn choose(name: &str) {
    *CHOSEN.lock().unwrap() = name.trim().to_string();
}

fn name_of(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "?".into())
}

/// Names of the microphones connected now, the default first.
pub fn input_names() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = host.default_input_device().iter().map(name_of).collect();
    if let Ok(devices) = host.input_devices() {
        for name in devices.map(|d| name_of(&d)) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// The chosen microphone, or the default one when none is chosen or it is not connected
/// (then `false`: the chosen one is missing).
fn device() -> Result<(cpal::Device, bool)> {
    device_named(&CHOSEN.lock().unwrap().clone())
}

/// The microphone called `chosen`, else the default one (`false`: `chosen` is missing).
fn device_named(chosen: &str) -> Result<(cpal::Device, bool)> {
    let host = cpal::default_host();
    if !chosen.is_empty()
        && let Ok(mut devices) = host.input_devices()
        && let Some(device) = devices.find(|d| name_of(d) == chosen)
    {
        return Ok((device, true));
    }
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no microphone found"))?;
    Ok((device, chosen.is_empty()))
}

/// Records from the chosen (else the default) input device until [`Recorder::stop`].
/// The stream lives on the thread that created it.
pub struct Recorder {
    stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

/// Name and format of the microphone we would record from.
pub fn describe_input() -> Result<String> {
    let (device, found) = device()?;
    let config = device.default_input_config().context("microphone config")?;
    let missing = match found {
        true => String::new(),
        false => {
            let chosen = CHOSEN.lock().unwrap();
            crate::text::pick(
                format!(", {chosen} ist nicht da"),
                format!(", {chosen} is not connected"),
            )
        }
    };
    Ok(format!(
        "{} ({} Hz, {} ch, {:?}{missing})",
        name_of(&device),
        config.sample_rate(),
        config.channels(),
        config.sample_format()
    ))
}

impl Recorder {
    pub fn start() -> Result<Self> {
        let (device, _) = device()?;
        Self::start_on(device)
    }

    /// From the microphone called `name` ("" or not connected: the default one), apart from the
    /// one chosen for dictating (the Souffleur has its own). Returns its name too.
    pub fn start_named(name: &str) -> Result<(Self, String)> {
        let (device, _) = device_named(name.trim())?;
        let name = name_of(&device);
        Ok((Self::start_on(device)?, name))
    }

    /// What the default output device plays (WASAPI loopback: cpal records an output device
    /// through an input stream), for the Souffleur's "Andere" on Windows.
    #[cfg(windows)]
    pub fn start_loopback() -> Result<Self> {
        let device = cpal::default_host()
            .default_output_device()
            .context("no output device")?;
        let config = device
            .default_output_config()
            .context("output device config")?;
        Self::start_with(device, config)
    }

    /// The rate the samples come in.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    fn start_on(device: cpal::Device) -> Result<Self> {
        let config = device.default_input_config().context("microphone config")?;
        Self::start_with(device, config)
    }

    fn start_with(device: cpal::Device, config: cpal::SupportedStreamConfig) -> Result<Self> {
        let rate = config.sample_rate();
        let channels = config.channels() as usize;
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));
        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config.into(), channels, buffer.clone()),
            SampleFormat::I16 => build::<i16>(&device, config.into(), channels, buffer.clone()),
            SampleFormat::I32 => build::<i32>(&device, config.into(), channels, buffer.clone()),
            SampleFormat::U16 => build::<u16>(&device, config.into(), channels, buffer.clone()),
            other => Err(anyhow!("unsupported microphone sample format {other:?}")),
        }?;
        stream.play().context("start microphone")?;
        Ok(Recorder {
            stream,
            buffer,
            rate,
        })
    }

    /// Everything recorded so far, without stopping.
    pub fn snapshot(&self) -> Clip {
        Clip {
            samples: self.buffer.lock().unwrap().clone(),
            rate: self.rate,
        }
    }

    /// Everything recorded since the last call, taken out of the buffer (a long recording that
    /// is handled piece by piece).
    pub fn take(&self) -> Clip {
        Clip {
            samples: std::mem::take(&mut *self.buffer.lock().unwrap()),
            rate: self.rate,
        }
    }

    /// Loudness of the last `millis` in dBFS, for the level meter.
    pub fn recent_db(&self, millis: u32) -> f32 {
        let buffer = self.buffer.lock().unwrap();
        let n = (self.rate * millis / 1000) as usize;
        let tail = &buffer[buffer.len().saturating_sub(n)..];
        Clip {
            samples: tail.to_vec(),
            rate: self.rate,
        }
        .rms_db()
    }

    pub fn stop(self) -> Clip {
        drop(self.stream);
        let samples = std::mem::take(&mut *self.buffer.lock().unwrap());
        Clip {
            samples,
            rate: self.rate,
        }
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    buffer: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut buffer = buffer.lock().unwrap();
            for frame in data.chunks(channels) {
                let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                buffer.push(sum / channels as f32);
            }
        },
        |err| {
            eprintln!("microphone: {err}");
            crate::stats::problem(&format!("microphone: {err}"));
        },
        None,
    )?;
    Ok(stream)
}
