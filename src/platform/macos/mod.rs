//! macOS glue: the global event tap that watches the triggers, pasting, permissions, plus the
//! macOS-only parts of the app as submodules (overlay and bar as AppKit panels, the Neural Engine
//! helper, PocketTTS, pausing media). `platform/mod.rs` says what each platform provides.

pub mod ane;
pub mod bar;
mod glass;
pub mod media;
pub mod overlay;
pub mod pocket;
pub mod system_audio;

pub use objc2::MainThreadMarker;

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_foundation::string::CFString;
use core_foundation::string::CFStringRef;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use super::{RECORDING, TapEvent, VOICE};
use crate::trigger::{Capture, Captured, Input, Seen, Trigger, flags};

/// Marks events we post ourselves (the ⌘V), so the tap lets them through untouched.
const OWN_EVENT: i64 = 0x5051_4954; // "SQIT"
const KEY_ESCAPE: u16 = 53;
const KEY_V: u16 = 9;
const KEY_C: u16 = 8;

static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

type OnCaptured = Box<dyn Fn(Captured) + Send>;
/// While the settings window records a trigger: the capture and who gets the result. Keys and
/// buttons go there instead of starting a dictation.
static CAPTURE: Mutex<Option<(Capture, OnCaptured)>> = Mutex::new(None);
/// The key that ended a capture: its auto-repeats and release are swallowed too.
static CAPTURED_KEY: Mutex<Option<u16>> = Mutex::new(None);

/// Records the next trigger pressed (see [`Capture`]); `on_done` gets it once. Needs the tap.
pub fn start_capture(on_done: impl Fn(Captured) + Send + 'static) {
    *CAPTURE.lock().unwrap() = Some((Capture::default(), Box::new(on_done)));
}

/// Ends a running capture; `true` when one was running.
pub fn stop_capture() -> bool {
    CAPTURE.lock().unwrap().take().is_some()
}

