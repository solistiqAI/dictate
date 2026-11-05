//! Linux glue (BETA): the triggers, pasting, sounds, start at login. **Never run on a real Linux
//! desktop yet**: it compiles (`scripts/check-linux.sh`, docs/linux.md), nothing more. Expect
//! the first run to find bugs.
//!
//! What it does, against `platform/mod.rs`:
//!
//! - Triggers: in an X11 session XInput2 raw key and button events (`x11.rs`, no permission),
//!   with passive grabs so a key or mouse trigger (F13, a combo, a side button) and Esc while
//!   recording do not reach the app; on Wayland (no global input for normal apps by design) the
//!   keyboards and mice in `/dev/input` (`evdev.rs`; the user must be in the `input` group;
//!   nothing is swallowed there). Both feed `dictate_core::hook::Hook` through the evdev → key
//!   mapping in `dictate_core::linux`, so the rules are the Windows hook's (hold, tap, another key
//!   interrupts, Esc cancels). No Fn key: the defaults are right Alt and F13.
//! - Paste: the clipboard, then Ctrl+V: XTest on X11; `ydotool` or `wtype` on Wayland. When no
//!   key can be sent the text stays on the clipboard and the card says so (`app.rs`, `deliver`).
//! - Selected text (reading aloud, talking it over): the PRIMARY selection.
//! - Sounds: `paplay` (PulseAudio, PipeWire), else `pw-play`, else `aplay`. Mute while
//!   recording: `pactl` on the default sink. Pausing media: `playerctl` (MPRIS, `media.rs`).
//! - Start at login: `~/.config/autostart/dictate.desktop`. Links and folders: `xdg-open`.
//! - The tray: whether the desktop shows it (`tray`: a StatusNotifier host on the session bus,
//!   else an XEmbed tray); without one `app.rs` opens the window and quits when it closes.
//! - The Souffleur's "Andere": the default sink's monitor through `parec` (`system_audio.rs`).
//!
//! Missing (stand-ins in `ane.rs`, `bar.rs`, `pocket.rs`): Neural Engine and Apple's model, the
//! bar, PocketTTS (the system voice is `espeak-ng`), learning from corrections (`Field::focused`
//! is `None`; AT-SPI would do it), the focus check (`focused_accepts_text` cannot tell, so it
//! pastes), the overlay's struck-through words and shortcuts beside the pill.

pub mod ane;
pub mod bar;
mod evdev;
pub mod media;
pub mod overlay;
pub mod pocket;
pub mod system_audio;
mod x11;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};

use super::{RECORDING, TapEvent, VOICE};
use crate::trigger::{Capture, Captured, Trigger};
use dictate_core::hook::{Event, Hook, Out, VK_ESCAPE};
use dictate_core::linux::Session;

/// No main-thread marker type on Linux (GTK's main thread is tao's); the app passes this one to
/// the overlay and the bar.
#[derive(Clone, Copy, Debug)]
pub struct MainThreadMarker(std::marker::PhantomData<*const ()>);

impl MainThreadMarker {
    pub fn new() -> Option<Self> {
        Some(MainThreadMarker(std::marker::PhantomData))
    }
}

/// The session's kind, read once.
fn session() -> Session {
    static SESSION: std::sync::OnceLock<Session> = std::sync::OnceLock::new();
    *SESSION.get_or_init(|| dictate_core::linux::session(|name| std::env::var(name).ok()))
}

// ---------------------------------------------------------------------------------------------
// Triggers

struct HookState {
    hook: Hook,
    events: Sender<TapEvent>,
}

static HOOK: Mutex<Option<HookState>> = Mutex::new(None);

type OnCaptured = Box<dyn Fn(Captured) + Send>;
/// While the settings window records a trigger: the capture and who gets the result.
static CAPTURE: Mutex<Option<(Capture, OnCaptured)>> = Mutex::new(None);

/// Until when (ms since [`epoch`]) events are our own keys (a paste's Ctrl+V) and are ignored:
/// neither XInput2 raw events nor `/dev/input` mark where a key came from.
static OWN_UNTIL: AtomicU64 = AtomicU64::new(0);

/// The virtual-key codes of Alt, Super and Shift held now (bit per [`BLOCKERS`] entry), for
/// [`wait_for_modifiers`].
static HELD: AtomicU64 = AtomicU64::new(0);
const BLOCKERS: [u16; 6] = [0xA4, 0xA5, 0x5B, 0x5C, 0xA0, 0xA1];

