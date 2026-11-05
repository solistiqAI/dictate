//! The Souffleur's "Andere" on Windows (beta, compile-checked only): what the PC plays, recorded
//! as WASAPI loopback of the default output device (cpal opens an input stream on an output device
//! as loopback). No permission is asked. Same interface as the macOS tap: `start`, `take` (mono
//! f32 at `rate`), `ended`.

use anyhow::Result;

use crate::audio::Recorder;

pub struct SystemAudio {
    recorder: Recorder,
    pub rate: u32,
}

impl SystemAudio {
    pub fn start() -> Result<Self> {
        let recorder = Recorder::start_loopback()?;
        let rate = recorder.rate();
        Ok(SystemAudio { recorder, rate })
    }

    pub fn take(&self) -> Vec<f32> {
        self.recorder.take().samples
    }

    /// The stream reports errors to the log; it does not end by itself.
    pub fn ended(&mut self) -> bool {
        false
    }
}
