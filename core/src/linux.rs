//! The Linux app's platform-free part (BETA, `src/platform/linux/`, docs/linux.md): Linux key
//! and button numbers turned into the [`crate::hook`] events the Windows hook already feeds, the
//! X11 grabs that keep a trigger key from reaching the app, the small files and strings the
//! desktop wants (autostart entry, language, OS name), and what to do when it shows no tray icon
//! ([`Tray`]). Tested here on any OS.
//!
//! Keys come as evdev codes (`KEY_*` in `linux/input-event-codes.h`): `/dev/input` delivers them
//! as they are, X11 adds 8 (an X keycode is the evdev code + 8 under the usual evdev driver).
//! They are translated into the Windows virtual-key codes [`crate::hook::Hook`] takes, so the
//! trigger rules (hold, tap, another key interrupts, Esc cancels) are the same code on Windows
//! and Linux, and `config.toml` names the same keys everywhere.

use std::path::Path;

use crate::hook::Event;
use crate::trigger::{Input, Trigger, flags};

/// An X keycode is the evdev code plus this (X11's evdev driver).
pub const X11_KEYCODE_OFFSET: u16 = 8;

/// evdev codes of the keys we send for a paste (left Ctrl, V), as `ydotool key` takes them.
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_V: u16 = 47;

/// Left/right modifiers by evdev code (Shift, Ctrl, Alt, Super = the Windows/⌘ key).
const MODIFIER_KEYS: &[(u16, u16)] = &[
    (42, 0xA0),  // KEY_LEFTSHIFT → VK_LSHIFT
    (54, 0xA1),  // KEY_RIGHTSHIFT
    (29, 0xA2),  // KEY_LEFTCTRL
    (97, 0xA3),  // KEY_RIGHTCTRL
    (56, 0xA4),  // KEY_LEFTALT
    (100, 0xA5), // KEY_RIGHTALT (AltGr)
    (125, 0x5B), // KEY_LEFTMETA (Super)
    (126, 0x5C), // KEY_RIGHTMETA
];

/// The modifiers that turn our Ctrl+V into another shortcut while still held: Alt, Super, Shift.
pub const PASTE_BLOCKERS: &[u16] = &[56, 100, 125, 126, 42, 54];

/// Letters by evdev code, row by row (positions, named as on a US keyboard, as macOS and the
/// triggers name them).
const LETTERS: &[(u16, u8)] = &[
    (16, b'Q'),
    (17, b'W'),
    (18, b'E'),
    (19, b'R'),
    (20, b'T'),
    (21, b'Y'),
    (22, b'U'),
    (23, b'I'),
    (24, b'O'),
    (25, b'P'),
    (30, b'A'),
    (31, b'S'),
    (32, b'D'),
    (33, b'F'),
    (34, b'G'),
    (35, b'H'),
    (36, b'J'),
    (37, b'K'),
    (38, b'L'),
    (44, b'Z'),
    (45, b'X'),
    (46, b'C'),
    (47, b'V'),
    (48, b'B'),
    (49, b'N'),
    (50, b'M'),
];

/// Keys without a virtual-key code of their own get one above every real one (0x100 + the
/// evdev code): they still count as "another key" while a trigger is held, and name no trigger.
const OTHER_KEY: u16 = 0x100;

/// The Windows virtual-key code [`crate::hook::Hook`] takes for an evdev key code. Every key gets
/// one (see [`OTHER_KEY`]); `None` for codes that are no keys (mouse, touch, joystick buttons).
pub fn vk(code: u16) -> Option<u16> {
    if let Some(&(_, vk)) = MODIFIER_KEYS.iter().find(|m| m.0 == code) {
        return Some(vk);
    }
    if let Some(&(_, letter)) = LETTERS.iter().find(|l| l.0 == code) {
        return Some(letter as u16);
    }
    let vk = match code {
        1 => crate::hook::VK_ESCAPE,
        2..=10 => 0x31 + (code - 2),      // KEY_1 … KEY_9
        11 => 0x30,                       // KEY_0
        57 => 0x20,                       // KEY_SPACE
        59..=68 => 0x70 + (code - 59),    // KEY_F1 … KEY_F10
        87 => 0x7A,                       // KEY_F11
        88 => 0x7B,                       // KEY_F12
        183..=194 => 0x7C + (code - 183), // KEY_F13 … KEY_F24
        // BTN_* (0x100–0x15f): buttons of mice, joysticks, pens and touchpads.
        0x100..=0x15f => return None,
        0 => return None,
        _ => OTHER_KEY + code,
    };
    Some(vk)
}

