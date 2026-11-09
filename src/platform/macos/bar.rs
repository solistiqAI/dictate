//! The bar at the bottom of the screen: always there, a row of small buttons ("Diktieren",
//! "Souffleur", "Einstellungen"; "Fertig" while a dictation runs). While the Souffleur runs it shows
//! a dot and the time since it started, and its buttons bring the side panel back or end it. Just
//! above the Dock, below the recorder overlay, on the screen you work on. A non-activating panel,
//! so clicking it leaves the focus where it was (a dictation started here goes into the app you
//! were in). Main thread only. Button tags and the clock text: `src/bar.rs`.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;

use super::glass::Hud;

use crate::bar::{DICTATE, PANEL, SETTINGS, SOUFFLEUR, STOP, elapsed};
use crate::text;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSBackingStoreType,
    NSBezelStyle, NSBitmapImageFileType, NSBox, NSBoxType, NSButton, NSColor, NSControl,
    NSControlSize, NSFont, NSPanel, NSScreen, NSStatusWindowLevel, NSTextAlignment, NSTextField,
    NSTitlePosition, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

const ROW_H: f64 = 32.0;
const PAD: f64 = 5.0;
const SPACING: f64 = 4.0;
/// Space for dot and time before the buttons while the Souffleur runs.
const CLOCK_W: f64 = 64.0;
/// Bottom edge above the bottom of the visible screen (the Dock's top); the overlay's pill starts
/// at 56, so both fit.
const BOTTOM: f64 = 10.0;

type Click = Box<dyn Fn(isize)>;

define_class!(
    // SAFETY: NSObject has no subclassing requirements; Target does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Click]
    struct Target;

    impl Target {
        #[unsafe(method(clicked:))]
        fn clicked(&self, sender: &NSControl) {
            (self.ivars())(sender.tag());
        }
    }

    unsafe impl NSObjectProtocol for Target {}
);

impl Target {
    fn new(mtm: MainThreadMarker, callback: Click) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(callback);
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Bar {
    panel: Retained<NSPanel>,
    pill: Hud,
    dot: Retained<NSBox>,
    time: Retained<NSTextField>,
    buttons: Vec<Retained<NSButton>>,
    _target: Retained<Target>,
    /// Unix seconds the running Souffleur started.
    meeting: Cell<Option<i64>>,
    recording: Cell<bool>,
    visible: Cell<bool>,
    /// The visible frame of the screen it was placed on, to follow when that changes.
    screen: Cell<Option<NSRect>>,
    shown_time: RefCell<String>,
}

impl Bar {
    /// `on_click` gets a button's tag ([`DICTATE`], …).
    pub fn new(mtm: MainThreadMarker, on_click: impl Fn(isize) + 'static) -> Self {
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(300.0, ROW_H));
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        panel.setLevel(NSStatusWindowLevel);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(true);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setHidesOnDeactivate(false);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        if let Some(dark) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
            panel.setAppearance(Some(&dark));
        }
        unsafe { panel.setReleasedWhenClosed(false) };