/// Feeds a tap event to a running capture. `None` when no capture runs (normal handling).
fn capture(kind: CGEventType, event: &CGEvent) -> Option<CallbackResult> {
    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
    {
        let mut captured = CAPTURED_KEY.lock().unwrap();
        if *captured == Some(keycode) {
            match kind {
                CGEventType::KeyDown => return Some(CallbackResult::Drop), // auto-repeat
                CGEventType::KeyUp => {
                    *captured = None;
                    return Some(CallbackResult::Drop);
                }
                _ => {}
            }
        }
    }
    let mut slot = CAPTURE.lock().unwrap();
    let (capture, _) = slot.as_mut()?;
    let event_flags = event.get_flags().bits();
    let seen = match kind {
        CGEventType::KeyDown => Seen::KeyDown {
            keycode,
            flags: event_flags,
        },
        CGEventType::FlagsChanged => Seen::Flags {
            keycode,
            flags: event_flags,
        },
        CGEventType::OtherMouseDown => Seen::Mouse {
            button: event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER) as u16,
        },
        // Releases of keys pressed before the capture pass; the captured one is handled above.
        _ => return Some(CallbackResult::Keep),
    };
    if let Some(done) = capture.feed(seen)
        && let Some((_, on_done)) = slot.take()
    {
        drop(slot);
        if matches!(kind, CGEventType::KeyDown) {
            *CAPTURED_KEY.lock().unwrap() = Some(keycode);
        }
        on_done(done);
    }
    Some(match kind {
        CGEventType::FlagsChanged => CallbackResult::Keep,
        _ => CallbackResult::Drop,
    })
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    fn AXValueGetTypeID() -> core_foundation::base::CFTypeID;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// Asks for the Accessibility permission (needed to see keys of other apps and to paste).
/// Shows the system prompt the first time. The permission belongs to the app that runs us
/// (Terminal, iTerm, …) until we ship as a signed .app.
pub fn ensure_accessibility() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Installs the event tap on the current (main) run loop. Keep the returned tap alive.
pub fn install_tap(
    triggers: Vec<Trigger>,
    events: Sender<TapEvent>,
) -> Result<CGEventTap<'static>> {
    let down = Mutex::new(vec![false; triggers.len()]);
    let callback = move |_proxy, kind: CGEventType, event: &CGEvent| -> CallbackResult {
        if matches!(
            kind,
            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
        ) {
            // macOS disables slow taps; switch it right back on.
            let port = TAP_PORT.load(Ordering::Relaxed);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port as CFMachPortRef, true) };
            }
            return CallbackResult::Keep;
        }
        if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == OWN_EVENT {
            return CallbackResult::Keep;
        }
        if let Some(result) = capture(kind, event) {
            return result;
        }
        let now = Instant::now();
        let mut down = down.lock().unwrap();
        let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
        let event_flags = event.get_flags().bits();

        if matches!(kind, CGEventType::KeyDown)
            && keycode == KEY_ESCAPE
            && (RECORDING.load(Ordering::Relaxed) || VOICE.load(Ordering::Relaxed))
        {
            let _ = events.send(TapEvent::Cancel);
            return CallbackResult::Drop;
        }

        for (index, trigger) in triggers.iter().enumerate() {
            let pressed = match (trigger.input, kind) {
                (
                    Input::Modifier {
                        keycode: code,
                        mask,
                    },
                    CGEventType::FlagsChanged,
                ) if code == keycode => Some(event_flags & mask != 0),
                (
                    Input::Key {
                        keycode: code,
                        modifiers,
                    },
                    CGEventType::KeyDown,
                ) if code == keycode => {
                    if down[index] {
                        return CallbackResult::Drop; // auto-repeat
                    }
                    (event_flags & flags::ALL == modifiers).then_some(true)
                }
                (Input::Key { keycode: code, .. }, CGEventType::KeyUp)
                    if code == keycode && down[index] =>
                {
                    Some(false)
                }
                (
                    Input::Mouse { button },
                    CGEventType::OtherMouseDown | CGEventType::OtherMouseUp,
                ) => {
                    let number =
                        event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER) as u16;
                    (number == button).then_some(matches!(kind, CGEventType::OtherMouseDown))
                }
                _ => None,
            };
            let Some(pressed) = pressed else { continue };
            if pressed == down[index] {
                continue;
            }
            down[index] = pressed;
            let _ = events.send(if pressed {
                TapEvent::Press(index, now)
            } else {
                TapEvent::Release(index, now)
            });
            // Modifiers pass through (other shortcuts use them); keys and buttons are ours.
            return match trigger.input {
                Input::Modifier { .. } => CallbackResult::Keep,
                _ => CallbackResult::Drop,
            };
        }
        if matches!(kind, CGEventType::KeyDown) {
            // Another key while a keyboard trigger is held: maybe a shortcut, not dictation.
            for (index, trigger) in triggers.iter().enumerate() {
                if down[index] && !matches!(trigger.input, Input::Mouse { .. }) {
                    let _ = events.send(TapEvent::Interrupt(index, now));
                }
            }
        }
        CallbackResult::Keep
    };

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
        ],
        callback,
    )
    .map_err(|()| {
        anyhow!(
            "cannot watch the keyboard: grant Accessibility (System Settings → Privacy & Security)"
        )
    })?;
    TAP_PORT.store(
        tap.mach_port().as_concrete_TypeRef() as *mut c_void,
        Ordering::Relaxed,
    );
    let source = tap
        .mach_port()
        .create_runloop_source(0)
        .map_err(|()| anyhow!("run loop source"))?;
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    Ok(tap)
}

/// Whether we may watch keys and paste, without prompting.
pub fn is_trusted() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::false_value().as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

/// Opens a file or folder with its default app (`text` forces the text editor).
pub fn open(path: &std::path::Path, text: bool) {
    let mut command = std::process::Command::new("open");
    if text {
        command.arg("-t");
    }
    let _ = command.arg(path).spawn();
}