/// Trigger number (2 = middle, 3 = back, 4 = forward, as `mouse2`…`mouse4` name them) of an
/// evdev button code. Left and right clicks are never triggers.
pub fn evdev_button(code: u16) -> Option<u16> {
    match code {
        0x112 => Some(2),         // BTN_MIDDLE
        0x113 | 0x116 => Some(3), // BTN_SIDE, BTN_BACK
        0x114 | 0x115 => Some(4), // BTN_EXTRA, BTN_FORWARD
        0x117 => Some(5),         // BTN_TASK
        _ => None,
    }
}

/// Trigger number of an X11 button (2 = middle, 8 = back, 9 = forward; 4–7 are the wheel).
pub fn x11_button(button: u32) -> Option<u16> {
    match button {
        2 => Some(2),
        8 => Some(3),
        9 => Some(4),
        _ => None,
    }
}

/// The X11 button of a trigger number (for the grab), the other way round.
fn x11_button_of(trigger: u16) -> Option<u32> {
    (1..=9).find(|&b| x11_button(b) == Some(trigger))
}

/// An evdev `input_event` (type, code, value) as a hook event: `EV_KEY` only, value 1 = down,
/// 2 = auto-repeat (a down again, as Windows sends it), 0 = up.
pub fn evdev_event(kind: u16, code: u16, value: i32) -> Option<Event> {
    const EV_KEY: u16 = 1;
    if kind != EV_KEY {
        return None;
    }
    let down = value != 0;
    if let Some(button) = evdev_button(code) {
        return Some(match down {
            true => Event::MouseDown(button),
            false => Event::MouseUp(button),
        });
    }
    let vk = vk(code)?;
    Some(match down {
        true => Event::KeyDown(vk),
        false => Event::KeyUp(vk),
    })
}

/// What X11's raw input events (XInput2) report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raw {
    KeyPress,
    KeyRelease,
    ButtonPress,
    ButtonRelease,
}

/// An XInput2 raw event (its kind and `detail`: X keycode or button) as a hook event.
pub fn x11_event(kind: Raw, detail: u32) -> Option<Event> {
    match kind {
        Raw::KeyPress | Raw::KeyRelease => {
            let code = u16::try_from(detail)
                .ok()?
                .checked_sub(X11_KEYCODE_OFFSET)?;
            evdev_event(1, code, (kind == Raw::KeyPress) as i32)
        }
        Raw::ButtonPress => x11_button(detail).map(Event::MouseDown),
        Raw::ButtonRelease => x11_button(detail).map(Event::MouseUp),
    }
}

/// The evdev code of a key as the triggers number it (macOS key code), for the X11 grab.
pub fn evdev_code(keycode: u16) -> Option<u16> {
    (1..0x100).find(|&code| vk(code).and_then(crate::hook::keycode) == Some(keycode))
}

/// X11 modifier masks (`ShiftMask`, `ControlMask`, `Mod1Mask` = Alt, `Mod4Mask` = Super).
pub mod x11_mask {
    pub const SHIFT: u32 = 0x01;
    pub const LOCK: u32 = 0x02;
    pub const CONTROL: u32 = 0x04;
    pub const ALT: u32 = 0x08;
    pub const NUM_LOCK: u32 = 0x10;
    pub const SUPER: u32 = 0x40;
    pub const ANY: u32 = 1 << 15;
}

