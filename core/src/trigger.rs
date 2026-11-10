//! Triggers (which key or mouse button starts a recording) and the press/release state machine.
//! Platform independent so it can be unit tested; the macOS event tap feeds it.

use std::time::Duration;

use anyhow::{Context, Result, bail};

/// How long a press must last in hybrid mode to count as hold-to-talk.
pub const HYBRID_HOLD: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Hold to talk, release to stop.
    Hold,
    /// Press to start, press again to stop.
    Toggle,
    /// A short tap records hands-free until the next press; holding records until release.
    Hybrid,
}

impl Mode {
    /// The name `FromStr` accepts.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Hold => "hold",
            Mode::Toggle => "toggle",
            Mode::Hybrid => "hybrid",
        }
    }
}

impl std::str::FromStr for Mode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "hold" => Ok(Mode::Hold),
            "toggle" => Ok(Mode::Toggle),
            "hybrid" => Ok(Mode::Hybrid),
            _ => bail!("unknown mode {s:?} (hold, toggle, hybrid)"),
        }
    }
}

/// Modifier keys a combo can require. Bits match CGEventFlags.
pub mod flags {
    pub const SHIFT: u64 = 0x0002_0000;
    pub const CONTROL: u64 = 0x0004_0000;
    pub const OPTION: u64 = 0x0008_0000;
    pub const COMMAND: u64 = 0x0010_0000;
    pub const FN: u64 = 0x0080_0000;
    pub const ALL: u64 = SHIFT | CONTROL | OPTION | COMMAND;
    /// What a modifier combo ([`super::Input::Chord`]) can hold: the four and Fn.
    pub const CHORD: u64 = ALL | FN;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A modifier key on its own (Fn, right ⌥, …), seen as flag changes.
    /// `mask` is the device-dependent flag bit that is set while the key is down.
    Modifier { keycode: u16, mask: u64 },
    /// A regular key, optionally with modifiers held (⌃⌥⌘D, F18, …).
    Key { keycode: u16, modifiers: u64 },
    /// Two or more modifiers held together, no other key (⌥⌘Fn): pressed while exactly these
    /// are down (`modifiers`, bits of [`flags::CHORD`]), released when one is let go.
    Chord { modifiers: u64 },
    /// A mouse button by number (2 = middle, 3 = back, 4 = forward).
    Mouse { button: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub input: Input,
    pub mode: Mode,
}

/// Parses `fn`, `right-option`, `f18`, `ctrl+alt+cmd+d`, `alt+cmd+fn` or `mouse3`, with an optional
/// `@hold|@toggle|@hybrid` suffix that overrides `default_mode`.
pub fn parse_trigger(spec: &str, default_mode: Mode) -> Result<Trigger> {
    let (input, mode) = match spec.split_once('@') {
        Some((input, mode)) => (input, mode.parse()?),
        None => (spec, default_mode),
    };
    let input = input.trim().to_ascii_lowercase();
    Ok(Trigger {
        input: parse_input(&input).with_context(|| format!("trigger {spec:?}"))?,
        mode,
    })
}

fn parse_input(s: &str) -> Result<Input> {
    if let Some(n) = s.strip_prefix("mouse") {
        let button: u16 = n.parse().context("mouse button number")?;
        if !(2..=31).contains(&button) {
            bail!("mouse buttons 0 and 1 are left/right click; use 2…31");
        }
        return Ok(Input::Mouse { button });
    }
    if let Some(input) = modifier(s) {
        return Ok(input);
    }
    if s.split('+').all(|part| chord_flag(part).is_some()) {
        let modifiers = s.split('+').filter_map(chord_flag).fold(0, |m, f| m | f);
        if modifiers.count_ones() < 2 {
            bail!("a modifier on its own: fn or right-…; a combo of modifiers needs two of them");
        }
        return Ok(Input::Chord { modifiers });
    }
    let mut modifiers = 0;
    let mut parts: Vec<&str> = s.split('+').collect();
    let key = parts.pop().unwrap_or_default();
    for part in parts {
        modifiers |= match part {
            "shift" => flags::SHIFT,
            "ctrl" | "control" => flags::CONTROL,
            "alt" | "opt" | "option" => flags::OPTION,
            "cmd" | "command" => flags::COMMAND,
            _ => bail!("unknown modifier {part:?}"),
        };
    }
    let keycode = keycode(key).with_context(|| format!("unknown key {key:?}"))?;
    Ok(Input::Key { keycode, modifiers })
}

/// Modifier keys that work as a trigger on their own: name, keycode and the device-dependent
/// mask from IOKit's NX_DEVICE*KEYMASK.
const MODIFIERS: &[(&str, u16, u64)] = &[
    ("fn", 63, flags::FN),
    ("right-option", 61, 0x40),
    ("right-command", 54, 0x10),
    ("right-control", 62, 0x2000),
    ("right-shift", 60, 0x04),
];

/// The flag of a modifier name in a combo (`alt+cmd+fn`).
fn chord_flag(s: &str) -> Option<u64> {
    Some(match s {
        "shift" => flags::SHIFT,
        "ctrl" | "control" => flags::CONTROL,
        "alt" | "opt" | "option" => flags::OPTION,
        "cmd" | "command" => flags::COMMAND,
        "fn" | "globe" => flags::FN,
        _ => return None,
    })
}

/// `modifiers` as a spec's names in the usual order: `shift+alt+cmd+fn`.
fn modifier_names(modifiers: u64) -> Vec<&'static str> {
    [
        (flags::SHIFT, "shift"),
        (flags::CONTROL, "ctrl"),
        (flags::OPTION, "alt"),
        (flags::COMMAND, "cmd"),
        (flags::FN, "fn"),
    ]
    .into_iter()
    .filter(|(bit, _)| modifiers & bit != 0)
    .map(|(_, name)| name)
    .collect()
}

