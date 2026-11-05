//! Pausing playback while recording: not on Windows yet (the system media transport controls,
//! `GlobalSystemMediaTransportControlsSessionManager`, would do it). `find` says there is none.

pub struct Adapter;

impl Adapter {
    pub fn find() -> Option<Self> {
        None
    }

    pub fn pause_if_playing(&self) {}

    pub fn resume(&self) {}
}
