//! Windows glue (BETA): low-level hooks that watch the triggers, pasting, the focus check, sounds,
//! start at login. **Never run on a real Windows PC yet**: it compiles (checked from a Mac with
//! `cargo xwin check --target x86_64-pc-windows-msvc`, see README → "Windows (beta)"), nothing
//! more. Expect the first run on real hardware to find bugs.
//!
//! What it does, against `platform/mod.rs`:
//!
//! - Triggers: `WH_KEYBOARD_LL` and `WH_MOUSE_LL` hooks on the main thread (tao's message loop
//!   runs them), fed into `dictate_core::hook::Hook`, which keeps the macOS tap's rules (hold,
//!   tap, interrupt by another key, Esc cancels). No Fn key: the defaults are right Alt and F13.
//!   A right-Alt or Windows-key trigger sends an unassigned key (0xE8) when pressed, so releasing
//!   it does not open the window's menu or the Start menu.
//! - Paste: the clipboard plus `SendInput` Ctrl+V, the old clipboard put back. Waits up to a
//!   second for Alt/Win/Shift to be let go first (Ctrl+Alt+V would type something else).
//! - Focus check: UI Automation's focused element; lists, buttons, menus and the like mean "no
//!   text field" (the overlay keeps the text), everything else pastes.
//! - Sounds: `PlaySound` from memory, scaled to the volume. Mute while recording: the default
//!   output's `IAudioEndpointVolume`.
//! - Start at login: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value "Dictate".
//! - No permission prompts: `is_trusted` is always true; the microphone privacy switch is in
//!   Settings (`open_privacy("Microphone")` opens it).
//!
//! Missing (stand-ins in `ane.rs`, `bar.rs`, `media.rs`, `pocket.rs`): Neural Engine and Apple's
//! model, the bar, pausing media, PocketTTS and the system voice (`say`), learning from
//! corrections (`Field::focused` is `None`), the overlay's struck-through words and shortcuts beside the pill.

pub mod ane;
pub mod bar;
pub mod media;
pub mod overlay;
pub mod pocket;
pub mod system_audio;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{HANDLE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};
use windows::core::{PCWSTR, w};

use super::{RECORDING, TapEvent, VOICE};
use crate::trigger::{Capture, Captured, Input, Trigger};
use dictate_core::hook::{Event, Hook, Out, VK_ESCAPE};

/// Marks input we send ourselves (Ctrl+V, Ctrl+C, the menu mask), so the hooks let it through.
const OWN_EVENT: usize = 0x5051_4954; // "SQIT"
const VK_CONTROL: u16 = 0x11;
const VK_C: u16 = 0x43;
const VK_V: u16 = 0x56;
/// Unassigned virtual key: "another key came" for Windows, so a lone Alt/Win does nothing.
const VK_MASK: u16 = 0xE8;
/// macOS key codes of the Alt and Windows keys (`dictate_core::hook`).
const ALT_AND_WIN: [u16; 4] = [58, 61, 55, 54];

/// Windows has no main-thread marker type; the app passes this one to the overlay and the bar.
#[derive(Clone, Copy, Debug)]
pub struct MainThreadMarker(std::marker::PhantomData<*const ()>);

impl MainThreadMarker {
    pub fn new() -> Option<Self> {
        Some(MainThreadMarker(std::marker::PhantomData))
    }
}

// ---------------------------------------------------------------------------------------------
// Triggers

struct HookState {
    hook: Hook,
    triggers: Vec<Trigger>,
    events: Sender<TapEvent>,
}

static HOOK: Mutex<Option<HookState>> = Mutex::new(None);

type OnCaptured = Box<dyn Fn(Captured) + Send>;
/// While the settings window records a trigger: the capture and who gets the result.
static CAPTURE: Mutex<Option<(Capture, OnCaptured)>> = Mutex::new(None);
/// The key that ended a capture: its auto-repeats and release are swallowed too.
static CAPTURED_KEY: Mutex<Option<u16>> = Mutex::new(None);