fn epoch() -> Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn millis() -> u64 {
    epoch().elapsed().as_millis() as u64
}

/// Records the next trigger pressed (see [`Capture`]); `on_done` gets it once. Needs the tap.
pub fn start_capture(on_done: impl Fn(Captured) + Send + 'static) {
    *CAPTURE.lock().unwrap() = Some((Capture::default(), Box::new(on_done)));
}

/// Ends a running capture; `true` when one was running.
pub fn stop_capture() -> bool {
    CAPTURE.lock().unwrap().take().is_some()
}

/// The thread that reads the input; dropping it stops the thread (within a fifth of a second).
pub struct Tap {
    stop: std::sync::Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        *HOOK.lock().unwrap() = None;
    }
}

/// Starts watching the triggers: XInput2 in an X11 session, `/dev/input` on Wayland. Keep the
/// returned value alive.
pub fn install_tap(triggers: Vec<Trigger>, events: Sender<TapEvent>) -> Result<Tap> {
    *HOOK.lock().unwrap() = Some(HookState {
        hook: Hook::new(triggers.clone()),
        events,
    });
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let thread = match session() {
        Session::X11 => x11::watch(triggers, stop.clone()),
        Session::Wayland => evdev::watch(stop.clone()),
    };
    let thread = match thread {
        Ok(thread) => thread,
        Err(e) => {
            *HOOK.lock().unwrap() = None;
            return Err(e);
        }
    };
    keep_clipboard();
    Ok(Tap {
        stop,
        thread: Some(thread),
    })
}

/// One input event from either backend. Returns whether it is ours (a trigger's; only the X11
/// grabs can actually keep it from the app).
fn handle(event: Event) -> bool {
    if millis() < OWN_UNTIL.load(Ordering::Relaxed) {
        return false;
    }
    if let Event::KeyDown(vk) | Event::KeyUp(vk) = event
        && let Some(bit) = BLOCKERS.iter().position(|&b| b == vk)
    {
        match event {
            Event::KeyDown(_) => HELD.fetch_or(1 << bit, Ordering::Relaxed),
            _ => HELD.fetch_and(!(1 << bit), Ordering::Relaxed),
        };
    }
    let mut slot = HOOK.lock().unwrap();
    let Some(state) = slot.as_mut() else {
        return false;
    };
    if let Some(swallow) = capture(&mut state.hook, event) {
        return swallow;
    }
    let now = Instant::now();
    if event == Event::KeyDown(VK_ESCAPE) && cancels() {
        let _ = state.events.send(TapEvent::Cancel);
        return true;
    }
    let mut out = Vec::new();
    let ours = state.hook.feed(event, &mut out);
    for o in out {
        let _ = state.events.send(match o {
            Out::Press(i) => TapEvent::Press(i, now),
            Out::Release(i) => TapEvent::Release(i, now),
            Out::Interrupt(i) => TapEvent::Interrupt(i, now),
        });
    }
    ours
}

/// Esc cancels now (a recording or a voice runs).
fn cancels() -> bool {
    RECORDING.load(Ordering::Relaxed) || VOICE.load(Ordering::Relaxed)
}

/// Feeds an event to a running capture; `None` when none runs (normal handling).
fn capture(hook: &mut Hook, event: Event) -> Option<bool> {
    let mut slot = CAPTURE.lock().unwrap();
    let (capture, _) = slot.as_mut()?;
    let modifier = hook.track(event).is_some();
    let Some(seen) = hook.seen(event) else {
        return Some(false);
    };
    if let Some(done) = capture.feed(seen)
        && let Some((_, on_done)) = slot.take()
    {
        drop(slot);
        on_done(done);
    }
    Some(!modifier)
}

/// Waits up to a second for Alt, Super and Shift to be let go (a trigger still held would turn
/// our Ctrl+V into another shortcut).
fn wait_for_modifiers() {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(1) && HELD.load(Ordering::Relaxed) != 0 {
        thread::sleep(Duration::from_millis(20));
    }
}

/// Sends Ctrl+V to the focused window: XTest on X11, `ydotool` or `wtype` on Wayland. Events
/// in the next half second are taken as ours.
fn send_paste_keys() -> Result<()> {
    OWN_UNTIL.store(millis() + 500, Ordering::Relaxed);
    match session() {
        Session::X11 => x11::send_ctrl_v(),
        Session::Wayland => wayland_ctrl_v(),
    }
}