fn modifier(s: &str) -> Option<Input> {
    let s = match s {
        "globe" => "fn",
        "right-alt" => "right-option",
        "right-cmd" => "right-command",
        "right-ctrl" => "right-control",
        s => s,
    };
    MODIFIERS
        .iter()
        .find(|(name, _, _)| *name == s)
        .map(|&(_, keycode, mask)| Input::Modifier { keycode, mask })
}

/// macOS virtual key codes (positions on an ANSI keyboard; on a German layout Y and Z swap).
const KEYS: &[(&str, u16)] = &[
    ("a", 0),
    ("s", 1),
    ("d", 2),
    ("f", 3),
    ("h", 4),
    ("g", 5),
    ("z", 6),
    ("x", 7),
    ("c", 8),
    ("v", 9),
    ("b", 11),
    ("q", 12),
    ("w", 13),
    ("e", 14),
    ("r", 15),
    ("y", 16),
    ("t", 17),
    ("1", 18),
    ("2", 19),
    ("3", 20),
    ("4", 21),
    ("6", 22),
    ("5", 23),
    ("9", 25),
    ("7", 26),
    ("8", 28),
    ("0", 29),
    ("o", 31),
    ("u", 32),
    ("i", 34),
    ("p", 35),
    ("l", 37),
    ("j", 38),
    ("k", 40),
    ("n", 45),
    ("m", 46),
    ("space", 49),
    ("f1", 122),
    ("f2", 120),
    ("f3", 99),
    ("f4", 118),
    ("f5", 96),
    ("f6", 97),
    ("f7", 98),
    ("f8", 100),
    ("f9", 101),
    ("f10", 109),
    ("f11", 103),
    ("f12", 111),
    ("f13", 105),
    ("f14", 107),
    ("f15", 113),
    ("f16", 106),
    ("f17", 64),
    ("f18", 79),
    ("f19", 80),
    ("f20", 90),
];

pub(crate) fn keycode(key: &str) -> Option<u16> {
    KEYS.iter()
        .find(|(name, _)| *name == key)
        .map(|(_, code)| *code)
}

fn key_name(code: u16) -> Option<&'static str> {
    KEYS.iter().find(|(_, c)| *c == code).map(|(name, _)| *name)
}