/// Records the next trigger pressed (see [`Capture`]); `on_done` gets it once. Needs the hooks.
pub fn start_capture(on_done: impl Fn(Captured) + Send + 'static) {
    *CAPTURE.lock().unwrap() = Some((Capture::default(), Box::new(on_done)));
}

/// Ends a running capture; `true` when one was running.
pub fn stop_capture() -> bool {
    CAPTURE.lock().unwrap().take().is_some()
}

/// The installed hooks; dropping it removes them.
pub struct Tap {
    keyboard: HHOOK,
    mouse: HHOOK,
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            let _ = UnhookWindowsHookEx(self.mouse);
        }
        *HOOK.lock().unwrap() = None;
    }
}

/// Installs the keyboard and mouse hooks on the current (main) thread, whose message loop runs
/// them. Keep the returned value alive.
pub fn install_tap(triggers: Vec<Trigger>, events: Sender<TapEvent>) -> Result<Tap> {
    *HOOK.lock().unwrap() = Some(HookState {
        hook: Hook::new(triggers.clone()),
        triggers,
        events,
    });
    unsafe {
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
            .context("install the keyboard hook")?;
        let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) {
            Ok(mouse) => mouse,
            Err(e) => {
                let _ = UnhookWindowsHookEx(keyboard);
                return Err(e).context("install the mouse hook");
            }
        };
        Ok(Tap { keyboard, mouse })
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if info.dwExtraInfo != OWN_EVENT {
            let vk = info.vkCode as u16;
            let event = match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Event::KeyDown(vk),
                _ => Event::KeyUp(vk),
            };
            if handle(event) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        // XBUTTON1 (back) = 3, XBUTTON2 (forward) = 4, as the triggers number them on macOS.
        let x = 2 + (info.mouseData >> 16) as u16;
        let event = match wparam.0 as u32 {
            WM_MBUTTONDOWN => Some(Event::MouseDown(2)),
            WM_MBUTTONUP => Some(Event::MouseUp(2)),
            WM_XBUTTONDOWN => Some(Event::MouseDown(x)),
            WM_XBUTTONUP => Some(Event::MouseUp(x)),
            _ => None,
        };
        if info.dwExtraInfo != OWN_EVENT
            && let Some(event) = event
            && handle(event)
        {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// One hook event; `true` swallows it. Must be quick: Windows drops slow hooks.
fn handle(event: Event) -> bool {
    let mut slot = HOOK.lock().unwrap();
    let Some(state) = slot.as_mut() else {
        return false;
    };
    if let Some(swallow) = capture(&mut state.hook, event) {
        return swallow;
    }
    let now = Instant::now();
    if event == Event::KeyDown(VK_ESCAPE)
        && (RECORDING.load(Ordering::Relaxed) || VOICE.load(Ordering::Relaxed))
    {
        let _ = state.events.send(TapEvent::Cancel);
        return true;
    }
    let mut out = Vec::new();
    let swallow = state.hook.feed(event, &mut out);
    let mut mask = false;
    for o in out {
        let _ = state.events.send(match o {
            Out::Press(i) => {
                mask |= matches!(state.triggers[i].input,
                    Input::Modifier { keycode, .. } if ALT_AND_WIN.contains(&keycode));
                TapEvent::Press(i, now)
            }
            Out::Release(i) => TapEvent::Release(i, now),
            Out::Interrupt(i) => TapEvent::Interrupt(i, now),
        });
    }
    drop(slot);
    if mask {
        send_keys(&[(VK_MASK, true), (VK_MASK, false)]);
    }
    swallow
}

/// Feeds an event to a running capture; `None` when none runs (normal handling).
fn capture(hook: &mut Hook, event: Event) -> Option<bool> {
    {
        let mut captured = CAPTURED_KEY.lock().unwrap();
        if let (Event::KeyDown(vk) | Event::KeyUp(vk), Some(key)) = (event, *captured)
            && vk == key
        {
            if let Event::KeyUp(_) = event {
                *captured = None;
            }
            return Some(true); // the captured key's auto-repeats and release
        }
    }
    let mut slot = CAPTURE.lock().unwrap();
    let (capture, _) = slot.as_mut()?;
    let modifier = hook.track(event).is_some();
    let Some(seen) = hook.seen(event) else {
        return Some(false); // releases of keys pressed before the capture
    };
    if let Some(done) = capture.feed(seen)
        && let Some((_, on_done)) = slot.take()
    {
        drop(slot);
        if let (Event::KeyDown(vk), false) = (event, modifier) {
            *CAPTURED_KEY.lock().unwrap() = Some(vk);
        }
        on_done(done);
    }
    // Modifiers pass (as flag changes do on macOS); keys and buttons are the capture's.
    Some(!modifier)
}

/// Sends key presses (virtual key, down) marked as ours.
fn send_keys(keys: &[(u16, bool)]) -> bool {
    let inputs: Vec<INPUT> = keys
        .iter()
        .map(|&(vk, down)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: match down {
                        true => KEYBD_EVENT_FLAGS(0),
                        false => KEYEVENTF_KEYUP,
                    },
                    time: 0,
                    dwExtraInfo: OWN_EVENT,
                },
            },
        })
        .collect();
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