/// `ydotool` (uinput; any compositor, needs its daemon `ydotoold`), else `wtype` (wlroots and
/// KDE's virtual keyboard protocol; not GNOME).
fn wayland_ctrl_v() -> Result<()> {
    use dictate_core::linux::{KEY_LEFTCTRL, KEY_V};
    let ydotool = [
        format!("{KEY_LEFTCTRL}:1"),
        format!("{KEY_V}:1"),
        format!("{KEY_V}:0"),
        format!("{KEY_LEFTCTRL}:0"),
    ];
    let mut tried = Vec::new();
    for (program, args) in [
        (
            "ydotool",
            std::iter::once("key".to_string())
                .chain(ydotool)
                .collect::<Vec<_>>(),
        ),
        (
            "wtype",
            ["-M", "ctrl", "-k", "v", "-m", "ctrl"]
                .map(String::from)
                .to_vec(),
        ),
    ] {
        match quiet(program).args(&args).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => tried.push(format!("{program} failed ({status})")),
            Err(_) => tried.push(format!("{program} not installed")),
        }
    }
    bail!("cannot type Ctrl+V on Wayland: {}", tried.join(", "))
}

// ---------------------------------------------------------------------------------------------
// Text in other apps

/// When we last checked whether the input can be watched, and the answer.
static TRUSTED: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Whether the triggers can be watched: always in an X11 session with XInput2; on Wayland when
/// a keyboard in `/dev/input` is readable (the user is in the `input` group). Checked at most
/// every few seconds (the app asks on every pass of its loop).
pub fn is_trusted() -> bool {
    let mut cached = TRUSTED.lock().unwrap();
    if let Some((at, answer)) = *cached
        && at.elapsed() < Duration::from_secs(5)
    {
        return answer;
    }
    let answer = match session() {
        Session::X11 => x11::available(),
        Session::Wayland => evdev::readable(),
    };
    *cached = Some((Instant::now(), answer));
    answer
}

/// Nothing to prompt for: the `input` group is set by the user (docs/linux.md); says how.
pub fn ensure_accessibility() -> bool {
    let trusted = is_trusted();
    if !trusted {
        eprintln!(
            "input: cannot watch the keyboard. On Wayland add yourself to the input group \
             (sudo usermod -aG input $USER, then log in again); see docs/linux.md"
        );
    }
    trusted
}

/// Linux offers no portable way to ask what the focused widget is (AT-SPI would): cannot tell,
/// so the text is pasted.
pub fn focused_accepts_text() -> Option<bool> {
    None
}

/// Learning from corrections reads the focused field; not on Linux yet (AT-SPI's Text interface
/// would do it), so there is never a field to watch.
pub struct Field {
    /// The text without the selection (a paste replaces it).
    pub text: String,
    /// Insertion point in UTF-16 units.
    pub at: usize,
}

impl Field {
    pub fn focused() -> Option<Field> {
        None
    }

    pub fn current_text(&self) -> Option<String> {
        None
    }
}

thread_local! {
    /// One clipboard kept for the life of the app: on X11 the copied text lives in the process
    /// that owns it, and arboard hands it away when its last handle is dropped (`app.rs` makes a
    /// new one for every copy).
    static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

fn keep_clipboard() {
    CLIPBOARD.with(|c| {
        if c.borrow().is_none() {
            *c.borrow_mut() = arboard::Clipboard::new().ok();
        }
    });
}

/// The selected text: X11's (and Wayland's) PRIMARY selection, what a middle click would paste.
/// `None` when nothing is selected.
pub fn selected_text() -> Option<String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};
    let mut clipboard = arboard::Clipboard::new().ok()?;
    clipboard
        .get()
        .clipboard(LinuxClipboardKind::Primary)
        .text()
        .ok()
        .filter(|t| !t.trim().is_empty())
}

