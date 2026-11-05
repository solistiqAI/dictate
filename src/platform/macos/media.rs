//! Pause whatever is playing (Spotify, Music, a browser tab, a podcast…) while recording, and resume
//! it afterwards. macOS keeps "Now Playing" (MediaRemote) to Apple-signed processes since 15.4, so
//! this goes through mediaremote-adapter: Apple's `/usr/bin/perl` loads the adapter framework that
//! `scripts/bundle.sh` puts into the app's Resources (vendor/mediaremote-adapter/DICTATE-VENDOR.md).
//! No permission prompt.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// MRCommand ids (vendor/mediaremote-adapter/src/private/MediaRemote.h).
const PLAY: &str = "0";
const PAUSE: &str = "1";

/// Whether we paused the player, so only we resume it.
static PAUSED_BY_US: AtomicBool = AtomicBool::new(false);

pub struct Adapter {
    script: PathBuf,
    framework: PathBuf,
}

impl Adapter {
    /// The adapter inside `Dictate.app/Contents/Resources`; `None` outside the app bundle (CLI).
    pub fn find() -> Option<Self> {
        let resources = std::env::current_exe()
            .and_then(|exe| exe.canonicalize())
            .ok()?
            .parent()?
            .parent()?
            .join("Resources");
        let adapter = Adapter {
            script: resources.join("mediaremote-adapter.pl"),
            framework: resources.join("MediaRemoteAdapter.framework"),
        };
        (adapter.script.exists() && adapter.framework.exists()).then_some(adapter)
    }

    /// Runs one adapter command; gives up after `timeout` (the adapter waits on MediaRemote).
    fn run(&self, args: &[&str], timeout: Duration) -> Option<String> {
        let mut child = Command::new("/usr/bin/perl")
            .arg(&self.script)
            .arg(&self.framework)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!("media: adapter {args:?} timed out");
                    return None;
                }
            }
        }
        let output = child.wait_with_output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Pauses the "Now Playing" app if it is playing.
    pub fn pause_if_playing(&self) {
        let Some(info) = self.run(&["get", "--no-artwork"], Duration::from_secs(2)) else {
            return;
        };
        if is_playing(&info) && self.run(&["send", PAUSE], Duration::from_secs(2)).is_some() {
            PAUSED_BY_US.store(true, Ordering::Relaxed);
        }
    }

    /// Resumes what [`pause_if_playing`](Self::pause_if_playing) paused; otherwise does nothing.
    pub fn resume(&self) {
        if PAUSED_BY_US.swap(false, Ordering::Relaxed) {
            let _ = self.run(&["send", PLAY], Duration::from_secs(2));
        }
    }
}

/// `get` prints `null` or an object with `"playing": true|false`.
fn is_playing(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json.trim())
        .ok()
        .and_then(|v| v.get("playing")?.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::is_playing;

    #[test]
    fn reads_the_playing_flag() {
        assert!(is_playing(
            r#"{"bundleIdentifier":"com.spotify.client","playing":true,"title":"x"}"#
        ));
        assert!(!is_playing(
            r#"{"bundleIdentifier":"com.google.Chrome","playing":false}"#
        ));
        assert!(!is_playing("null\n"));
        assert!(!is_playing(""));
    }
}
