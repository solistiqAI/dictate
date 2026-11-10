//! Triggers from low-level key and mouse hooks, as Windows delivers them (`WH_KEYBOARD_LL`,
//! `WH_MOUSE_LL`): every key, modifiers included, comes as its own down/up with a virtual-key code.
//! [`Hook`] turns those into the same press/release/interrupt events the macOS event tap sends,
//! with the same rules (`platform/macos/mod.rs`, `install_tap`), and keeps the triggers' key codes
//! (`trigger.rs`, macOS virtual key codes) as they are: a Windows key is translated into the macOS
//! code of the same key. Platform free, so it is tested here; the app's
//! `platform/windows/mod.rs` feeds it.
//!
//! Modifier names on Windows: `alt`/`option` = Alt, `cmd`/`command` = the Windows key, `ctrl`,
//! `shift`; `right-alt` (AltGr on German keyboards), `right-ctrl`, `right-shift`, `right-cmd`
//! work on their own, and two or more modifiers together (`ctrl+alt`) as a combo. There is no Fn
//! key on Windows: a `fn` trigger, or a combo with `fn`, never fires there.

use crate::trigger::{Change, Input, Seen, Trigger, flags, modifiers_changed};

/// A key or mouse button event from the hooks. Keys by Windows virtual-key code (`VK_…`, left
/// and right modifiers apart as low-level hooks report them), mouse buttons by the numbers the
/// triggers use (2 = middle, 3 = back/XBUTTON1, 4 = forward/XBUTTON2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    KeyDown(u16),
    KeyUp(u16),
    MouseDown(u16),
    MouseUp(u16),
}

/// What a trigger did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Press(usize),
    Release(usize),
    /// Another key went down while this keyboard trigger was held (a shortcut, not dictation).
    Interrupt(usize),
}

pub const VK_ESCAPE: u16 = 0x1B;
/// macOS key code of Esc (what `Capture` takes as "cancel").
const KEY_ESCAPE: u16 = 53;

/// Modifier keys: Windows VK, macOS key code of the same key, its flag, its device bit (IOKit's
/// NX_DEVICE*KEYMASK, as `trigger.rs` uses for lone modifiers).
const MODIFIERS: &[(u16, u16, u64, u64)] = &[
    (0xA0, 56, flags::SHIFT, 0x02),     // VK_LSHIFT
    (0xA1, 60, flags::SHIFT, 0x04),     // VK_RSHIFT
    (0xA2, 59, flags::CONTROL, 0x01),   // VK_LCONTROL
    (0xA3, 62, flags::CONTROL, 0x2000), // VK_RCONTROL
    (0xA4, 58, flags::OPTION, 0x20),    // VK_LMENU (left Alt)
    (0xA5, 61, flags::OPTION, 0x40),    // VK_RMENU (right Alt, AltGr)
    (0x5B, 55, flags::COMMAND, 0x08),   // VK_LWIN
    (0x5C, 54, flags::COMMAND, 0x10),   // VK_RWIN
];

fn modifier(vk: u16) -> Option<(u16, u64, u64)> {
    MODIFIERS
        .iter()
        .find(|m| m.0 == vk)
        .map(|&(_, code, flag, device)| (code, flag, device))
}

/// The macOS key code for a Windows virtual-key code, for the keys triggers can name (letters,
/// digits, space, F1–F20) and Esc.
pub fn keycode(vk: u16) -> Option<u16> {
    let name = match vk {
        VK_ESCAPE => return Some(KEY_ESCAPE),
        0x20 => "space".to_string(),
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_ascii_lowercase().to_string(),
        0x70..=0x83 => format!("f{}", vk - 0x6F), // VK_F1 … VK_F20
        _ => return None,
    };
    crate::trigger::keycode(&name)
}

/// The triggers' state between hook events.
#[derive(Debug)]
pub struct Hook {
    triggers: Vec<Trigger>,
    down: Vec<bool>,
    /// Modifier flags and device bits held now (as CGEventFlags would have them).
    held: u64,
}