/// Pastes `text` into the focused app via the clipboard and Ctrl+V, then restores the clipboard.
/// When no key can be sent, the text stays on the clipboard and the error says why.
pub fn paste(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("open clipboard")?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text).context("set clipboard")?;
    thread::sleep(Duration::from_millis(30));
    wait_for_modifiers();
    send_paste_keys()?;
    // Give the target app time to read the clipboard before we put the old content back.
    thread::sleep(Duration::from_millis(250));
    if let Some(previous) = previous {
        clipboard.set_text(previous).context("restore clipboard")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Sound

/// A command with its output thrown away.
fn quiet(program: &str) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Plays a cue (a WAV from `feedback.rs`) at `volume` without blocking: `paplay` (PulseAudio and
/// PipeWire's pulse server), else `pw-play`, else ALSA's `aplay` (at full volume).
pub fn play(path: &Path, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    let path = path.to_path_buf();
    let volume = volume.clamp(0.0, 1.0);
    thread::spawn(move || {
        let tries = [
            (
                "paplay",
                vec![format!("--volume={}", (volume * 65536.0) as u32)],
            ),
            ("pw-play", vec![format!("--volume={volume:.2}")]),
            ("aplay", vec!["-q".to_string()]),
        ];
        for (program, args) in tries {
            if quiet(program)
                .args(&args)
                .arg(&path)
                .status()
                .is_ok_and(|s| s.success())
            {
                return;
            }
        }
    });
}

/// Whether we muted the output (so only we unmute it, and quitting restores it).
static MUTED_BY_US: AtomicBool = AtomicBool::new(false);

/// Mutes the default sink unless it already is muted (`pactl`, PulseAudio or PipeWire).
pub fn mute_output() {
    let muted = Command::new("pactl")
        .args(["get-sink-mute", "@DEFAULT_SINK@"])
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("yes"));
    if muted == Some(false)
        && quiet("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", "1"])
            .status()
            .is_ok_and(|s| s.success())
    {
        MUTED_BY_US.store(true, Ordering::Relaxed);
    }
}

/// Undoes [`mute_output`]; a no-op when we did not mute.
pub fn restore_output() {
    if MUTED_BY_US.swap(false, Ordering::Relaxed) {
        let _ = quiet("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", "0"])
            .status();
    }
}

// ---------------------------------------------------------------------------------------------
// Shell, settings, the app

/// Linux asks for nothing (Flatpak's portal would, but this is no Flatpak): unknown, a recording
/// of digital silence says so (`app.rs`, NO_MIC).
pub fn microphone_permission() -> Option<bool> {
    None
}

/// No privacy panes to open: says in the log what to do instead.
pub fn open_privacy(pane: &str) {
    match pane {
        "Accessibility" => {
            ensure_accessibility();
        }
        _ => eprintln!("{pane}: check the sound settings (pavucontrol, the desktop's settings)"),
    }
}

/// Starts `xdg-open` and reaps it in the background.
fn xdg_open(target: &std::ffi::OsStr) {
    match quiet("xdg-open").arg(target).spawn() {
        Ok(mut child) => {
            thread::spawn(move || child.wait());
        }
        Err(e) => eprintln!("xdg-open: {e}"),
    }
}

/// Opens a file or folder with its default app (`text`: a text file, which `xdg-open` gives to
/// the text editor anyway).
pub fn open(path: &Path, _text: bool) {
    xdg_open(path.as_os_str());
}

/// Opens a link with its default app.
pub fn open_url(url: &str) {
    xdg_open(std::ffi::OsStr::new(url));
}

fn autostart_path() -> Option<PathBuf> {
    Some(
        dirs::config_dir()?
            .join("autostart")
            .join("dictate.desktop"),
    )
}

pub fn starts_at_login() -> bool {
    autostart_path().is_some_and(|p| p.exists())
}

/// An XDG autostart entry that starts this program at login.
pub fn set_start_at_login(on: bool) -> Result<()> {
    let path = autostart_path().ok_or_else(|| anyhow!("no config folder"))?;
    if !on {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let ort = std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from);
    std::fs::write(
        &path,
        dictate_core::linux::autostart_entry(&exe, ort.as_deref()),
    )
    .with_context(|| format!("write {}", path.display()))
}

/// Where the Souffleur's panel goes: the window manager's placement on Linux for now.
pub fn mouse_screen() -> Option<(f64, f64, f64, f64)> {
    None
}

/// Nothing to set up: a tao app without windows shows only its tray icon.
pub fn menu_bar_app<T: 'static>(_event_loop: &mut tao::event_loop::EventLoop<T>) {}

