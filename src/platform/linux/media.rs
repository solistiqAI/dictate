//! Pause whatever is playing while recording, and resume it afterwards, through MPRIS (the
//! D-Bus interface Linux players offer) with `playerctl`: no `playerctl`, no pausing.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether we paused the player, so only we resume it.
static PAUSED_BY_US: AtomicBool = AtomicBool::new(false);

pub struct Adapter;

impl Adapter {
    /// `playerctl` on the PATH.
    pub fn find() -> Option<Self> {
        playerctl(&["--version"]).map(|_| Adapter)
    }

    /// Pauses the active player if it is playing.
    pub fn pause_if_playing(&self) {
        if playerctl(&["status"]).is_some_and(|s| s.trim() == "Playing")
            && playerctl(&["pause"]).is_some()
        {
            PAUSED_BY_US.store(true, Ordering::Relaxed);
        }
    }

    /// Resumes what [`pause_if_playing`](Self::pause_if_playing) paused; otherwise does nothing.
    pub fn resume(&self) {
        if PAUSED_BY_US.swap(false, Ordering::Relaxed) {
            let _ = playerctl(&["play"]);
        }
    }
}

/// Runs `playerctl` with `args`; its output when it succeeded.
fn playerctl(args: &[&str]) -> Option<String> {
    let output = Command::new("playerctl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