impl Hook {
    pub fn new(triggers: Vec<Trigger>) -> Self {
        Hook {
            down: vec![false; triggers.len()],
            triggers,
            held: 0,
        }
    }

    /// Feeds one event: what the triggers did goes into `out`; the answer says whether the event
    /// is ours (swallowed: the trigger key or button, its auto-repeats) or passes on to the app.
    /// Modifier triggers always pass (other shortcuts use them), like on macOS.
    pub fn feed(&mut self, event: Event, out: &mut Vec<Out>) -> bool {
        if let Some(code) = self.track(event) {
            let changes = modifiers_changed(&self.triggers, &mut self.down, code, self.held);
            out.extend(changes.into_iter().map(|change| match change {
                Change::Press(i) => Out::Press(i),
                Change::Release(i) => Out::Release(i),
                Change::Interrupt(i) => Out::Interrupt(i),
            }));
            return false;
        }
        let key = match event {
            Event::KeyDown(vk) | Event::KeyUp(vk) => keycode(vk),
            _ => None,
        };
        for i in 0..self.triggers.len() {
            let pressed = match (self.triggers[i].input, event) {
                (Input::Key { keycode, modifiers }, Event::KeyDown(_)) if key == Some(keycode) => {
                    if self.down[i] {
                        return true; // auto-repeat
                    }
                    (self.held & flags::ALL == modifiers).then_some(true)
                }
                (Input::Key { keycode, .. }, Event::KeyUp(_))
                    if key == Some(keycode) && self.down[i] =>
                {
                    Some(false)
                }
                (Input::Mouse { button }, Event::MouseDown(b)) if b == button => Some(true),
                (Input::Mouse { button }, Event::MouseUp(b)) if b == button => Some(false),
                _ => None,
            };
            if let Some(pressed) = pressed
                && self.set(i, pressed, out)
            {
                return true;
            }
        }
        if let Event::KeyDown(_) = event {
            for (i, trigger) in self.triggers.iter().enumerate() {
                if self.down[i] && !matches!(trigger.input, Input::Mouse { .. }) {
                    out.push(Out::Interrupt(i));
                }
            }
        }
        false
    }

    /// Keeps the held modifiers up to date (what [`Hook::feed`] does first; alone while the
    /// settings record a trigger). Returns the macOS key code of a modifier key event.
    pub fn track(&mut self, event: Event) -> Option<u16> {
        let (Event::KeyDown(vk) | Event::KeyUp(vk)) = event else {
            return None;
        };
        let (code, flag, device) = modifier(vk)?;
        self.hold(flag, device, matches!(event, Event::KeyDown(_)));
        Some(code)
    }

    /// What the settings' "Aufnehmen" (`trigger::Capture`) sees of an event; `None` for key
    /// releases of ordinary keys (they pass). Call after [`Hook::track`] (modifier state).
    pub fn seen(&self, event: Event) -> Option<Seen> {
        match event {
            Event::KeyDown(vk) | Event::KeyUp(vk) if modifier(vk).is_some() => Some(Seen::Flags {
                keycode: modifier(vk)?.0,
                flags: self.held,
            }),
            Event::KeyDown(vk) => Some(Seen::KeyDown {
                // Keys without a name get a code no macOS key has, for the "unknown key" note.
                keycode: keycode(vk).unwrap_or(1000 + vk),
                flags: self.held,
            }),
            Event::MouseDown(button) => Some(Seen::Mouse { button }),
            _ => None,
        }
    }

    /// Records a change of trigger `i`; `true` when it was one (the event belongs to it).
    fn set(&mut self, i: usize, pressed: bool, out: &mut Vec<Out>) -> bool {
        if pressed == self.down[i] {
            return false;
        }
        self.down[i] = pressed;
        out.push(match pressed {
            true => Out::Press(i),
            false => Out::Release(i),
        });
        true
    }