/// What the recorder should do after an input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    /// Throw the recording away without transcribing.
    Discard,
    None,
}

/// Turns presses and releases of the triggers into start/stop actions: hold, toggle and hybrid
/// (a tap starts hands-free, a long press is push-to-talk).
#[derive(Debug, Default)]
pub struct Machine {
    recording: bool,
    /// The trigger whose press started the current recording and when, until it is released.
    starter: Option<(usize, Duration)>,
}

impl Machine {
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// `at` is any monotonic timestamp (event time).
    pub fn press(&mut self, trigger: usize, _mode: Mode, at: Duration) -> Action {
        if self.recording {
            // A press while recording (hands-free, or another trigger) stops right away.
            self.recording = false;
            self.starter = None;
            return Action::Stop;
        }
        self.recording = true;
        self.starter = Some((trigger, at));
        Action::Start
    }

    pub fn release(&mut self, trigger: usize, mode: Mode, at: Duration) -> Action {
        let Some((starter, pressed_at)) = self.starter else {
            return Action::None;
        };
        if starter != trigger {
            return Action::None;
        }
        self.starter = None;
        if !self.recording {
            return Action::None;
        }
        let held = at.saturating_sub(pressed_at);
        match mode {
            Mode::Hold => self.stop(),
            Mode::Hybrid if held >= HYBRID_HOLD => self.stop(),
            // A short hybrid tap keeps recording hands-free until the next press.
            Mode::Hybrid | Mode::Toggle => Action::None,
        }
    }

    /// Another key went down while `trigger` was held. Within the first second after the press
    /// that started the recording this was a shortcut like Fn+← or ⌥⌘5+X, not dictation:
    /// discard the recording (an accidental start). Mouse triggers never call this.
    pub fn interrupt(&mut self, trigger: usize, at: Duration) -> Action {
        match self.starter {
            Some((starter, pressed_at))
                if starter == trigger
                    && self.recording
                    && at.saturating_sub(pressed_at) <= INTERRUPT_WINDOW =>
            {
                self.recording = false;
                self.starter = None;
                Action::Discard
            }
            _ => Action::None,
        }
    }

    /// A start or stop from outside the triggers (the local server): hands-free, nothing to release.
    pub fn toggle(&mut self) -> Action {
        self.starter = None;
        self.recording = !self.recording;
        if self.recording {
            Action::Start
        } else {
            Action::Stop
        }
    }

    /// Esc or an error: forget the recording without transcribing.
    pub fn cancel(&mut self) -> bool {
        let was = self.recording;
        self.recording = false;
        self.starter = None;
        was
    }

    fn stop(&mut self) -> Action {
        self.recording = false;
        Action::Stop
    }
}

/// What a modifier change did to the triggers (see [`modifiers_changed`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Press(usize),
    Release(usize),
    /// A combo of modifiers took over while this trigger was held (Fn, then ⌥⌘ for ⌥⌘Fn).
    Interrupt(usize),
}

/// Modifier flags changed: `keycode` is the modifier key, `flags` the flags after it
/// (CGEventFlags, device bits included). Updates `down` (which triggers are held) and says what
/// happened, in order. Lone modifiers (`fn`, `right-option`) follow their key; combos
/// (`alt+cmd+fn`) are pressed while exactly their modifiers are down. A combo wins over a lone
/// modifier that is part of it: ⌥⌘ then Fn presses `alt+cmd+fn`, not `fn`; Fn then ⌥⌘
/// interrupts `fn` first (the machine throws that start away) and presses the combo.
pub fn modifiers_changed(
    triggers: &[Trigger],
    down: &mut [bool],
    keycode: u16,
    flags: u64,
) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut presses = Vec::new();
    for (i, trigger) in triggers.iter().enumerate() {
        if let Input::Chord { modifiers } = trigger.input {
            let pressed = flags & flags::CHORD == modifiers;
            if pressed != down[i] {
                down[i] = pressed;
                match pressed {
                    true => presses.push(i),
                    false => changes.push(Change::Release(i)),
                }
            }
        }
    }
    let chord = !presses.is_empty();
    if chord {
        for (i, trigger) in triggers.iter().enumerate() {
            if down[i] && !presses.contains(&i) && !matches!(trigger.input, Input::Mouse { .. }) {
                changes.push(Change::Interrupt(i));
            }
        }
    }
    for (i, trigger) in triggers.iter().enumerate() {
        if let Input::Modifier {
            keycode: code,
            mask,
        } = trigger.input
            && code == keycode
        {
            let pressed = flags & mask != 0;
            if pressed == down[i] || (pressed && chord) {
                continue;
            }
            down[i] = pressed;
            match pressed {
                true => presses.push(i),
                false => changes.push(Change::Release(i)),
            }
        }
    }
    changes.extend(presses.into_iter().map(Change::Press));
    changes
}