/// Whether the desktop shows the tray icon (`dictate_core::linux::Tray`): a StatusNotifier
/// watcher with a host on the session bus, else an XEmbed tray. Asks the bus through GIO (in the
/// tree with GTK), each call waiting at most a second.
pub fn tray() -> dictate_core::linux::Tray {
    let (watcher, host) = status_notifier();
    dictate_core::linux::Tray::from_probe(watcher, host, x11::system_tray())
}

/// Whether `org.kde.StatusNotifierWatcher` has an owner, and whether a host has registered with
/// it; `None` where the bus did not answer.
fn status_notifier() -> (Option<bool>, Option<bool>) {
    use gtk::gio;
    use gtk::glib::{Variant, VariantTy, prelude::*};
    const WATCHER: &str = "org.kde.StatusNotifierWatcher";
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return (None, None);
    };
    let call =
        |name: &str, path: &str, interface: &str, method: &str, args: Variant, reply: &str| {
            bus.call_sync(
                Some(name),
                path,
                interface,
                method,
                Some(&args),
                Some(VariantTy::new(reply).unwrap()),
                gio::DBusCallFlags::NO_AUTO_START,
                1000,
                gio::Cancellable::NONE,
            )
            .ok()
        };
    let owned = call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        (WATCHER,).to_variant(),
        "(b)",
    )
    .and_then(|r| r.get::<(bool,)>())
    .map(|(b,)| b);
    if owned != Some(true) {
        return (owned, None);
    }
    let host = call(
        WATCHER,
        "/StatusNotifierWatcher",
        "org.freedesktop.DBus.Properties",
        "Get",
        (WATCHER, "IsStatusNotifierHostRegistered").to_variant(),
        "(v)",
    )
    .and_then(|r| r.get::<(Variant,)>())
    .and_then(|(v,)| v.get::<bool>());
    (owned, host)
}

/// The task bar shows open windows by itself.
pub fn show_in_dock<T: 'static>(_target: &tao::event_loop::EventLoopWindowTarget<T>, _on: bool) {}

/// A window comes to the front with its own `set_focus`.
pub fn activate(_mtm: MainThreadMarker) {}

/// No app-wide menu bar on Linux; the webview handles Ctrl+C/V/Z/A in its fields by itself.
pub fn set_app_menu(menu: tray_icon::menu::Menu) {
    std::mem::forget(menu);
}

/// The system for the usage report ("Ubuntu 24.04.1 LTS (Linux 6.8.0-47-generic)").
pub fn os_version() -> String {
    dictate_core::linux::os_name(
        &std::fs::read_to_string("/etc/os-release").unwrap_or_default(),
        &std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default(),
    )
}

/// The user's languages from `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, `LANG`.
pub fn preferred_languages() -> Vec<String> {
    dictate_core::linux::preferred_languages(|name| std::env::var(name).ok())
}

/// Sends stderr (our log) into `file` from now on.
pub fn log_to(file: &std::fs::File) {
    use std::os::fd::AsRawFd;
    unsafe { libc::dup2(file.as_raw_fd(), 2) };
}

/// Local time of a Unix timestamp: year, month, day, hour, minute.
pub fn local_time(unix: f64) -> (i32, u32, u32, u32, u32) {
    let t = unix as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    (
        tm.tm_year + 1900,
        tm.tm_mon as u32 + 1,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
    )
}

/// A program on the PATH.
fn on_path(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

/// The system voice: `espeak-ng` reading its text from stdin (`voice.rs` writes it and waits),
/// in the language of the UI. The Mac's voice names mean nothing to it and are left out; `rate`
/// is words per minute on both.
pub fn say(_voice: &str, rate: u32) -> Result<Command> {
    let Some(espeak) = on_path("espeak-ng").or_else(|| on_path("espeak")) else {
        bail!(
            "{}",
            crate::text::pick(
                "Vorlesen braucht espeak-ng (z. B. sudo apt install espeak-ng)",
                "Reading aloud needs espeak-ng (e.g. sudo apt install espeak-ng)"
            )
        );
    };
    let mut command = Command::new(espeak);
    command.args(["-v", crate::text::lang().code(), "-s", &rate.to_string()]);
    Ok(command)
}

/// Whether we were started as an app rather than from a terminal, beyond what stderr shows: no
/// (`main.rs` checks stderr; from the autostart entry it is no terminal).
pub fn launched_as_app() -> bool {
    false
}