    fn hold(&mut self, flag: u64, device: u64, down: bool) {
        if down {
            self.held |= flag | device;
            return;
        }
        self.held &= !device;
        let same_flag = MODIFIERS
            .iter()
            .filter(|m| m.2 == flag)
            .any(|m| self.held & m.3 != 0);
        if !same_flag {
            self.held &= !flag;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{Capture, Captured, Mode, parse_trigger};

    const RALT: u16 = 0xA5;
    const LCTRL: u16 = 0xA2;
    const LSHIFT: u16 = 0xA0;
    const F13: u16 = 0x7C;
    const KEY_5: u16 = 0x35;
    const KEY_Q: u16 = 0x51;

    fn hook(specs: &[&str]) -> Hook {
        Hook::new(
            specs
                .iter()
                .map(|s| parse_trigger(s, Mode::Hybrid).unwrap())
                .collect(),
        )
    }

    fn feed(h: &mut Hook, event: Event) -> (bool, Vec<Out>) {
        let mut out = Vec::new();
        let swallowed = h.feed(event, &mut out);
        (swallowed, out)
    }

    #[test]
    fn windows_keys_become_the_same_macos_keys() {
        assert_eq!(keycode(0x41), Some(0)); // A
        assert_eq!(keycode(KEY_5), Some(23));
        assert_eq!(keycode(0x20), Some(49));
        assert_eq!(keycode(0x70), Some(122)); // F1
        assert_eq!(keycode(F13), Some(105));
        assert_eq!(keycode(0x83), Some(90)); // F20
        assert_eq!(keycode(VK_ESCAPE), Some(KEY_ESCAPE));
        assert_eq!(keycode(0x25), None); // ←
    }

    #[test]
    fn right_alt_alone_presses_and_releases_and_passes_through() {
        let mut h = hook(&["right-alt"]);
        assert_eq!(
            feed(&mut h, Event::KeyDown(RALT)),
            (false, vec![Out::Press(0)])
        );
        // Auto-repeat of a held modifier: nothing new.
        assert_eq!(feed(&mut h, Event::KeyDown(RALT)), (false, vec![]));
        assert_eq!(
            feed(&mut h, Event::KeyUp(RALT)),
            (false, vec![Out::Release(0)])
        );
        // Left Alt is another key.
        assert_eq!(feed(&mut h, Event::KeyDown(0xA4)), (false, vec![]));
    }

    #[test]
    fn altgr_combo_interrupts_the_right_alt_trigger() {
        // AltGr+Q (@ on German keyboards): Windows sends a fake left Ctrl, then right Alt, then Q.
        let mut h = hook(&["right-alt"]);
        assert_eq!(feed(&mut h, Event::KeyDown(LCTRL)), (false, vec![]));
        assert_eq!(
            feed(&mut h, Event::KeyDown(RALT)),
            (false, vec![Out::Press(0)])
        );
        assert_eq!(
            feed(&mut h, Event::KeyDown(KEY_Q)),
            (false, vec![Out::Interrupt(0)])
        );
    }

    #[test]
    fn key_triggers_are_swallowed_with_their_repeats() {
        let mut h = hook(&["f13"]);
        assert_eq!(
            feed(&mut h, Event::KeyDown(F13)),
            (true, vec![Out::Press(0)])
        );
        assert_eq!(feed(&mut h, Event::KeyDown(F13)), (true, vec![]));
        assert_eq!(
            feed(&mut h, Event::KeyUp(F13)),
            (true, vec![Out::Release(0)])
        );
        // A release without a press is not ours.
        assert_eq!(feed(&mut h, Event::KeyUp(F13)), (false, vec![]));
    }

    #[test]
    fn combos_need_exactly_their_modifiers() {
        let mut h = hook(&["shift+ctrl+5"]);
        assert_eq!(feed(&mut h, Event::KeyDown(KEY_5)), (false, vec![]));
        feed(&mut h, Event::KeyDown(LSHIFT));
        assert_eq!(feed(&mut h, Event::KeyDown(KEY_5)), (false, vec![]));
        feed(&mut h, Event::KeyUp(KEY_5));
        feed(&mut h, Event::KeyDown(0xA3)); // right Ctrl counts as Ctrl
        assert_eq!(
            feed(&mut h, Event::KeyDown(KEY_5)),
            (true, vec![Out::Press(0)])
        );
        assert_eq!(
            feed(&mut h, Event::KeyUp(KEY_5)),
            (true, vec![Out::Release(0)])
        );
    }

    #[test]
    fn one_shift_released_while_the_other_is_held_keeps_shift() {
        let mut h = hook(&["shift+5"]);
        feed(&mut h, Event::KeyDown(LSHIFT));
        feed(&mut h, Event::KeyDown(0xA1));
        feed(&mut h, Event::KeyUp(LSHIFT));
        assert_eq!(
            feed(&mut h, Event::KeyDown(KEY_5)),
            (true, vec![Out::Press(0)])
        );
    }

    #[test]
    fn mouse_buttons_are_swallowed_and_never_interrupted() {
        let mut h = hook(&["mouse3", "right-alt"]);
        assert_eq!(
            feed(&mut h, Event::MouseDown(3)),
            (true, vec![Out::Press(0)])
        );
        assert_eq!(feed(&mut h, Event::KeyDown(KEY_Q)), (false, vec![]));
        assert_eq!(
            feed(&mut h, Event::MouseUp(3)),
            (true, vec![Out::Release(0)])
        );
        assert_eq!(feed(&mut h, Event::MouseDown(2)), (false, vec![]));
    }

    #[test]
    fn a_modifier_combo_presses_and_passes_through() {
        let mut h = hook(&["right-alt", "ctrl+alt"]);
        assert_eq!(feed(&mut h, Event::KeyDown(LCTRL)), (false, vec![]));
        // Right Alt with Ctrl held: the combo, not right-alt.
        assert_eq!(
            feed(&mut h, Event::KeyDown(RALT)),
            (false, vec![Out::Press(1)])
        );
        assert_eq!(
            feed(&mut h, Event::KeyUp(LCTRL)),
            (false, vec![Out::Release(1)])
        );
        assert_eq!(feed(&mut h, Event::KeyUp(RALT)), (false, vec![]));
        // A combo with fn never fires on Windows (no Fn key there).
        let mut h = hook(&["alt+cmd+fn"]);
        assert_eq!(feed(&mut h, Event::KeyDown(0xA4)), (false, vec![]));
        assert_eq!(feed(&mut h, Event::KeyDown(0x5B)), (false, vec![]));
    }

    #[test]
    fn capture_records_windows_keys_as_trigger_specs() {
        let mut h = hook(&[]);
        let mut capture = Capture::default();
        let mut run = |h: &mut Hook, event| {
            h.track(event);
            h.seen(event).and_then(|seen| capture.feed(seen))
        };
        // Right Alt alone: on its release.
        assert_eq!(run(&mut h, Event::KeyDown(RALT)), None);
        assert_eq!(
            run(&mut h, Event::KeyUp(RALT)),
            Some(Captured::Spec("right-option".into()))
        );
        // A combo on the key's press.
        run(&mut h, Event::KeyDown(LSHIFT));
        run(&mut h, Event::KeyDown(LCTRL));
        assert_eq!(
            run(&mut h, Event::KeyDown(KEY_5)),
            Some(Captured::Spec("shift+ctrl+5".into()))
        );
        assert_eq!(run(&mut h, Event::KeyUp(LSHIFT)), None);
        assert_eq!(run(&mut h, Event::KeyUp(LCTRL)), None);
        // Modifiers alone: on the first release.
        run(&mut h, Event::KeyDown(LCTRL));
        run(&mut h, Event::KeyDown(0xA4));
        assert_eq!(
            run(&mut h, Event::KeyUp(0xA4)),
            Some(Captured::Spec("ctrl+alt".into()))
        );
        run(&mut h, Event::KeyUp(LCTRL));
        assert_eq!(
            run(&mut h, Event::KeyDown(VK_ESCAPE)),
            Some(Captured::Cancelled)
        );
        assert_eq!(
            run(&mut h, Event::MouseDown(4)),
            Some(Captured::Spec("mouse4".into()))
        );
        assert_eq!(
            run(&mut h, Event::KeyDown(0x25)),
            Some(Captured::Unknown(1037))
        );
    }
}
