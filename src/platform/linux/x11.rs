//! The triggers and the paste keys in an X11 session (BETA, never run). libX11, libXi and
//! libXtst are loaded at run time (`x11-dl`), so building needs none of their headers.
//!
//! - Watching: XInput2 raw key and button events on the root window, from a thread with its own
//!   connection. They come for every key in every app, need no permission, and cannot be
//!   swallowed, so:
//! - Swallowing: passive grabs (`XGrabKey`, `XGrabButton`) for the key and mouse triggers
//!   (`dictate_core::linux::grabs`), and for Esc while a recording or a voice runs. A lone
//!   modifier (right Alt) is never grabbed: it passes, as on the Mac.
//! - Pasting: XTest's fake Ctrl+V on a connection of its own, keycodes looked up from the
//!   keysyms (so other layouts get the key that types V).

use std::ffi::{CString, c_int};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};
use x11_dl::xinput2::{self, XInput2};
use x11_dl::xlib::{self, Display, Xlib};
use x11_dl::xtest::Xf86vmode as XTest;

use crate::trigger::Trigger;
use dictate_core::linux::{Grab, Raw, lock_variants, x11_event, x11_mask};

/// Esc's X keycode (evdev 1 + 8).
const ESCAPE: c_int = 9;

/// Whether an X server and XInput2 are there (libraries load, the display opens, XI ≥ 2.0).
pub fn available() -> bool {
    let (Ok(xlib), Ok(xi)) = (Xlib::open(), XInput2::open()) else {
        return false;
    };
    unsafe {
        let display = (xlib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            return false;
        }
        let ok = xi_opcode(&xlib, &xi, display).is_some();
        (xlib.XCloseDisplay)(display);
        ok
    }
}

/// Whether an XEmbed system tray runs (someone owns `_NET_SYSTEM_TRAY_S<screen>`: i3bar, tint2,
/// stalonetray); `None` without an X display.
pub fn system_tray() -> Option<bool> {
    std::env::var_os("DISPLAY").filter(|d| !d.is_empty())?;
    let xlib = Xlib::open().ok()?;
    unsafe {
        let display = (xlib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            return None;
        }
        let screen = (xlib.XDefaultScreen)(display);
        let name = CString::new(format!("_NET_SYSTEM_TRAY_S{screen}")).unwrap();
        let atom = (xlib.XInternAtom)(display, name.as_ptr(), xlib::False);
        let owned = (xlib.XGetSelectionOwner)(display, atom) != 0;
        (xlib.XCloseDisplay)(display);
        Some(owned)
    }
}

/// The XInputExtension's major opcode, when XI 2.0 or later is there.
unsafe fn xi_opcode(xlib: &Xlib, xi: &XInput2, display: *mut Display) -> Option<c_int> {
    let name = CString::new("XInputExtension").unwrap();
    let (mut opcode, mut event, mut error) = (0, 0, 0);
    unsafe {
        if (xlib.XQueryExtension)(display, name.as_ptr(), &mut opcode, &mut event, &mut error) == 0
        {
            return None;
        }
        let (mut major, mut minor) = (2, 0);
        if (xi.XIQueryVersion)(display, &mut major, &mut minor) != 0 {
            return None;
        }
    }
    Some(opcode)
}

/// Starts the watching thread; it ends when `stop` is set.
pub fn watch(triggers: Vec<Trigger>, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    let xlib = Xlib::open().context("load libX11")?;
    let xi = XInput2::open().context("load libXi")?;
    // Opened here so a missing display is an error of install_tap, not a silent thread.
    let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    if display.is_null() {
        bail!(
            "cannot open the X display (DISPLAY={:?})",
            std::env::var("DISPLAY").ok()
        );
    }
    let Some(opcode) = (unsafe { xi_opcode(&xlib, &xi, display) }) else {
        unsafe { (xlib.XCloseDisplay)(display) };
        bail!("the X server has no XInput 2");
    };
    // The pointer crosses into the thread, which alone uses it from now on.
    let display = display as usize;
    let thread = thread::Builder::new()
        .name("x11-input".into())
        .spawn(move || unsafe {
            run(
                &xlib,
                &xi,
                display as *mut Display,
                opcode,
                &triggers,
                &stop,
            );
            (xlib.XCloseDisplay)(display as *mut Display);
        })?;
    Ok(thread)
}

/// Ignores X errors while grabbing (a key another program has grabbed answers BadAccess, which
/// would end the process under Xlib's default handler).
unsafe extern "C" fn ignore_error(_: *mut Display, _: *mut xlib::XErrorEvent) -> c_int {
    0
}