/// A passive X11 grab: the key or button then goes to us alone, not to the focused app (what the
/// Mac's tap and the Windows hook do by swallowing it). Modifier triggers get none: they pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// X keycode and the modifier mask (Caps Lock and Num Lock not included: grab each of
    /// [`lock_variants`] too).
    Key { keycode: u32, modifiers: u32 },
    /// X button, with any modifiers.
    Button(u32),
}

/// The grabs for `triggers` (each once).
pub fn grabs(triggers: &[Trigger]) -> Vec<Grab> {
    let mut out: Vec<Grab> = Vec::new();
    for trigger in triggers {
        let grab = match trigger.input {
            Input::Key { keycode, modifiers } => evdev_code(keycode).map(|code| Grab::Key {
                keycode: (code + X11_KEYCODE_OFFSET) as u32,
                modifiers: x11_modifiers(modifiers),
            }),
            Input::Mouse { button } => x11_button_of(button).map(Grab::Button),
            Input::Modifier { .. } | Input::Chord { .. } => None,
        };
        if let Some(grab) = grab
            && !out.contains(&grab)
        {
            out.push(grab);
        }
    }
    out
}

fn x11_modifiers(modifiers: u64) -> u32 {
    [
        (flags::SHIFT, x11_mask::SHIFT),
        (flags::CONTROL, x11_mask::CONTROL),
        (flags::OPTION, x11_mask::ALT),
        (flags::COMMAND, x11_mask::SUPER),
    ]
    .iter()
    .filter(|(flag, _)| modifiers & flag != 0)
    .fold(0, |mask, (_, x)| mask | x)
}

/// A key grab holds only for exactly its modifiers; Caps Lock and Num Lock count as modifiers in
/// X11, so a grab is made four times.
pub fn lock_variants(modifiers: u32) -> [u32; 4] {
    use x11_mask::{LOCK, NUM_LOCK};
    [
        modifiers,
        modifiers | LOCK,
        modifiers | NUM_LOCK,
        modifiers | LOCK | NUM_LOCK,
    ]
}

/// The autostart entry (`~/.config/autostart/dictate.desktop`, XDG Autostart) that starts
/// `exe` at login; with `ort` (`ORT_DYLIB_PATH` when the user set it) through `env`, since a
/// login starts it without the shell's variables. ONNX Runtime from `dictate model download
/// onnxruntime` (the data folder) or next to `exe` is found without it (`onnxruntime.rs`).
pub fn autostart_entry(exe: &Path, ort: Option<&Path>) -> String {
    let mut command = Vec::new();
    if let Some(ort) = ort {
        command.push("env".to_string());
        command.push(format!("ORT_DYLIB_PATH={}", ort.to_string_lossy()));
    }
    command.push(exe.to_string_lossy().into_owned());
    let exec: Vec<String> = command.iter().map(|a| desktop_exec_arg(a)).collect();
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Dictate\n\
         Comment=Local dictation\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exec.join(" ")
    )
}

/// One argument of a desktop entry's `Exec` key (Desktop Entry Specification, "The Exec key"):
/// as it is when that is safe, else in `"` with `"`, `` ` ``, `$` and `\` escaped by a backslash;
/// then every backslash doubled (the escape of string values, applied first when read), and `%`
/// doubled (field codes).
fn desktop_exec_arg(arg: &str) -> String {
    let arg = arg.replace('%', "%%");
    let plain = |c: char| c.is_ascii_alphanumeric() || "/._-+,:@=".contains(c);
    if !arg.is_empty() && arg.chars().all(plain) {
        return arg;
    }
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

/// The user's languages, most preferred first ("de-DE", "en"), from the environment as gettext
/// reads it: `LANGUAGE` (a `:` list), else the first of `LC_ALL`, `LC_MESSAGES`, `LANG` that is
/// set. `C` and `POSIX` are no language. `get` reads a variable.
pub fn preferred_languages(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let tidy = |locale: &str| -> Option<String> {
        // "de_DE.UTF-8@euro" → "de-DE"
        let locale = locale.split(['.', '@']).next()?.trim();
        if locale.is_empty() || locale == "C" || locale == "POSIX" {
            return None;
        }
        Some(locale.replace('_', "-"))
    };
    let set = |name: &str| get(name).filter(|v| !v.trim().is_empty());
    // LANGUAGE counts only with a real locale set (gettext ignores it under C).
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| set(name));
    if let (Some(list), Some(_)) = (set("LANGUAGE"), &locale) {
        let languages: Vec<String> = list.split(':').filter_map(tidy).collect();
        if !languages.is_empty() {
            return languages;
        }
    }
    locale.as_deref().and_then(tidy).into_iter().collect()
}