/// Waits up to a second for Alt, the Windows keys and Shift to be let go (a trigger still held
/// would turn our Ctrl+V into another shortcut).
fn wait_for_modifiers() {
    const HELD: [i32; 4] = [0x12, 0x5B, 0x5C, 0x10]; // VK_MENU, VK_LWIN, VK_RWIN, VK_SHIFT
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(1)
        && HELD
            .iter()
            .any(|&vk| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0)
    {
        thread::sleep(Duration::from_millis(20));
    }
}

// ---------------------------------------------------------------------------------------------
// Text in other apps

/// No Accessibility permission on Windows: hooks and SendInput work for every app of the same
/// or lower integrity level (not into apps running as administrator while we do not).
pub fn ensure_accessibility() -> bool {
    true
}

pub fn is_trusted() -> bool {
    true
}

/// Whether the focused element takes text, from UI Automation: `Some(true)` for edit fields,
/// `Some(false)` for lists, buttons, menus and other controls that clearly do not, `None` when we
/// cannot tell (documents, web content, custom controls, errors), in which case we paste anyway.
pub fn focused_accepts_text() -> Option<bool> {
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;
        let kind = element.CurrentControlType().ok()?;
        accepts_text(kind.0)
    }
}

/// UI Automation control type ids (UIA_*ControlTypeId) → takes text?
fn accepts_text(control_type: i32) -> Option<bool> {
    match control_type {
        50004 | 50003 => Some(true), // Edit, ComboBox
        50000 | 50002 | 50006 | 50007 | 50008 | 50009 | 50010 | 50011 | 50013 | 50015 | 50019
        | 50021 | 50023 | 50024 => Some(false), // Button, CheckBox, Image, ListItem, List,
        // Menu, MenuBar, MenuItem, RadioButton, Slider, TabItem, ToolBar, Tree, TreeItem
        _ => None,
    }
}

/// Learning from corrections reads the focused field; not on Windows yet (UI Automation's
/// TextPattern would do it), so there is never a field to watch.
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

/// The text selected in the focused app: Ctrl+C with the clipboard put back afterwards. `None`
/// when nothing is selected.
pub fn selected_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let previous = clipboard.get_text().ok();
    // A marker nobody copies: still there after Ctrl+C means nothing was selected.
    let marker = format!("dictate-selection-{}", std::process::id());
    clipboard.set_text(marker.clone()).ok()?;
    thread::sleep(Duration::from_millis(30));
    wait_for_modifiers();
    send_keys(&[
        (VK_CONTROL, true),
        (VK_C, true),
        (VK_C, false),
        (VK_CONTROL, false),
    ]);
    thread::sleep(Duration::from_millis(200));
    let copied = clipboard.get_text().ok();
    match previous {
        Some(previous) => {
            let _ = clipboard.set_text(previous);
        }
        None => {
            let _ = clipboard.clear();
        }
    }
    copied.filter(|t| *t != marker && !t.trim().is_empty())
}

