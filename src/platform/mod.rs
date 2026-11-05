//! The platform layer: everything the app needs from the operating system, behind one set of
//! names. `macos/` is the app as it has always run; `windows/` is the Windows beta (never run on
//! a real PC yet, see its module docs and README.md → "Windows (beta)"); `linux/` the Linux beta
//! (never run on a real desktop yet, docs/linux.md).
//!
//! All three provide, as `crate::platform::…`:
//!
//! - **Triggers:** `install_tap(triggers, sender)` (keep the returned value alive; dropping it
//!   stops watching), `start_capture` / `stop_capture` (the settings' "Aufnehmen"), the
//!   [`RECORDING`] / [`VOICE`] flags the tap reads for Esc, and [`TapEvent`] it sends.
//! - **Text in other apps:** `focused_accepts_text`, `paste`, `selected_text`, `Field` (learning
//!   from corrections; `Field::focused()` is `None` where the platform cannot read fields).
//! - **Permissions:** `is_trusted`, `ensure_accessibility`, `microphone_permission`,
//!   `open_privacy` (Windows needs no Accessibility permission: always trusted; on Linux
//!   "trusted" means the input can be read: X11, or `/dev/input` on Wayland).
//! - **Sound:** `play` (the cues), `mute_output` / `restore_output`.
//! - **Shell:** `open`, `open_url`, `starts_at_login` / `set_start_at_login`, `mouse_screen`,
//!   `os_version`, `preferred_languages` (for `ui_language = "auto"`), `log_to`, `local_time`,
//!   `say` (the system voice as a command).
//! - **App and windows:** `MainThreadMarker`, `menu_bar_app`, `tray` (whether the desktop shows
//!   the tray icon: always on macOS and Windows; on Linux the session bus decides),
//!   `show_in_dock`, `activate`, `set_app_menu`; `preview_app` on macOS only (the PNG previews
//!   draw AppKit views).
//! - **Modules:** `overlay::Overlay` and `bar::Bar` (the windows; their platform-free parts are
//!   `src/overlay.rs` and `src/bar.rs`), `ane` (the Neural Engine helper), `pocket` (PocketTTS),
//!   `media` (pausing playback). On Windows the last three and the bar are stand-ins that do
//!   nothing or say "not supported"; on Linux `ane`, `pocket` and the bar are.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
compile_error!(
    "the dictate app builds on macOS, Windows and Linux; elsewhere build the core: cargo build -p dictate-core"
);

/// Set by the worker while a recording runs; the tap then swallows Esc to cancel it.
pub static RECORDING: AtomicBool = AtomicBool::new(false);
/// Set by the worker while a spoken conversation or a reading is on; Esc then ends it.
pub static VOICE: AtomicBool = AtomicBool::new(false);

/// What the tap (and the local server, the menu, the worker itself) tells the worker.
#[derive(Debug)]
pub enum TapEvent {
    Press(usize, Instant),
    Release(usize, Instant),
    /// Another key went down while this trigger was held.
    Interrupt(usize, Instant),
    Cancel,
    /// From the local server (`local.rs`): start a hands-free recording, or stop the running one.
    Toggle,
    /// From the menu: record a moment and report the level.
    MicTest,
    /// New settings (already validated).
    Configure(Box<crate::config::Config>),
    /// A model download started by a settings change has ended (error message on failure).
    Downloaded(String, Result<(), String>),
    /// From the menu (or the shortcut): read the selected text aloud / talk it over (`talk.rs`).
    Read,
    Discuss,
    /// The voice has finished speaking (the number says which reading or answer).
    Spoke(u64),
    /// The agent's answer: session id and text, or what went wrong.
    Answer(u64, Result<(u64, String), String>),
    /// From the local server: talk over this existing session of the agent plugin (a web page's
    /// Talk button); it
    /// stays open when the conversation ends.
    DiscussSession(u64),
    /// From the local server: end the spoken conversation (a web page's Stop button).
    EndTalk,
}
