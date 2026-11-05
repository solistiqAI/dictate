//! PocketTTS runs in the macOS Swift helper; Linux has none (the system voice, `espeak-ng`,
//! reads instead). `cached` is false, so callers never get here except through `speak`, which
//! says so.

use std::sync::Arc;

use anyhow::{Result, bail};

pub struct Pocket;

impl Pocket {
    pub fn new() -> Arc<Self> {
        Arc::new(Pocket)
    }

    pub fn cached() -> bool {
        false
    }

    pub fn fetch(self: &Arc<Self>) {}

    pub fn speak(
        self: &Arc<Self>,
        _text: &str,
        _voice: &str,
        _speed: f32,
        _on_done: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        bail!("PocketTTS gibt es unter Linux nicht")
    }

    pub fn stop(&self) {}
}