/// Whether the paste goes into text: `Some(false)` when nothing is focused or it is clearly not a
/// text control (list, button, …): the card keeps the text. `None` when we cannot tell (terminals,
/// an Accessibility error): paste anyway.
///
/// In a web view (a Tauri app, a browser) "nothing" or a non-text element (a group, a button just
/// clicked) means a paste goes nowhere and the text is lost; there the focus moves to the page's
/// only text area (a chat app's composer) and the text is pasted into it, else the card keeps it.
pub fn focused_accepts_text() -> Option<bool> {
    let verdict = native_focus_check();
    if verdict == Some(true) {
        return verdict;
    }
    let Some(web) = frontmost_web_area() else {
        return verdict;
    };
    if focused_element().is_some_and(|e| inside_editable(&e)) {
        return Some(true);
    }
    let app = frontmost_name();
    match only_text_area(&web) {
        Some(area) => {
            let name = CFString::new("AXFocused");
            let error = unsafe {
                AXUIElementSetAttributeValue(
                    area.as_CFTypeRef(),
                    name.as_concrete_TypeRef(),
                    CFBoolean::true_value().as_CFTypeRef(),
                )
            };
            if error != 0 {
                eprintln!(
                    "not pasted: no text field focused in {app}, focusing its text area failed ({error})"
                );
                return Some(false);
            }
            // The page takes the focus asynchronously; ⌘V right away can go to the old element.
            thread::sleep(Duration::from_millis(120));
            eprintln!("no text field focused in {app}: pasting into its only text area");
            Some(true)
        }
        None => {
            eprintln!(
                "not pasted: no text field focused in {app}, and not exactly one text area there"
            );
            Some(false)
        }
    }
}

/// The focused element in web content sits inside an editable region (contenteditable editors:
/// Notion, Google Docs, rich composers): WebKit and Chrome name it `AXEditableAncestor`.
fn inside_editable(element: &CFType) -> bool {
    attribute(element, "AXEditableAncestor").is_some()
        || attribute(element, "AXEditable")
            .and_then(|v| v.downcast::<CFBoolean>())
            .is_some_and(bool::from)
}

/// The web area's one visible text area (or, without any, its one text field), via WebKit's and
/// Chrome's element search (milliseconds, unlike walking the tree).
fn only_text_area(web: &CFType) -> Option<CFType> {
    let pairs = [
        (
            "AXSearchKey",
            CFString::new("AXTextFieldSearchKey").as_CFType(),
        ),
        ("AXDirection", CFString::new("AXDirectionNext").as_CFType()),
        (
            "AXResultsLimit",
            core_foundation::number::CFNumber::from(20).as_CFType(),
        ),
        ("AXVisibleOnly", CFBoolean::true_value().as_CFType()),
    ];
    let keys: Vec<(CFType, CFType)> = pairs
        .into_iter()
        .map(|(k, v)| (CFString::new(k).as_CFType(), v))
        .collect();
    let parameter = CFDictionary::from_CFType_pairs(&keys);
    let name = CFString::new("AXUIElementsForSearchPredicate");
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyParameterizedAttributeValue(
            web.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            parameter.as_CFTypeRef(),
            &mut value,
        )
    };
    if error != 0 || value.is_null() {
        return None;
    }
    let found = unsafe { CFType::wrap_under_create_rule(value) }.downcast::<CFArray>()?;
    let fields: Vec<(String, CFType)> = found
        .iter()
        .map(|item| unsafe { CFType::wrap_under_get_rule(*item as CFTypeRef) })
        .map(|e| {
            let role = attribute(&e, "AXRole")
                .and_then(|v| v.downcast::<CFString>())
                .map(|r| r.to_string())
                .unwrap_or_default();
            (role, e)
        })
        .collect();
    let areas: Vec<&CFType> = fields
        .iter()
        .filter(|(role, _)| role == "AXTextArea")
        .map(|(_, e)| e)
        .collect();
    match (areas.len(), fields.len()) {
        (1, _) => Some(areas[0].clone()),
        (0, 1) => Some(fields[0].1.clone()),
        _ => None,
    }
}