/// How long after a press another key still counts as part of a shortcut.
pub const INTERRUPT_WINDOW: Duration = Duration::from_secs(1);

/// What the settings window's "Aufnehmen" sees while it waits for the new trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seen {
    /// Modifier flags changed (`keycode` is the modifier key, `flags` the CGEventFlags after it).
    Flags {
        keycode: u16,
        flags: u64,
    },
    KeyDown {
        keycode: u16,
        flags: u64,
    },
    /// A mouse button other than left/right went down.
    Mouse {
        button: u16,
    },
}

/// The end of a capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Captured {
    /// A spec [`parse_trigger`] accepts.
    Spec(String),
    /// Esc.
    Cancelled,
    /// A key Dictate has no name for.
    Unknown(u16),
}

/// Records a trigger by pressing it: keys and combos on key down, mouse buttons on press, a
/// lone modifier (Fn, right ⌥) or a combo of modifiers (⌥⌘Fn) on the first release when nothing
/// else came in between.
#[derive(Debug, Default)]
pub struct Capture {
    /// A trigger-capable modifier that is down, by keycode.
    lone: Option<u16>,
    /// The modifiers held now and the most held together since the last time none was.
    held: u64,
    most: u64,
}

const KEY_ESCAPE: u16 = 53;

impl Capture {
    pub fn feed(&mut self, seen: Seen) -> Option<Captured> {
        match seen {
            Seen::KeyDown { keycode, flags } => {
                // A key came: what is held is part of a combo with it, not a combo of modifiers.
                self.lone = None;
                self.held = flags & flags::CHORD;
                self.most = 0;
                let modifiers = flags & flags::ALL;
                if keycode == KEY_ESCAPE && modifiers == 0 {
                    return Some(Captured::Cancelled);
                }
                let Some(key) = key_name(keycode) else {
                    return Some(Captured::Unknown(keycode));
                };
                let mut names = modifier_names(modifiers);
                names.push(key);
                Some(Captured::Spec(names.join("+")))
            }
            Seen::Mouse { button } if button >= 2 => Some(Captured::Spec(format!("mouse{button}"))),
            Seen::Mouse { .. } => None,
            Seen::Flags { keycode, flags } => {
                let held = flags & flags::CHORD;
                let let_go = self.held & !held != 0;
                self.held = held;
                self.most |= held;
                if let_go && self.most.count_ones() >= 2 {
                    // Two or more held together, nothing else, now one goes: that combo.
                    return Some(Captured::Spec(modifier_names(self.most).join("+")));
                }
                if held == 0 {
                    self.most = 0;
                }
                let known = MODIFIERS.iter().find(|(_, code, _)| *code == keycode);
                match (known, self.lone) {
                    // Pressed on its own: wait for the release.
                    (Some(&(_, _, mask)), None) if flags & mask != 0 => {
                        self.lone = Some(keycode);
                        None
                    }
                    // Released without anything in between: that is the trigger.
                    (Some(&(name, _, mask)), Some(lone))
                        if lone == keycode && flags & mask == 0 =>
                    {
                        self.lone = None;
                        Some(Captured::Spec(name.to_string()))
                    }
                    // Another modifier joined: a combo is coming, not a lone modifier.
                    _ => {
                        self.lone = None;
                        None
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn toggle_from_outside_is_hands_free() {
        let mut m = Machine::default();
        assert_eq!(m.toggle(), Action::Start);
        assert!(m.is_recording());
        // a key released later must not stop it
        assert_eq!(m.release(0, Mode::Hold, ms(900)), Action::None);
        assert_eq!(m.toggle(), Action::Stop);
        assert!(!m.is_recording());
        // a press of a trigger stops it as well
        assert_eq!(m.toggle(), Action::Start);
        assert_eq!(m.press(0, Mode::Hold, ms(10)), Action::Stop);
    }

    #[test]
    fn parses_specs() {
        let t = parse_trigger("fn", Mode::Hybrid).unwrap();
        assert_eq!(
            t.input,
            Input::Modifier {
                keycode: 63,
                mask: flags::FN
            }
        );
        assert_eq!(t.mode, Mode::Hybrid);
        let t = parse_trigger("mouse3@toggle", Mode::Hybrid).unwrap();
        assert_eq!(
            t,
            Trigger {
                input: Input::Mouse { button: 3 },
                mode: Mode::Toggle
            }
        );
        let t = parse_trigger("Ctrl+Alt+Cmd+D", Mode::Hold).unwrap();
        assert_eq!(
            t.input,
            Input::Key {
                keycode: 2,
                modifiers: flags::CONTROL | flags::OPTION | flags::COMMAND
            }
        );
        assert_eq!(
            parse_trigger("f18", Mode::Hold).unwrap().input,
            Input::Key {
                keycode: 79,
                modifiers: 0
            }
        );
        assert_eq!(
            parse_trigger("shift+alt+cmd+5", Mode::Hybrid)
                .unwrap()
                .input,
            Input::Key {
                keycode: 23,
                modifiers: flags::SHIFT | flags::OPTION | flags::COMMAND
            }
        );
        assert!(parse_trigger("mouse1", Mode::Hold).is_err());
        assert!(parse_trigger("hyper+x", Mode::Hold).is_err());
        assert!(parse_trigger("fn@sometimes", Mode::Hold).is_err());
        assert_eq!(
            parse_trigger("alt+cmd+fn", Mode::Hold).unwrap().input,
            Input::Chord {
                modifiers: flags::OPTION | flags::COMMAND | flags::FN
            }
        );
        assert_eq!(
            parse_trigger("Globe+Ctrl", Mode::Hold).unwrap().input,
            Input::Chord {
                modifiers: flags::CONTROL | flags::FN
            }
        );
        assert!(parse_trigger("cmd+command", Mode::Hold).is_err());
        assert!(parse_trigger("shift", Mode::Hold).is_err());
    }

    /// The tap's view: flags after each modifier key event, through [`modifiers_changed`].
    fn flag_changes(specs: &[&str], events: &[(u16, u64)]) -> (Vec<bool>, Vec<Vec<Change>>) {
        let triggers: Vec<Trigger> = specs
            .iter()
            .map(|s| parse_trigger(s, Mode::Hold).unwrap())
            .collect();
        let mut down = vec![false; triggers.len()];
        let changes = events
            .iter()
            .map(|&(keycode, flags)| modifiers_changed(&triggers, &mut down, keycode, flags))
            .collect();
        (down, changes)
    }

    const OPT: u64 = flags::OPTION | 0x20;
    const OPT_CMD: u64 = OPT | flags::COMMAND | 0x08;

    #[test]
    fn a_modifier_combo_needs_exactly_its_modifiers() {
        let all = OPT_CMD | flags::FN;
        // ⌥, ⌘, Fn: pressed on Fn; let go of ⌥: released.
        let (down, changes) = flag_changes(
            &["alt+cmd+fn"],
            &[
                (58, OPT),
                (55, OPT_CMD),
                (63, all),
                (58, all & !OPT),
                (55, flags::FN),
                (63, 0),
            ],
        );
        use Change::*;
        assert_eq!(
            changes,
            [
                vec![],
                vec![],
                vec![Press(0)],
                vec![Release(0)],
                vec![],
                vec![]
            ]
        );
        assert_eq!(down, [false]);
        // ⇧ on top is another combo: released, pressed again when ⇧ goes.
        let shift = all | flags::SHIFT | 0x02;
        let (_, changes) = flag_changes(&["alt+cmd+fn"], &[(63, all), (56, shift), (56, all)]);
        assert_eq!(changes, [vec![Press(0)], vec![Release(0)], vec![Press(0)]]);
        // Caps Lock and device bits don't matter.
        let (down, _) = flag_changes(&["alt+cmd+fn"], &[(63, all | 0x0001_0000 | 0x100)]);
        assert_eq!(down, [true]);
    }

    #[test]
    fn a_combo_wins_over_the_lone_modifier_in_it() {
        use Change::*;
        let all = OPT_CMD | flags::FN;
        // ⌥⌘ held, then Fn: the combo, not fn; its release changes nothing for fn.
        let (down, changes) = flag_changes(
            &["fn", "alt+cmd+fn"],
            &[(58, OPT), (55, OPT_CMD), (63, all), (63, OPT_CMD)],
        );
        assert_eq!(changes, [vec![], vec![], vec![Press(1)], vec![Release(1)]]);
        assert_eq!(down, [false, false]);
        // Fn first: fn starts, the combo interrupts it (the machine discards) and presses.
        let (down, changes) = flag_changes(
            &["fn", "alt+cmd+fn"],
            &[
                (63, flags::FN),
                (58, flags::FN | OPT),
                (55, all),
                (63, OPT_CMD),
            ],
        );
        assert_eq!(
            changes,
            [
                vec![Press(0)],
                vec![],
                vec![Interrupt(0), Press(1)],
                vec![Release(1), Release(0)]
            ]
        );
        assert_eq!(down, [false, false]);
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hold, ms(0)), Action::Start);
        assert_eq!(m.interrupt(0, ms(300)), Action::Discard);
        assert_eq!(m.press(1, Mode::Hold, ms(300)), Action::Start);
        assert_eq!(m.release(1, Mode::Hold, ms(2000)), Action::Stop);
        assert_eq!(m.release(0, Mode::Hold, ms(2000)), Action::None);
        // Without a combo configured, Fn with ⌥⌘ held stays a Fn press, as before.
        let (down, changes) = flag_changes(&["fn"], &[(58, OPT), (55, OPT_CMD), (63, all)]);
        assert_eq!(changes, [vec![], vec![], vec![Press(0)]]);
        assert_eq!(down, [true]);
    }

    #[test]
    fn capture_takes_a_combo_of_modifiers_on_the_first_release() {
        let all = OPT_CMD | flags::FN;
        let got = capture(&[
            Seen::Flags {
                keycode: 58,
                flags: OPT,
            },
            Seen::Flags {
                keycode: 55,
                flags: OPT_CMD,
            },
            Seen::Flags {
                keycode: 63,
                flags: all,
            },
            Seen::Flags {
                keycode: 63,
                flags: OPT_CMD,
            },
        ]);
        assert_eq!(got, Some(Captured::Spec("alt+cmd+fn".into())));
        assert_eq!(
            parse_trigger("alt+cmd+fn", Mode::Hold).unwrap().input,
            Input::Chord {
                modifiers: flags::OPTION | flags::COMMAND | flags::FN
            }
        );
        // Right ⌥ with ⌘: the combo, not right-option.
        let got = capture(&[
            Seen::Flags {
                keycode: 55,
                flags: flags::COMMAND | 0x08,
            },
            Seen::Flags {
                keycode: 61,
                flags: flags::COMMAND | 0x08 | flags::OPTION | 0x40,
            },
            Seen::Flags {
                keycode: 61,
                flags: flags::COMMAND | 0x08,
            },
        ]);
        assert_eq!(got, Some(Captured::Spec("alt+cmd".into())));
        // ⌥⌘ then a key: the key's combo; letting go afterwards records nothing more.
        let mut c = Capture::default();
        for seen in [
            Seen::Flags {
                keycode: 58,
                flags: OPT,
            },
            Seen::Flags {
                keycode: 55,
                flags: OPT_CMD,
            },
        ] {
            assert_eq!(c.feed(seen), None);
        }
        assert_eq!(
            c.feed(Seen::KeyDown {
                keycode: 23,
                flags: OPT_CMD
            }),
            Some(Captured::Spec("alt+cmd+5".into()))
        );
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 58,
                flags: flags::COMMAND | 0x08
            }),
            None
        );
    }