/// The system for the usage report: `PRETTY_NAME` from `/etc/os-release` ("Ubuntu 24.04.1 LTS"),
/// else `NAME VERSION_ID`, else "Linux"; the kernel release after it when known.
pub fn os_name(os_release: &str, kernel: &str) -> String {
    let field = |key: &str| {
        os_release.lines().find_map(|line| {
            let value = line.trim().strip_prefix(key)?.strip_prefix('=')?;
            let value = value.trim().trim_matches('"').trim_matches('\'');
            (!value.is_empty()).then(|| value.to_string())
        })
    };
    let name = field("PRETTY_NAME")
        .or_else(|| {
            let name = field("NAME")?;
            Some(match field("VERSION_ID") {
                Some(version) => format!("{name} {version}"),
                None => name,
            })
        })
        .unwrap_or_else(|| "Linux".into());
    match kernel.trim() {
        "" => name,
        kernel => format!("{name} (Linux {kernel})"),
    }
}

/// Which input the session gives us: `XDG_SESSION_TYPE` (and `WAYLAND_DISPLAY`, `DISPLAY`);
/// `DICTATE_INPUT=x11|evdev` overrides (docs/linux.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    /// An X11 session: XInput2 for the triggers, XTest for pasting, no permission needed.
    X11,
    /// Wayland (or no display): the triggers from `/dev/input` (group `input`), pasting through
    /// `ydotool` or `wtype`.
    Wayland,
}

pub fn session(get: impl Fn(&str) -> Option<String>) -> Session {
    let set = |name: &str| get(name).filter(|v| !v.trim().is_empty());
    match set("DICTATE_INPUT").as_deref().map(str::trim) {
        Some("x11") => return Session::X11,
        Some("evdev" | "wayland") => return Session::Wayland,
        _ => {}
    }
    match set("XDG_SESSION_TYPE").as_deref() {
        Some("x11") => Session::X11,
        Some("wayland") => Session::Wayland,
        _ if set("WAYLAND_DISPLAY").is_some() => Session::Wayland,
        _ if set("DISPLAY").is_some() => Session::X11,
        _ => Session::Wayland,
    }
}

/// Whether the desktop shows the tray icon. libayatana-appindicator hands the icon to a
/// StatusNotifier watcher (`org.kde.StatusNotifierWatcher` on the session bus), which a panel
/// draws when it has registered as a host (`IsStatusNotifierHostRegistered`); without a watcher
/// it falls back to an XEmbed tray (X11: the owner of `_NET_SYSTEM_TRAY_S<screen>`). GNOME has
/// neither without an extension (docs/linux.md): then nothing of Dictate is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tray {
    Shown,
    Missing,
}

impl Tray {
    /// From what the desktop answered; `None` where the question failed (no session bus, no X
    /// display, a timeout), which counts as no.
    pub fn from_probe(
        watcher_owned: Option<bool>,
        host_registered: Option<bool>,
        xembed_tray: Option<bool>,
    ) -> Tray {
        let notifier = watcher_owned == Some(true) && host_registered == Some(true);
        // The XEmbed fallback is used only when no watcher is there to take the icon.
        let xembed = watcher_owned != Some(true) && xembed_tray == Some(true);
        match notifier || xembed {
            true => Tray::Shown,
            false => Tray::Missing,
        }
    }