/// The focus check for any app: the focused element's role and editability.
fn native_focus_check() -> Option<bool> {
    let focused = match focus() {
        Ok(focused) => focused,
        Err(AX_ERROR_NO_VALUE) if frontmost_builds_tree_on_request() => {
            eprintln!(
                "nothing focused in {} yet (Electron, tree still building), pasting anyway",
                frontmost_name()
            );
            return None;
        }
        Err(AX_ERROR_NO_VALUE) => {
            eprintln!("not pasted: nothing focused in {}", frontmost_name());
            return Some(false);
        }
        Err(error) => {
            eprintln!(
                "focused element in {} unknown (Accessibility error {error}), pasting anyway",
                frontmost_name()
            );
            return None;
        }
    };
    if let Some(editable) =
        attribute(&focused, "AXEditable").and_then(|v| v.downcast::<CFBoolean>())
    {
        let editable: bool = editable.into();
        if !editable {
            eprintln!(
                "not pasted: the focused element in {} is not editable",
                frontmost_name()
            );
        }
        return Some(editable);
    }
    let role = attribute(&focused, "AXRole")?
        .downcast::<CFString>()?
        .to_string();
    let accepts = accepts_text(&role);
    if accepts == Some(false) {
        eprintln!(
            "not pasted: the focused element in {} is a {role}",
            frontmost_name()
        );
    }
    accepts
}

/// The frontmost app's name, for the log.
fn frontmost_name() -> String {
    objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|app| app.localizedName())
        .map_or_else(|| "?".to_string(), |name| name.to_string())
}

/// `kAXErrorNoValue`: the attribute has no value, here: nothing has the focus.
const AX_ERROR_NO_VALUE: i32 = -25212;

fn attribute(element: &CFType, name: &str) -> Option<CFType> {
    attribute_or_error(element, name).ok()
}

/// An attribute's value, or the `AXError` code that came instead.
fn attribute_or_error(element: &CFType, name: &str) -> Result<CFType, i32> {
    let mut value: CFTypeRef = std::ptr::null();
    let name = CFString::new(name);
    let error = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            &mut value,
        )
    };
    match (error, value.is_null()) {
        (0, false) => Ok(unsafe { CFType::wrap_under_create_rule(value) }),
        (0, true) => Err(AX_ERROR_NO_VALUE),
        (error, _) => Err(error),
    }
}

fn focused_element() -> Option<CFType> {
    focus().ok()
}

/// The focused element, or the `AXError` code. The system-wide element fails for some apps with
/// `kAXErrorCannotComplete` (-25204) although a field has the focus (a Tauri/WebKit
/// window, every time); asking the frontmost app itself works there. Electron apps (VS Code, Slack,
/// Notion, …) answer "nothing focused" until an assistive app asks for their Accessibility tree, so
/// the first time an app says that, it gets `AXManualAccessibility` and a moment to build the tree.
fn focus() -> Result<CFType, i32> {
    let system = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateSystemWide()) };
    let error = match attribute_or_error(&system, "AXFocusedUIElement") {
        Ok(focused) => return Ok(focused),
        Err(error) => error,
    };
    let Some(pid) = objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
    else {
        return Err(error);
    };
    let app = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateApplication(pid)) };
    let mut result = attribute_or_error(&app, "AXFocusedUIElement");
    if result.as_ref().err() == Some(&AX_ERROR_NO_VALUE) {
        // Electron builds its tree on request; WebKit and others sometimes answer "nothing" for a
        // moment while the focus or the page changes (2 of 300 queries in a busy Chrome window).
        let tries = if enable_manual_accessibility(&app, pid) {
            10
        } else {
            6
        };
        for n in 1..=tries {
            thread::sleep(Duration::from_millis(50));
            result = attribute_or_error(&app, "AXFocusedUIElement");
            if result.is_ok() {
                eprintln!("focus in {} after {} ms", frontmost_name(), n * 50);
                break;
            }
        }
    }
    result
}

/// The web area in the frontmost app's focused window (WKWebView, a browser), if it holds one.
pub fn frontmost_web_area() -> Option<CFType> {
    let pid = objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())?;
    let app = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateApplication(pid)) };
    let window = attribute(&app, "AXFocusedWindow")?;
    // A Tauri app's web area is 4 levels down, Chrome's 8.
    let mut level = vec![window];
    for _ in 0..10 {
        let mut next = Vec::new();
        for element in level {
            let role = attribute(&element, "AXRole")
                .and_then(|v| v.downcast::<CFString>())
                .map(|r| r.to_string());
            if role.as_deref() == Some("AXWebArea") {
                return Some(element);
            }
            if let Some(children) =
                attribute(&element, "AXChildren").and_then(|v| v.downcast::<CFArray>())
            {
                next.extend(
                    children
                        .iter()
                        .take(40)
                        .map(|child| unsafe { CFType::wrap_under_get_rule(*child as CFTypeRef) }),
                );
            }
        }
        if next.is_empty() || next.len() > 400 {
            return None;
        }
        level = next;
    }
    None
}