/// Makes (`on`) or removes the grabs, each with every Caps Lock / Num Lock combination.
unsafe fn set_grabs(xlib: &Xlib, display: *mut Display, root: u64, grabs: &[Grab], on: bool) {
    unsafe {
        let previous = (xlib.XSetErrorHandler)(Some(ignore_error));
        for grab in grabs {
            match *grab {
                Grab::Key { keycode, modifiers } => {
                    for mods in lock_variants(modifiers) {
                        if on {
                            (xlib.XGrabKey)(
                                display,
                                keycode as c_int,
                                mods,
                                root,
                                xlib::False,
                                xlib::GrabModeAsync,
                                xlib::GrabModeAsync,
                            );
                        } else {
                            (xlib.XUngrabKey)(display, keycode as c_int, mods, root);
                        }
                    }
                }
                Grab::Button(button) => {
                    if on {
                        (xlib.XGrabButton)(
                            display,
                            button,
                            x11_mask::ANY,
                            root,
                            xlib::False,
                            (xlib::ButtonPressMask | xlib::ButtonReleaseMask) as u32,
                            xlib::GrabModeAsync,
                            xlib::GrabModeAsync,
                            0,
                            0,
                        );
                    } else {
                        (xlib.XUngrabButton)(display, button, x11_mask::ANY, root);
                    }
                }
            }
        }
        (xlib.XSync)(display, xlib::False);
        (xlib.XSetErrorHandler)(previous);
    }
}

unsafe fn run(
    xlib: &Xlib,
    xi: &XInput2,
    display: *mut Display,
    opcode: c_int,
    triggers: &[Trigger],
    stop: &AtomicBool,
) {
    unsafe {
        let root = (xlib.XDefaultRootWindow)(display);
        let mut mask = [0u8; 4];
        for event in [
            xinput2::XI_RawKeyPress,
            xinput2::XI_RawKeyRelease,
            xinput2::XI_RawButtonPress,
            xinput2::XI_RawButtonRelease,
        ] {
            xinput2::XISetMask(&mut mask, event);
        }
        let mut event_mask = xinput2::XIEventMask {
            deviceid: xinput2::XIAllMasterDevices,
            mask_len: mask.len() as c_int,
            mask: mask.as_mut_ptr(),
        };
        (xi.XISelectEvents)(display, root, &mut event_mask, 1);
        let grabs = dictate_core::linux::grabs(triggers);
        set_grabs(xlib, display, root, &grabs, true);
        let escape = [Grab::Key {
            keycode: ESCAPE as u32,
            modifiers: 0,
        }];
        let mut escape_grabbed = false;
        (xlib.XFlush)(display);
        let fd = (xlib.XConnectionNumber)(display);
        while !stop.load(Ordering::Relaxed) {
            // Esc belongs to us only while it cancels something.
            if super::cancels() != escape_grabbed {
                escape_grabbed = !escape_grabbed;
                set_grabs(xlib, display, root, &escape, escape_grabbed);
            }
            while (xlib.XPending)(display) > 0 {
                let mut event: xlib::XEvent = std::mem::zeroed();
                (xlib.XNextEvent)(display, &mut event);
                let cookie = &mut event.generic_event_cookie;
                if cookie.type_ != xlib::GenericEvent
                    || cookie.extension != opcode
                    || (xlib.XGetEventData)(display, cookie) == 0
                {
                    continue; // the grabbed keys' own events, among others
                }
                let kind = match cookie.evtype {
                    xinput2::XI_RawKeyPress => Some(Raw::KeyPress),
                    xinput2::XI_RawKeyRelease => Some(Raw::KeyRelease),
                    xinput2::XI_RawButtonPress => Some(Raw::ButtonPress),
                    xinput2::XI_RawButtonRelease => Some(Raw::ButtonRelease),
                    _ => None,
                };
                let detail = (*(cookie.data as *const xinput2::XIRawEvent)).detail;
                (xlib.XFreeEventData)(display, cookie);
                if let Some(event) = kind.and_then(|kind| x11_event(kind, detail as u32)) {
                    super::handle(event);
                }
            }
            let mut poll = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            libc::poll(&mut poll, 1, 100);
        }
        set_grabs(xlib, display, root, &grabs, false);
        if escape_grabbed {
            set_grabs(xlib, display, root, &escape, false);
        }
    }
}

/// Ctrl+V through XTest, on a connection of its own.
pub fn send_ctrl_v() -> Result<()> {
    let xlib = Xlib::open().context("load libX11")?;
    let xtest = XTest::open().context("load libXtst")?;
    unsafe {
        let display = (xlib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            bail!("cannot open the X display");
        }
        let (mut event, mut error, mut major, mut minor) = (0, 0, 0, 0);
        let result =
            if (xtest.XTestQueryExtension)(display, &mut event, &mut error, &mut major, &mut minor)
                == 0
            {
                Err(anyhow::anyhow!("the X server has no XTest"))
            } else {
                let control = (xlib.XKeysymToKeycode)(display, x11_dl::keysym::XK_Control_L as u64);
                let v = (xlib.XKeysymToKeycode)(display, x11_dl::keysym::XK_v as u64);
                if control == 0 || v == 0 {
                    Err(anyhow::anyhow!(
                        "no key for Ctrl or V in this keyboard layout"
                    ))
                } else {
                    for (key, down) in [(control, 1), (v, 1), (v, 0), (control, 0)] {
                        (xtest.XTestFakeKeyEvent)(display, key as u32, down, 0);
                    }
                    (xlib.XSync)(display, xlib::False);
                    Ok(())
                }
            };
        (xlib.XCloseDisplay)(display);
        result
    }
}