/// Pastes `text` into the focused app via the clipboard and Ctrl+V, then restores the clipboard.
pub fn paste(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("open clipboard")?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text).context("set clipboard")?;
    thread::sleep(Duration::from_millis(30));
    wait_for_modifiers();
    if !send_keys(&[
        (VK_CONTROL, true),
        (VK_V, true),
        (VK_V, false),
        (VK_CONTROL, false),
    ]) {
        bail!("SendInput was blocked (the app runs as administrator?)");
    }
    // Give the target app time to read the clipboard before we put the old content back.
    thread::sleep(Duration::from_millis(250));
    if let Some(previous) = previous {
        clipboard.set_text(previous).context("restore clipboard")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Sound

/// Plays a cue (a WAV from `feedback.rs`) at `volume` without blocking.
pub fn play(path: &std::path::Path, volume: f32) {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
    if volume <= 0.0 {
        return;
    }
    let Ok(wav) = scaled_wav(path, volume) else {
        return;
    };
    // PlaySound has no volume of its own: the samples are scaled, played from memory on a
    // thread that keeps the buffer until the sound is over.
    thread::spawn(move || unsafe {
        let _ = PlaySoundW(
            PCWSTR(wav.as_ptr().cast()),
            None,
            SND_MEMORY | SND_NODEFAULT | SND_SYNC,
        );
    });
}

fn scaled_wav(path: &std::path::Path, volume: f32) -> Result<Vec<u8>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut out, spec)?;
        match spec.sample_format {
            hound::SampleFormat::Float => {
                for s in reader.samples::<f32>() {
                    writer.write_sample(s? * volume)?;
                }
            }
            hound::SampleFormat::Int => {
                for s in reader.samples::<i32>() {
                    writer.write_sample((s? as f32 * volume) as i32)?;
                }
            }
        }
        writer.finalize()?;
    }
    Ok(out.into_inner())
}

/// Whether we muted the output (so only we unmute it, and quitting restores it).
static MUTED_BY_US: AtomicBool = AtomicBool::new(false);

fn output_volume() -> Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows::Win32::Media::Audio::{
        IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate(CLSCTX_ALL, None).ok()
    }
}

/// Mutes the default output unless it already is muted.
pub fn mute_output() {
    let Some(volume) = output_volume() else {
        return;
    };
    unsafe {
        if volume.GetMute().is_ok_and(|m| !m.as_bool())
            && volume.SetMute(true, std::ptr::null()).is_ok()
        {
            MUTED_BY_US.store(true, Ordering::Relaxed);
        }
    }
}