        let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let pill = Hud::new(mtm, ROW_H / 2.0, true);
        let red = NSColor::systemRedColor();
        let dot = filled_box(mtm, &red, 3.5);
        pill.content.addSubview(&dot);
        let time = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        time.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
            12.0, 0.23,
        )));
        time.setTextColor(Some(&NSColor::whiteColor()));
        time.setAlignment(NSTextAlignment::Left);
        pill.content.addSubview(&time);

        let target = Target::new(mtm, Box::new(on_click));
        let buttons = (0..3)
            .map(|_| {
                let b = unsafe {
                    NSButton::buttonWithTitle_target_action(
                        &NSString::from_str(""),
                        Some(&target as &AnyObject),
                        Some(sel!(clicked:)),
                        mtm,
                    )
                };
                b.setBezelStyle(NSBezelStyle::AccessoryBarAction);
                b.setControlSize(NSControlSize::Small);
                b.setFont(Some(&NSFont::systemFontOfSize_weight(12.0, 0.23)));
                pill.content.addSubview(&b);
                b
            })
            .collect();
        root.addSubview(&pill.view);
        panel.setContentView(Some(&root));
        Bar {
            panel,
            pill,
            dot,
            time,
            buttons,
            _target: target,
            meeting: Cell::new(None),
            recording: Cell::new(false),
            visible: Cell::new(false),
            screen: Cell::new(None),
            shown_time: RefCell::new(String::new()),
        }
    }

    /// On screen or not (switched off in the settings).
    pub fn set_visible(&self, mtm: MainThreadMarker, on: bool) {
        if on == self.visible.get() {
            return;
        }
        self.visible.set(on);
        if on {
            self.layout(mtm);
            self.panel.orderFrontRegardless();
        } else {
            self.panel.orderOut(None);
        }
    }

    /// The Souffleur started (`Some(unix seconds)`) or ended (`None`).
    pub fn set_meeting(&self, mtm: MainThreadMarker, started: Option<i64>) {
        if started != self.meeting.get() {
            self.meeting.set(started);
            self.layout(mtm);
        }
    }

    /// A dictation runs: "Diktieren" becomes "Fertig".
    pub fn set_recording(&self, mtm: MainThreadMarker, on: bool) {
        if on != self.recording.get() {
            self.recording.set(on);
            self.layout(mtm);
        }
    }

    /// Once a second: the Souffleur's time, and the screen you work on (it moves along).
    pub fn tick(&self, mtm: MainThreadMarker, now: i64) {
        if let Some(started) = self.meeting.get() {
            let text = elapsed(now - started);
            if *self.shown_time.borrow() != text {
                self.time.setStringValue(&NSString::from_str(&text));
                *self.shown_time.borrow_mut() = text;
            }
        }
        if self.visible.get() {
            let screen = NSScreen::mainScreen(mtm).map(|s| s.visibleFrame());
            if screen != self.screen.get() {
                self.layout(mtm);
            }
        }
    }

    fn layout(&self, mtm: MainThreadMarker) {
        let running = self.meeting.get().is_some();
        if let Some(started) = self.meeting.get() {
            self.shown_time.borrow_mut().clear();
            self.tick(mtm, started.max(now_unix()));
        }
        let dictate = if self.recording.get() {
            text::done()
        } else {
            text::dictate()
        };
        let titles: [(&str, isize); 3] = if running {
            [
                (text::side_panel(), PANEL),
                (text::stop_souffleur(), STOP),
                (text::settings(), SETTINGS),
            ]
        } else {
            [
                (dictate, DICTATE),
                ("Souffleur", SOUFFLEUR),
                (text::settings(), SETTINGS),
            ]
        };
        let mut x = PAD + if running { CLOCK_W } else { 0.0 };
        for (button, (title, tag)) in self.buttons.iter().zip(titles) {
            button.setTitle(&NSString::from_str(title));
            button.setTag(tag);
            button.sizeToFit();
            let size = button.frame().size;
            let w = size.width + 10.0;
            button.setFrame(NSRect::new(
                NSPoint::new(x, (ROW_H - size.height) / 2.0),
                NSSize::new(w, size.height),
            ));
            x += w + SPACING;
        }
        let size = NSSize::new(x - SPACING + PAD, ROW_H);
        self.pill.set_radius(ROW_H / 2.0);
        self.pill
            .set_frame(NSRect::new(NSPoint::new(0.0, 0.0), size));
        self.dot.setHidden(!running);
        self.time.setHidden(!running);
        self.dot.setFrame(NSRect::new(
            NSPoint::new(PAD + 9.0, ROW_H / 2.0 - 3.5),
            NSSize::new(7.0, 7.0),
        ));
        self.time.setFrame(NSRect::new(
            NSPoint::new(PAD + 22.0, ROW_H / 2.0 - 8.0),
            NSSize::new(CLOCK_W - 20.0, 16.0),
        ));

        let screen = NSScreen::mainScreen(mtm).map(|s| s.visibleFrame());
        self.screen.set(screen);
        let origin = match screen {
            Some(visible) => NSPoint::new(
                visible.origin.x + (visible.size.width - size.width) / 2.0,
                visible.origin.y + BOTTOM,
            ),
            None => self.panel.frame().origin,
        };
        self.panel.setFrame_display(NSRect::new(origin, size), true);
    }

    /// Draws the bar into a PNG without putting it on screen (`dictate bar-preview`).
    pub fn render_png(&self, recording: bool, meeting: Option<i64>) -> Option<Vec<u8>> {
        let mtm = MainThreadMarker::from(&*self.panel);
        self.recording.set(recording);
        self.meeting.set(meeting);
        self.layout(mtm);
        self.panel
            .setFrameOrigin(NSPoint::new(-20_000.0, -20_000.0));
        self.panel.orderFrontRegardless();
        self.panel.displayIfNeeded();
        let view = self.panel.contentView()?;
        let bounds = view.bounds();
        let rep = view.bitmapImageRepForCachingDisplayInRect(bounds)?;
        view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
        let data = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        self.panel.orderOut(None);
        Some(data.to_vec())
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

fn filled_box(mtm: MainThreadMarker, color: &NSColor, radius: f64) -> Retained<NSBox> {
    let b = NSBox::new(mtm);
    b.setBoxType(NSBoxType::Custom);
    b.setTitlePosition(NSTitlePosition::NoTitle);
    b.setBorderWidth(0.0);
    b.setCornerRadius(radius);
    b.setFillColor(color);
    b
}