/// Processes already asked for their Accessibility tree, each with whether it took the request, so
/// each is asked (and waited for) once.
static MANUAL_ACCESSIBILITY: Mutex<Vec<(i32, bool)>> = Mutex::new(Vec::new());

/// Whether the frontmost app took `AXManualAccessibility` (Electron). Its tree may still be building
/// (VS Code takes over a second after its first request), so "nothing focused" proves nothing there.
fn frontmost_builds_tree_on_request() -> bool {
    let Some(pid) = objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
    else {
        return false;
    };
    MANUAL_ACCESSIBILITY.lock().unwrap().contains(&(pid, true))
}

/// Asks an Electron app to build its Accessibility tree; true when it took the request (other apps
/// do not know the attribute).
fn enable_manual_accessibility(app: &CFType, pid: i32) -> bool {
    if MANUAL_ACCESSIBILITY
        .lock()
        .unwrap()
        .iter()
        .any(|(asked, _)| *asked == pid)
    {
        return false;
    }
    let name = CFString::new("AXManualAccessibility");
    let error = unsafe {
        AXUIElementSetAttributeValue(
            app.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            CFBoolean::true_value().as_CFTypeRef(),
        )
    };
    MANUAL_ACCESSIBILITY.lock().unwrap().push((pid, error == 0));
    if error == 0 {
        eprintln!("Accessibility switched on in {}", frontmost_name());
    }
    error == 0
}

/// The focused text field as Accessibility shows it, for learning from corrections (`learn.rs`).
/// Only apps that expose their text have one (native fields, Safari, Mail, Notes); terminals and
/// many Electron apps do not.
pub struct Field {
    element: CFType,
    /// The text without the selection (a paste replaces it).
    pub text: String,
    /// Insertion point in UTF-16 units.
    pub at: usize,
}

// SAFETY: AXUIElement references may be used from any thread; Field is only read.
unsafe impl Send for Field {}

#[repr(C)]
#[derive(Default)]
struct CFRange {
    location: isize,
    length: isize,
}

const AX_VALUE_CF_RANGE: u32 = 4;

impl Field {
    pub fn focused() -> Option<Field> {
        let element = focused_element()?;
        let text = attribute(&element, "AXValue")?
            .downcast::<CFString>()?
            .to_string();
        let range = attribute(&element, "AXSelectedTextRange")?;
        if range.type_of() != unsafe { AXValueGetTypeID() } {
            return None;
        }
        let mut selected = CFRange::default();
        let ok = unsafe {
            AXValueGetValue(
                range.as_CFTypeRef(),
                AX_VALUE_CF_RANGE,
                (&mut selected as *mut CFRange).cast(),
            )
        };
        if !ok || selected.location < 0 || selected.length < 0 {
            return None;
        }
        let units: Vec<u16> = text.encode_utf16().collect();
        let (at, end) = (
            selected.location as usize,
            (selected.location + selected.length) as usize,
        );
        if end > units.len() {
            return None;
        }
        let text = String::from_utf16(&[&units[..at], &units[end..]].concat()).ok()?;
        Some(Field { element, text, at })
    }

    /// The field's text now, while it still has the focus.
    pub fn current_text(&self) -> Option<String> {
        if focused_element()? != self.element {
            return None;
        }
        Some(
            attribute(&self.element, "AXValue")?
                .downcast::<CFString>()?
                .to_string(),
        )
    }
}

fn accepts_text(role: &str) -> Option<bool> {
    match role {
        "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField" => Some(true),
        "AXList" | "AXOutline" | "AXTable" | "AXBrowser" | "AXButton" | "AXImage" | "AXWindow"
        | "AXApplication" | "AXMenu" | "AXMenuItem" | "AXCheckBox" | "AXRadioButton"
        | "AXSlider" | "AXPopUpButton" | "AXTabGroup" | "AXToolbar" | "AXDockItem"
        | "AXSplitGroup" => Some(false),
        _ => None,
    }
}