    /// At login the panel may come up after Dictate: asks `probe` up to `tries` times, with
    /// `pause` between, before calling the tray missing.
    pub fn settle(tries: u32, mut probe: impl FnMut() -> Tray, mut pause: impl FnMut()) -> Tray {
        for i in 0..tries.max(1) {
            if i > 0 {
                pause();
            }
            if probe() == Tray::Shown {
                return Tray::Shown;
            }
        }
        Tray::Missing
    }

    /// Without a tray the settings window opens at start: else nothing would be on screen, and
    /// neither the settings nor "Beenden" could be reached.
    pub fn open_window_at_start(self) -> bool {
        self == Tray::Missing
    }

    /// Without a tray closing the window quits: else the app would run on unseen.
    pub fn quit_on_close(self) -> bool {
        self == Tray::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook::{Hook, Out};
    use crate::trigger::{Mode, parse_trigger};

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string())
        }
    }

    fn triggers(specs: &[&str]) -> Vec<Trigger> {
        specs
            .iter()
            .map(|s| parse_trigger(s, Mode::Hybrid).unwrap())
            .collect()
    }

    #[test]
    fn evdev_keys_become_the_keys_the_triggers_name() {
        let mac = |code| vk(code).and_then(crate::hook::keycode);
        assert_eq!(mac(30), crate::trigger::keycode("a")); // KEY_A
        assert_eq!(mac(44), crate::trigger::keycode("z")); // KEY_Z (Y on German keyboards)
        assert_eq!(mac(11), crate::trigger::keycode("0"));
        assert_eq!(mac(6), crate::trigger::keycode("5"));
        assert_eq!(mac(57), crate::trigger::keycode("space"));
        assert_eq!(mac(59), crate::trigger::keycode("f1"));
        assert_eq!(mac(88), crate::trigger::keycode("f12"));
        assert_eq!(mac(183), crate::trigger::keycode("f13"));
        assert_eq!(mac(190), crate::trigger::keycode("f20"));
        assert_eq!(vk(1), Some(crate::hook::VK_ESCAPE));
        // Every key is "another key"; buttons are not keys.
        assert_eq!(vk(105), Some(0x100 + 105)); // KEY_LEFT
        assert_eq!(mac(105), None);
        assert_eq!(vk(0x110), None); // BTN_LEFT
        assert_eq!(vk(0x14a), None); // BTN_TOUCH
        assert_eq!(vk(0x160), Some(0x100 + 0x160)); // KEY_OK
    }

    #[test]
    fn evdev_events_drive_the_hook_like_windows_hooks() {
        let mut hook = Hook::new(triggers(&["right-alt", "f13", "mouse3"]));
        let mut feed = |kind, code, value| {
            let mut out = Vec::new();
            let event = evdev_event(kind, code, value).unwrap();
            hook.feed(event, &mut out);
            out
        };
        assert_eq!(feed(1, 100, 1), vec![Out::Press(0)]); // right Alt down
        assert_eq!(feed(1, 100, 2), vec![]); // auto-repeat
        assert_eq!(feed(1, 16, 1), vec![Out::Interrupt(0)]); // AltGr+Q: @, not dictation
        assert_eq!(feed(1, 16, 0), vec![]);
        assert_eq!(feed(1, 100, 0), vec![Out::Release(0)]);
        assert_eq!(feed(1, 183, 1), vec![Out::Press(1)]); // F13
        assert_eq!(feed(1, 183, 0), vec![Out::Release(1)]);
        assert_eq!(feed(1, 0x113, 1), vec![Out::Press(2)]); // BTN_SIDE
        assert_eq!(feed(1, 0x113, 0), vec![Out::Release(2)]);
        // Not keys: relative motion, sync, left click.
        assert_eq!(evdev_event(2, 0, 5), None);
        assert_eq!(evdev_event(0, 0, 0), None);
        assert_eq!(evdev_event(1, 0x110, 1), None);
    }

    #[test]
    fn x11_raw_events_are_evdev_plus_eight() {
        assert_eq!(
            x11_event(Raw::KeyPress, 108), // Alt_R / ISO_Level3_Shift
            Some(Event::KeyDown(0xA5))
        );
        assert_eq!(x11_event(Raw::KeyRelease, 191), Some(Event::KeyUp(0x7C))); // F13
        assert_eq!(x11_event(Raw::ButtonPress, 8), Some(Event::MouseDown(3)));
        assert_eq!(x11_event(Raw::ButtonRelease, 9), Some(Event::MouseUp(4)));
        assert_eq!(x11_event(Raw::ButtonPress, 4), None); // wheel
        assert_eq!(x11_event(Raw::ButtonPress, 1), None);
        assert_eq!(x11_event(Raw::KeyPress, 3), None); // below the offset
    }

    #[test]
    fn grabs_cover_keys_and_buttons_but_not_lone_modifiers() {
        let grabs = grabs(&triggers(&[
            "right-alt",
            "f13",
            "shift+ctrl+5",
            "cmd+alt+d",
            "mouse3",
            "mouse4@toggle",
            "f13@toggle",
        ]));
        assert_eq!(
            grabs,
            vec![
                Grab::Key {
                    keycode: 191,
                    modifiers: 0
                },
                Grab::Key {
                    keycode: 14, // KEY_5 + 8
                    modifiers: x11_mask::SHIFT | x11_mask::CONTROL
                },
                Grab::Key {
                    keycode: 40, // KEY_D + 8
                    modifiers: x11_mask::ALT | x11_mask::SUPER
                },
                Grab::Button(8),
                Grab::Button(9),
            ]
        );
        assert_eq!(lock_variants(x11_mask::SHIFT), [0x01, 0x03, 0x11, 0x13]);
    }

    #[test]
    fn every_trigger_key_has_an_evdev_code() {
        for name in ["a", "z", "0", "9", "space", "f1", "f12", "f13", "f20"] {
            let code = crate::trigger::keycode(name).unwrap();
            let evdev = evdev_code(code).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(vk(evdev).and_then(crate::hook::keycode), Some(code));
        }
    }

    #[test]
    fn autostart_entry_quotes_the_path_when_needed() {
        let plain = autostart_entry(Path::new("/home/ada/.local/bin/dictate"), None);
        assert!(plain.starts_with("[Desktop Entry]\n"));
        assert!(plain.contains("\nExec=/home/ada/.local/bin/dictate\n"));
        assert!(plain.contains("\nType=Application\n"));
        // In the file: \\$ (the string escape of \$, which quotes the $).
        let spaced = autostart_entry(Path::new("/opt/My Apps/dic$tate"), None);
        assert!(
            spaced.contains("\nExec=\"/opt/My Apps/dic\\\\$tate\"\n"),
            "{spaced}"
        );
        assert_eq!(desktop_exec_arg("/a/100%"), "\"/a/100%%\"");
        assert_eq!(desktop_exec_arg("a\\b"), "\"a\\\\\\\\b\"");
        let ort = autostart_entry(
            Path::new("/usr/bin/dictate"),
            Some(Path::new("/opt/ort/lib/libonnxruntime.so")),
        );
        assert!(
            ort.contains(
                "\nExec=env ORT_DYLIB_PATH=/opt/ort/lib/libonnxruntime.so /usr/bin/dictate\n"
            ),
            "{ort}"
        );
    }

    #[test]
    fn languages_from_the_environment() {
        assert_eq!(
            preferred_languages(env(&[("LANG", "de_DE.UTF-8")])),
            vec!["de-DE"]
        );
        assert_eq!(
            preferred_languages(env(&[("LANGUAGE", "en_GB:de"), ("LANG", "de_DE.UTF-8")])),
            vec!["en-GB", "de"]
        );
        // LC_ALL wins over LANG; C is no language; LANGUAGE alone under C is ignored.
        assert_eq!(
            preferred_languages(env(&[("LC_ALL", "de_AT@euro"), ("LANG", "en_US")])),
            vec!["de-AT"]
        );
        assert!(preferred_languages(env(&[("LANG", "C.UTF-8")])).is_empty());
        assert!(preferred_languages(env(&[("LANGUAGE", "de")])).is_empty());
        assert!(preferred_languages(env(&[])).is_empty());
    }

    #[test]
    fn os_name_from_os_release() {
        let ubuntu = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n";
        assert_eq!(
            os_name(ubuntu, "6.8.0-47-generic\n"),
            "Ubuntu 24.04.1 LTS (Linux 6.8.0-47-generic)"
        );
        assert_eq!(os_name("NAME=Arch\nVERSION_ID=rolling", ""), "Arch rolling");
        assert_eq!(os_name("", "6.1"), "Linux (Linux 6.1)");
    }

    #[test]
    fn session_from_the_environment() {
        assert_eq!(
            session(env(&[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")])),
            Session::Wayland
        );
        assert_eq!(session(env(&[("XDG_SESSION_TYPE", "x11")])), Session::X11);
        assert_eq!(session(env(&[("DISPLAY", ":0")])), Session::X11);
        assert_eq!(
            session(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")])),
            Session::Wayland
        );
        assert_eq!(
            session(env(&[
                ("XDG_SESSION_TYPE", "wayland"),
                ("DICTATE_INPUT", "x11")
            ])),
            Session::X11
        );
        assert_eq!(session(env(&[])), Session::Wayland);
    }

    #[test]
    fn tray_needs_a_registered_host_or_an_xembed_tray() {
        use Tray::*;
        // KDE, Ubuntu's GNOME, XFCE: a watcher with a host.
        assert_eq!(Tray::from_probe(Some(true), Some(true), None), Shown);
        // GNOME without the extension: no watcher, no XEmbed tray.
        assert_eq!(Tray::from_probe(Some(false), None, Some(false)), Missing);
        assert_eq!(Tray::from_probe(Some(false), None, None), Missing);
        // A watcher without a panel (its host gone): the icon goes there and nobody draws it,
        // even where an XEmbed tray would have shown it.
        assert_eq!(
            Tray::from_probe(Some(true), Some(false), Some(true)),
            Missing
        );
        assert_eq!(Tray::from_probe(Some(true), None, None), Missing);
        // i3bar, tint2, stalonetray: no watcher, the XEmbed fallback.
        assert_eq!(Tray::from_probe(Some(false), None, Some(true)), Shown);
        assert_eq!(Tray::from_probe(None, None, Some(true)), Shown);
        // No session bus and no X display: nothing to show it.
        assert_eq!(Tray::from_probe(None, None, None), Missing);
    }

    #[test]
    fn without_a_tray_the_window_opens_and_closing_it_quits() {
        assert!(Tray::Missing.open_window_at_start());
        assert!(Tray::Missing.quit_on_close());
        assert!(!Tray::Shown.open_window_at_start());
        assert!(!Tray::Shown.quit_on_close());
    }

    #[test]
    fn tray_settles_when_the_panel_comes_up_late() {
        let answers = [Tray::Missing, Tray::Missing, Tray::Shown];
        let (mut asked, mut paused) = (0, 0);
        let tray = Tray::settle(
            5,
            || {
                asked += 1;
                answers[asked - 1]
            },
            || paused += 1,
        );
        assert_eq!((tray, asked, paused), (Tray::Shown, 3, 2));

        let (mut asked, mut paused) = (0, 0);
        let tray = Tray::settle(
            4,
            || {
                asked += 1;
                Tray::Missing
            },
            || paused += 1,
        );
        assert_eq!((tray, asked, paused), (Tray::Missing, 4, 3));
        // Asks at least once.
        assert_eq!(Tray::settle(0, || Tray::Shown, || {}), Tray::Shown);
    }
}