/// Undoes [`mute_output`]; a no-op when we did not mute.
pub fn restore_output() {
    if MUTED_BY_US.swap(false, Ordering::Relaxed)
        && let Some(volume) = output_volume()
    {
        unsafe {
            let _ = volume.SetMute(false, std::ptr::null());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Shell, settings, the app

/// Windows asks for nothing up front; the microphone can be switched off for desktop apps in
/// Settings → Privacy. Unknown here: a recording of digital silence says so (`app.rs`, NO_MIC).
pub fn microphone_permission() -> Option<bool> {
    None
}

/// Privacy pages of Settings ("Microphone"; there is no Accessibility page to open).
pub fn open_privacy(pane: &str) {
    if pane == "Microphone" {
        open_url("ms-settings:privacy-microphone");
    }
}

fn shell_open(target: &str, program: Option<&str>) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let target = wide(target);
    let (file, params) = match program {
        Some(program) => (wide(program), Some(target)),
        None => (target, None),
    };
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(file.as_ptr()),
            params
                .as_ref()
                .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Opens a file or folder with its default app (`text` opens it in Notepad).
pub fn open(path: &std::path::Path, text: bool) {
    let path = path.to_string_lossy();
    shell_open(&path, text.then_some("notepad.exe"));
}

/// Opens a link (https, mailto, ms-settings:, …) with its default app.
pub fn open_url(url: &str) {
    shell_open(url, None);
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("Dictate");

pub fn starts_at_login() -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

/// The Run key's value: this program, quoted, started at login.
pub fn set_start_at_login(on: bool) -> Result<()> {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
            return Ok(());
        }
        let exe = std::env::current_exe()?;
        let command: Vec<u16> = format!("\"{}\"", exe.display())
            .encode_utf16()
            .chain([0])
            .collect();
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            (command.len() * 2) as u32,
        )
        .ok()
        .context("write the Run key")?;
    }
    Ok(())
}

/// Where the Souffleur's panel goes: tao's default placement on Windows for now.
pub fn mouse_screen() -> Option<(f64, f64, f64, f64)> {
    None
}

/// Nothing to set up: a tao app without windows shows only its tray icon on Windows.
pub fn menu_bar_app<T: 'static>(_event_loop: &mut tao::event_loop::EventLoop<T>) {}

/// The notification area is always there.
pub fn tray() -> dictate_core::linux::Tray {
    dictate_core::linux::Tray::Shown
}

/// The taskbar shows open windows by itself.
pub fn show_in_dock<T: 'static>(_target: &tao::event_loop::EventLoopWindowTarget<T>, _on: bool) {}

/// Windows brings a window to the front with its own `set_focus`.
pub fn activate(_mtm: MainThreadMarker) {}

/// Windows has no app-wide menu; the webview handles Ctrl+C/V/Z/A in its fields by itself.
pub fn set_app_menu(_menu: tray_icon::menu::Menu) {}

/// The system's version for the usage report ("Windows 10.0.26100.1"), from `ver`.
pub fn os_version() -> String {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd")
        .args(["/c", "ver"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .and_then(|v| {
            let number = v
                .rsplit_once("Version ")?
                .1
                .trim_end_matches(']')
                .to_string();
            Some(format!("Windows {number}"))
        })
        .unwrap_or_else(|| "Windows ?".into())
}

/// Sends stderr (our log) into `file` from now on.
pub fn log_to(file: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::System::Console::{STD_ERROR_HANDLE, SetStdHandle};
    // Our own handle, kept open for the rest of the run.
    let Ok(file) = file.try_clone() else { return };
    unsafe {
        let _ = SetStdHandle(STD_ERROR_HANDLE, HANDLE(file.as_raw_handle()));
    }
    std::mem::forget(file);
}

/// Started from Explorer, the Start menu or the Run key, Windows gives this console program a
/// console window of its own: closed here (the log goes to the data folder instead). From a
/// terminal the console is shared and stays.
pub fn launched_as_app() -> bool {
    use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    let mut processes = [0u32; 2];
    let attached = unsafe { GetConsoleProcessList(&mut processes) };
    if attached == 1 {
        unsafe {
            let _ = FreeConsole();
        }
        return true;
    }
    false
}

/// Local time of a Unix timestamp: year, month, day, hour, minute.
/// The user's display language as the one preferred language ("de", "en"): the primary language
/// of `GetUserDefaultUILanguage` (LANG_GERMAN = 0x07). Compile-checked only.
pub fn preferred_languages() -> Vec<String> {
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    let code = match id & 0x3ff {
        0x07 => "de",
        _ => "en",
    };
    vec![code.to_string()]
}

pub fn local_time(unix: f64) -> (i32, u32, u32, u32, u32) {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    // FILETIME: 100 ns steps since 1601.
    let ticks = ((unix.max(0.0) + 11_644_473_600.0) * 10_000_000.0) as u64;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&file, &mut utc).is_err()
            || SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err()
        {
            local = utc;
        }
    }
    (
        local.wYear as i32,
        local.wMonth as u32,
        local.wDay as u32,
        local.wHour as u32,
        local.wMinute as u32,
    )
}

/// The system voice: not on Windows yet (SAPI / `System.Speech` would do it).
pub fn say(_voice: &str, _rate: u32) -> Result<std::process::Command> {
    bail!(
        "{}",
        crate::text::pick(
            "Vorlesen gibt es unter Windows noch nicht",
            "Reading aloud is not available on Windows yet"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::accepts_text;

    #[test]
    fn text_controls() {
        assert_eq!(accepts_text(50004), Some(true)); // Edit
        assert_eq!(accepts_text(50008), Some(false)); // List
        assert_eq!(accepts_text(50030), None); // Document (web content)
    }
}