/// The text selected in the focused app: Accessibility's selected text where the app has it
/// (native apps, Safari), else ⌘C with the clipboard put back afterwards (browsers, terminals,
/// Electron). `None` when nothing is selected.
pub fn selected_text() -> Option<String> {
    if let Some(text) = focused_element()
        .and_then(|e| attribute(&e, "AXSelectedText"))
        .and_then(|v| v.downcast::<CFString>())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
    {
        return Some(text);
    }
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let previous = clipboard.get_text().ok();
    // A marker nobody copies: still there after ⌘C means nothing was selected.
    let marker = format!("dictate-selection-{}", std::process::id());
    clipboard.set_text(marker.clone()).ok()?;
    thread::sleep(Duration::from_millis(30));
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
    for key_down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), KEY_C, key_down).ok()?;
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, OWN_EVENT);
        event.post(CGEventTapLocation::HID);
    }
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

/// Pastes `text` into the focused app via the clipboard and ⌘V, then restores the clipboard.
pub fn paste(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("open clipboard")?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text).context("set clipboard")?;
    thread::sleep(Duration::from_millis(30));
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| anyhow!("event source"))?;
    for key_down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), KEY_V, key_down)
            .map_err(|()| anyhow!("keyboard event"))?;
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, OWN_EVENT);
        event.post(CGEventTapLocation::HID);
    }
    // Give the target app time to read the clipboard before we put the old content back.
    thread::sleep(Duration::from_millis(250));
    if let Some(previous) = previous {
        clipboard.set_text(previous).context("restore clipboard")?;
    }
    Ok(())
}

/// Plays a short system sound without blocking.
pub fn play(path: &std::path::Path, volume: f32) {
    if volume <= 0.0 {
        return;
    }
    let _ = std::process::Command::new("afplay")
        .arg("-v")
        .arg(format!("{volume:.2}"))
        .arg(path)
        .spawn();
}

/// Whether we muted the output (so only we unmute it, and quitting restores it).
static MUTED_BY_US: AtomicBool = AtomicBool::new(false);

fn osascript(script: &str) -> Option<String> {
    let out = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Mutes the default output unless it already is muted. Blocks ~0.1 s (osascript).
pub fn mute_output() {
    if osascript("output muted of (get volume settings)").as_deref() == Some("false")
        && osascript("set volume output muted true").is_some()
    {
        MUTED_BY_US.store(true, Ordering::Relaxed);
    }
}

/// Undoes [`mute_output`]; a no-op when we did not mute.
pub fn restore_output() {
    if MUTED_BY_US.swap(false, Ordering::Relaxed) {
        let _ = osascript("set volume output muted false");
    }
}

#[cfg(test)]
mod tests {
    use super::accepts_text;

    #[test]
    fn text_roles() {
        assert_eq!(accepts_text("AXTextArea"), Some(true));
        assert_eq!(accepts_text("AXOutline"), Some(false));
        assert_eq!(accepts_text("AXWebArea"), None);
    }
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeAudio: &'static objc2_foundation::NSString;
}

/// Microphone permission of this app: `Some(true)` granted, `Some(false)` denied or restricted,
/// `None` not asked yet.
pub fn microphone_permission() -> Option<bool> {
    let class = objc2::runtime::AnyClass::get(c"AVCaptureDevice")?;
    let status: isize =
        unsafe { objc2::msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] };
    match status {
        3 => Some(true),
        1 | 2 => Some(false),
        _ => None,
    }
}

/// Privacy panes of System Settings.
pub fn open_privacy(pane: &str) {
    let url = format!("x-apple.systempreferences:com.apple.preference.security?Privacy_{pane}");
    let _ = std::process::Command::new("open").arg(url).spawn();
}

/// The login agent's label: the app's bundle id (`scripts/bundle.sh`, `DICTATE_BUNDLE_ID`), so a
/// team build and the public one keep their own; the public default outside a bundle (CLI).
fn login_agent() -> String {
    objc2_foundation::NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|id| id.to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| "app.dictate".into())
}

fn login_agent_path() -> Option<std::path::PathBuf> {
    Some(
        dirs::home_dir()?
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", login_agent())),
    )
}

pub fn starts_at_login() -> bool {
    login_agent_path().is_some_and(|p| p.exists())
}