    #[test]
    fn hold_records_while_pressed() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hold, ms(0)), Action::Start);
        assert_eq!(m.release(0, Mode::Hold, ms(100)), Action::Stop);
        assert!(!m.is_recording());
    }

    #[test]
    fn toggle_ignores_release() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Toggle, ms(0)), Action::Start);
        assert_eq!(m.release(0, Mode::Toggle, ms(2000)), Action::None);
        assert_eq!(m.press(0, Mode::Toggle, ms(5000)), Action::Stop);
        assert_eq!(m.release(0, Mode::Toggle, ms(5100)), Action::None);
    }

    #[test]
    fn hybrid_tap_goes_hands_free_and_hold_stops_on_release() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hybrid, ms(0)), Action::Start);
        assert_eq!(m.release(0, Mode::Hybrid, ms(200)), Action::None);
        assert!(m.is_recording());
        assert_eq!(m.press(0, Mode::Hybrid, ms(4000)), Action::Stop);
        assert_eq!(m.release(0, Mode::Hybrid, ms(4100)), Action::None);

        assert_eq!(m.press(0, Mode::Hybrid, ms(10_000)), Action::Start);
        assert_eq!(m.release(0, Mode::Hybrid, ms(11_000)), Action::Stop);
    }

    #[test]
    fn another_trigger_stops_and_its_release_is_ignored() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hybrid, ms(0)), Action::Start);
        assert_eq!(m.release(0, Mode::Hybrid, ms(100)), Action::None);
        assert_eq!(m.press(1, Mode::Toggle, ms(3000)), Action::Stop);
        assert_eq!(m.release(1, Mode::Toggle, ms(3100)), Action::None);
        assert!(!m.cancel());
    }

    #[test]
    fn hybrid_hold_exactly_at_threshold_stops() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hybrid, ms(0)), Action::Start);
        assert_eq!(m.release(0, Mode::Hybrid, HYBRID_HOLD), Action::Stop);
        assert_eq!(m.press(0, Mode::Hybrid, ms(1000)), Action::Start);
        assert_eq!(m.release(0, Mode::Hybrid, ms(1499)), Action::None);
    }

    #[test]
    fn a_shortcut_with_the_trigger_held_discards_the_recording() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hybrid, ms(0)), Action::Start);
        assert_eq!(m.interrupt(0, ms(300)), Action::Discard);
        assert!(!m.is_recording());
        assert_eq!(m.release(0, Mode::Hybrid, ms(400)), Action::None);
        assert_eq!(m.press(0, Mode::Hybrid, ms(2000)), Action::Start);
    }

    #[test]
    fn late_or_foreign_interruptions_are_ignored() {
        let mut m = Machine::default();
        assert_eq!(m.press(0, Mode::Hybrid, ms(0)), Action::Start);
        // Typing while holding to dictate, after the first second, keeps the recording.
        assert_eq!(m.interrupt(0, ms(1500)), Action::None);
        assert_eq!(m.interrupt(1, ms(100)), Action::None);
        // Hands-free (released) recordings are not interrupted by typing.
        assert_eq!(m.release(0, Mode::Hybrid, ms(2000)), Action::Stop);
        assert_eq!(m.press(0, Mode::Hybrid, ms(3000)), Action::Start);
        assert_eq!(m.release(0, Mode::Hybrid, ms(3100)), Action::None);
        assert_eq!(m.interrupt(0, ms(3200)), Action::None);
        assert!(m.is_recording());
    }

    fn capture(events: &[Seen]) -> Option<Captured> {
        let mut c = Capture::default();
        events.iter().find_map(|e| c.feed(*e))
    }

    #[test]
    fn captured_specs_parse_back_to_the_pressed_trigger() {
        // ⇧⌥⌘5: modifiers arrive as flag changes, then the key.
        let combo = flags::SHIFT | flags::OPTION | flags::COMMAND;
        let got = capture(&[
            Seen::Flags {
                keycode: 56,
                flags: flags::SHIFT,
            },
            Seen::Flags {
                keycode: 58,
                flags: flags::SHIFT | flags::OPTION,
            },
            Seen::Flags {
                keycode: 55,
                flags: combo,
            },
            Seen::KeyDown {
                keycode: 23,
                flags: combo,
            },
        ]);
        assert_eq!(got, Some(Captured::Spec("shift+alt+cmd+5".into())));
        assert_eq!(
            parse_trigger("shift+alt+cmd+5", Mode::Hold).unwrap().input,
            Input::Key {
                keycode: 23,
                modifiers: combo
            }
        );
        let f18 = capture(&[Seen::KeyDown {
            keycode: 79,
            flags: flags::FN,
        }]);
        assert_eq!(f18, Some(Captured::Spec("f18".into())));
        let mouse = capture(&[Seen::Mouse { button: 4 }]);
        assert_eq!(mouse, Some(Captured::Spec("mouse4".into())));

        // Every key and modifier we know round-trips.
        for (name, code) in KEYS {
            let Some(Captured::Spec(spec)) = capture(&[Seen::KeyDown {
                keycode: *code,
                flags: 0,
            }]) else {
                panic!("{name}");
            };
            let parsed = parse_trigger(&spec, Mode::Hold).unwrap().input;
            assert_eq!(
                parsed,
                Input::Key {
                    keycode: *code,
                    modifiers: 0
                },
                "{name}"
            );
        }
        for (name, code, mask) in MODIFIERS {
            let got = capture(&[
                Seen::Flags {
                    keycode: *code,
                    flags: *mask,
                },
                Seen::Flags {
                    keycode: *code,
                    flags: 0,
                },
            ]);
            let Some(Captured::Spec(spec)) = got else {
                panic!("{name}")
            };
            let parsed = parse_trigger(&spec, Mode::Hold).unwrap().input;
            assert_eq!(
                parsed,
                Input::Modifier {
                    keycode: *code,
                    mask: *mask
                },
                "{name}"
            );
        }
    }

    #[test]
    fn capture_waits_for_a_lone_modifier_release_and_esc_cancels() {
        let mut c = Capture::default();
        // Right ⌘ down, then X: a combo (⌘X), not right-command.
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 54,
                flags: flags::COMMAND | 0x10
            }),
            None
        );
        assert_eq!(
            c.feed(Seen::KeyDown {
                keycode: 7,
                flags: flags::COMMAND | 0x10
            }),
            Some(Captured::Spec("cmd+x".into()))
        );
        // Left modifiers alone are no trigger; their release yields nothing.
        let mut c = Capture::default();
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 55,
                flags: flags::COMMAND | 0x08
            }),
            None
        );
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 55,
                flags: 0
            }),
            None
        );
        // Fn pressed, right ⌥ joins: neither counts alone, the two are a combo.
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 63,
                flags: flags::FN
            }),
            None
        );
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 61,
                flags: flags::FN | flags::OPTION | 0x40
            }),
            None
        );
        assert_eq!(
            c.feed(Seen::Flags {
                keycode: 63,
                flags: flags::OPTION | 0x40
            }),
            Some(Captured::Spec("alt+fn".into()))
        );
        assert_eq!(
            capture(&[Seen::KeyDown {
                keycode: 53,
                flags: 0
            }]),
            Some(Captured::Cancelled)
        );
        assert_eq!(
            capture(&[Seen::KeyDown {
                keycode: 10,
                flags: 0
            }]),
            Some(Captured::Unknown(10))
        );
        assert_eq!(capture(&[Seen::Mouse { button: 1 }]), None);
    }
}