/// A LaunchAgent that opens this app bundle at login.
pub fn set_start_at_login(on: bool) -> Result<()> {
    let path = login_agent_path().ok_or_else(|| anyhow!("no home folder"))?;
    if !on {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        return Ok(());
    }
    let exe = std::env::current_exe()?.canonicalize()?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| anyhow!("only Dictate.app can start at login, not the command line tool"))?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{}</string>
  <key>ProgramArguments</key><array><string>/usr/bin/open</string><string>{}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
        login_agent(),
        bundle.display()
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, plist)?;
    Ok(())
}

/// The usable area (without menu bar and Dock) of the screen the mouse is on, as tao places
/// windows: logical points from the top left of the main screen, `(x, y, width, height)`.
/// None off the main thread.
pub fn mouse_screen() -> Option<(f64, f64, f64, f64)> {
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::MainThreadMarker;
    let mtm = MainThreadMarker::new()?;
    let mouse = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    let main_height = screens.firstObject()?.frame().size.height;
    let inside = |s: &NSScreen| {
        let f = s.frame();
        (f.origin.x..f.origin.x + f.size.width).contains(&mouse.x)
            && (f.origin.y..f.origin.y + f.size.height).contains(&mouse.y)
    };
    let screen = screens
        .iter()
        .find(|s| inside(s))
        .or_else(|| NSScreen::mainScreen(mtm))?;
    let v = screen.visibleFrame();
    let top = main_height - (v.origin.y + v.size.height);
    Some((v.origin.x, top, v.size.width, v.size.height))
}

/// Opens a link (https, mailto, notion://, …) with its default app.
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

/// A menu bar app: no Dock icon, no ⌘-Tab entry until a window opens.
pub fn menu_bar_app<T: 'static>(event_loop: &mut tao::event_loop::EventLoop<T>) {
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
}

/// The menu bar is always there.
pub fn tray() -> dictate_core::linux::Tray {
    dictate_core::linux::Tray::Shown
}

/// While a window is open Dictate is a regular app (Dock icon, ⌘-Tab, menu bar); back to a menu
/// bar app when it closes.
pub fn show_in_dock<T: 'static>(target: &tao::event_loop::EventLoopWindowTarget<T>, on: bool) {
    use tao::platform::macos::{ActivationPolicy, EventLoopWindowTargetExtMacOS};
    target.set_activation_policy_at_runtime(match on {
        true => ActivationPolicy::Regular,
        false => ActivationPolicy::Accessory,
    });
}

/// Brings the app to the front (before focusing one of its windows).
pub fn activate(mtm: MainThreadMarker) {
    objc2_app_kit::NSApplication::sharedApplication(mtm).activate();
}

/// The app's main menu, seen while a window is open (`app.rs` builds it). It lives as long as the app.
pub fn set_app_menu(menu: tray_icon::menu::Menu) {
    menu.init_for_nsapp();
    std::mem::forget(menu);
}

/// For the previews: an accessory app on the main thread, so AppKit views can be drawn. They
/// draw the HUD material: Liquid Glass is composited on screen only, a PNG of it stays empty.
pub fn preview_app() -> Result<MainThreadMarker> {
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow!("not on the main thread"))?;
    glass::without_glass();
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    Ok(mtm)
}

/// The system's version for the usage report ("26.1").
pub fn os_version() -> String {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "?".into())
}

/// The user's preferred languages, most preferred first ("de-DE", "en-US"; System Settings →
/// General → Language & Region).
pub fn preferred_languages() -> Vec<String> {
    objc2_foundation::NSLocale::preferredLanguages()
        .iter()
        .map(|l| l.to_string())
        .collect()
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

/// The system voice: `say` reading its text from stdin (`voice.rs` writes it and waits).
pub fn say(voice: &str, rate: u32) -> Result<std::process::Command> {
    let mut command = std::process::Command::new("/usr/bin/say");
    if !voice.is_empty() {
        command.args(["-v", voice]);
    }
    command.args(["-r", &rate.to_string()]);
    Ok(command)
}

/// Whether we were started as an app rather than from a terminal, beyond what stderr shows. On
/// macOS stderr tells (`main.rs` checks it), so: no.
pub fn launched_as_app() -> bool {
    false
}
