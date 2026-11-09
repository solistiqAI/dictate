//! The recorder overlay at the bottom of the screen: a compact dark pill with the
//! level meter (or a short status) and, above it, a card with the text as it is recognised. When
//! nothing takes text, the card keeps the transcript with "Kopieren" and "Schließen".
//! A non-activating panel, so the app you dictate into keeps the focus. Main thread only.
//! The platform-free part (button tags, text cutting, sizes) is `src/overlay.rs`.

use objc2::rc::Retained;

use super::glass::Hud;

use crate::overlay::{
    Left, MAX_LINES, SETTINGS, SHOWN_CHARS, SOUFFLEUR, WAITING, bar_height, lines_for, tail_marked,
};
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSBackingStoreType,
    NSBitmapImageFileType, NSBox, NSBoxType, NSButton, NSCellImagePosition, NSColor, NSControl,
    NSFont, NSFontWeightMedium, NSImage, NSImageSymbolConfiguration, NSLineBreakMode, NSPanel,
    NSScreen, NSStatusWindowLevel, NSTextAlignment, NSTextField, NSTitlePosition, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSMutableAttributedString, NSNumber, NSObject, NSObjectProtocol, NSPoint,
    NSRange, NSRect, NSSize, NSString,
};

const WIDTH: f64 = 560.0;
const PILL_W: f64 = 168.0;
const PILL_H: f64 = 40.0;
const GAP: f64 = 10.0;
const LINE_H: f64 = 20.0;
const BARS: usize = 6;
/// Space for the button row in the card.
const ACTIONS_H: f64 = 44.0;

/// Their buttons: small and round (icon only; the Souffleur's time beside its icon when it runs),
/// centred on the pill's height, this far from it. The pill stays what the eye goes to.
const SIDE_GAP: f64 = 10.0;
const SIDE_H: f64 = 28.0;
/// Buttons the card can show at once.
const BUTTONS: usize = 3;
const BUTTON_W: f64 = 124.0;

type Callback = Box<dyn Fn(isize)>;

define_class!(
    // SAFETY: NSObject has no subclassing requirements; Target does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Callback]
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
    fn new(mtm: MainThreadMarker, callback: Callback) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(callback);
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Overlay {
    panel: Retained<NSPanel>,
    card: Hud,
    text: Retained<NSTextField>,
    buttons: Vec<Retained<NSButton>>,
    pill: Hud,
    dot: Retained<NSBox>,
    bars: Vec<Retained<NSBox>>,
    badge: Retained<NSTextField>,
    /// "Einstellungen" left of the pill, "Souffleur" right of it (capsule, button).
    sides: [(Hud, Retained<NSButton>); 2],
    shortcuts: std::cell::Cell<bool>,
    /// The running Souffleur's time ("12:03"), shown on its capsule.
    meeting: std::cell::RefCell<Option<String>>,
    _target: Retained<Target>,
}

impl Overlay {
    /// `on_click` gets the tag of a pressed button ([`COPY`], [`CLOSE`], …).
    pub fn new(mtm: MainThreadMarker, on_click: impl Fn(isize) + 'static) -> Self {
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, PILL_H));
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
        panel.setIgnoresMouseEvents(true);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setHidesOnDeactivate(false);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        // Dark like the HUD material always was: the glass follows the panel's appearance.
        if let Some(dark) = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }) {
            panel.setAppearance(Some(&dark));
        }
        unsafe { panel.setReleasedWhenClosed(false) };

        let root = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let card = Hud::new(mtm, 16.0, false);
        let text = label(mtm, 15.0);
        text.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        text.setMaximumNumberOfLines(MAX_LINES as isize);
        card.content.addSubview(&text);

        let target = Target::new(mtm, Box::new(on_click));
        let button = |title: &str, tag: isize| {
            let b = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(title),
                    Some(&target as &AnyObject),
                    Some(sel!(clicked:)),
                    mtm,
                )
            };
            b.setTag(tag);
            card.content.addSubview(&b);
            b
        };
        let buttons = (0..BUTTONS).map(|_| button("", 0)).collect();

        let pill = Hud::new(mtm, PILL_H / 2.0, false);
        let red = NSColor::systemRedColor();
        let dot = filled_box(mtm, &red, 4.5);
        dot.setFrame(NSRect::new(
            NSPoint::new(22.0, PILL_H / 2.0 - 4.5),
            NSSize::new(9.0, 9.0),
        ));
        pill.content.addSubview(&dot);
        let bars: Vec<_> = (0..BARS)
            .map(|_| {
                let bar = filled_box(mtm, &red, 1.5);
                pill.content.addSubview(&bar);
                bar
            })
            .collect();
        let badge = label(mtm, 13.0);
        badge.setAlignment(NSTextAlignment::Center);
        badge.setFrame(NSRect::new(
            NSPoint::new(10.0, 11.0),
            NSSize::new(PILL_W - 20.0, 18.0),
        ));
        pill.content.addSubview(&badge);

        let side = |title: &str, symbol: &str, tag: isize| {
            let capsule = Hud::new(mtm, SIDE_H / 2.0, false);
            let b = unsafe {
                NSButton::buttonWithTitle_target_action(
                    &NSString::from_str(""),
                    Some(&target as &AnyObject),
                    Some(sel!(clicked:)),
                    mtm,
                )
            };
            b.setTag(tag);
            b.setBordered(false);
            b.setToolTip(Some(&NSString::from_str(title)));
            let size = unsafe {
                NSImageSymbolConfiguration::configurationWithPointSize_weight(
                    12.0,
                    NSFontWeightMedium,
                )
            };
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(symbol),
                None,
            )
            .and_then(|i| i.imageWithSymbolConfiguration(&size));
            b.setImage(image.as_deref());
            b.setImagePosition(NSCellImagePosition::ImageLeading);
            b.setImageHugsTitle(true);
            b.setContentTintColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.72)));
            b.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
                11.0,
                unsafe { NSFontWeightMedium },
            )));
            capsule.content.addSubview(&b);
            root.addSubview(&capsule.view);
            (capsule, b)
        };
        let sides = [
            side(crate::text::settings(), "gearshape", SETTINGS),
            side("Souffleur", "text.bubble", SOUFFLEUR),
        ];

        root.addSubview(&card.view);
        root.addSubview(&pill.view);
        panel.setContentView(Some(&root));
        Overlay {
            panel,
            card,
            text,
            buttons,
            pill,
            dot,
            bars,
            badge,
            sides,
            shortcuts: std::cell::Cell::new(false),
            meeting: std::cell::RefCell::new(None),
            _target: target,
        }
    }

    /// Shows the pill and, when there is text, the card above it. `buttons` (title, tag; at most
    /// three, right-aligned) let the panel take clicks (only then; otherwise clicks go through to
    /// the app below).
    pub fn show(&self, mtm: MainThreadMarker, left: Left, text: &str, buttons: &[(&str, isize)]) {
        self.show_marked(mtm, left, text, &[], buttons);
    }

    /// As `show`, with the words at `struck` (byte ranges of `text`) dimmed and struck through:
    /// what "Korrigieren" takes out while you still speak.
    pub fn show_marked(
        &self,
        mtm: MainThreadMarker,
        left: Left,
        text: &str,
        struck: &[std::ops::Range<usize>],
        buttons: &[(&str, isize)],
    ) {
        self.set_left(left);
        self.layout(mtm, text, struck, buttons);
        self.panel
            .setIgnoresMouseEvents(buttons.is_empty() && !self.shortcuts.get());
        if !self.panel.isVisible() {
            self.panel.orderFrontRegardless();
        }
    }

    /// Shows "Einstellungen" and "Souffleur" beside the pill with the next `show` (while
    /// recording); `meeting` is the running Souffleur's time.
    pub fn set_shortcuts(&self, on: bool, meeting: Option<String>) {
        self.shortcuts.set(on);
        *self.meeting.borrow_mut() = meeting;
    }

    pub fn hide(&self) {
        self.panel.orderOut(None);
    }

    /// Draws the overlay into a PNG without putting it on screen (`dictate overlay-preview`;
    /// `set_shortcuts` first for the shortcuts beside the pill).
    pub fn render_png(
        &self,
        left: Left,
        text: &str,
        struck: &[std::ops::Range<usize>],
        buttons: &[(&str, isize)],
    ) -> Option<Vec<u8>> {
        self.set_left(left);
        self.layout(MainThreadMarker::from(&*self.panel), text, struck, buttons);
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

    fn set_left(&self, left: Left) {
        let meter = matches!(left, Left::Meter(_));
        self.dot.setHidden(!meter);
        for bar in &self.bars {
            bar.setHidden(!meter);
        }
        self.badge.setHidden(meter);
        match left {
            Left::Meter(levels) => {
                // Newest bar on the right; missing history shows as quiet. Centred in the pill.
                let quiet = BARS.saturating_sub(levels.len());
                for (i, bar) in self.bars.iter().enumerate() {
                    let db = if i < quiet {
                        f32::NEG_INFINITY
                    } else {
                        levels[levels.len() - BARS + i]
                    };
                    let height = bar_height(db).min(PILL_H - 12.0);
                    let x = 44.0 + i as f64 * 9.0;
                    bar.setFrame(NSRect::new(
                        NSPoint::new(x, (PILL_H - height) / 2.0),
                        NSSize::new(4.0, height),
                    ));
                }
            }
            Left::Symbol(symbol) => self.badge.setStringValue(&NSString::from_str(symbol)),
        }
    }

    /// Sizes the card to the text (up to five lines) and keeps the panel's bottom edge in place.
    fn layout(
        &self,
        mtm: MainThreadMarker,
        text: &str,
        struck: &[std::ops::Range<usize>],
        buttons: &[(&str, isize)],
    ) {
        let actions = !buttons.is_empty();
        let (text, struck) = tail_marked(text, struck, SHOWN_CHARS);
        let lines = lines_for(&text);
        // Measure the wrapped text; the estimate only caps it at MAX_LINES.
        self.text
            .setStringValue(&NSString::from_str(crate::overlay::shown(&text)));
        let color = match text == WAITING {
            true => NSColor::colorWithWhite_alpha(1.0, 0.5),
            false => NSColor::whiteColor(),
        };
        self.text.setTextColor(Some(&color));
        if !struck.is_empty() {
            self.text
                .setAttributedStringValue(&marked(&text, &struck, &color));
        }
        let text_w = WIDTH - 40.0;
        let measured = self
            .text
            .sizeThatFits(NSSize::new(text_w, 10_000.0))
            .height
            .ceil();
        let text_h = measured.min(MAX_LINES as f64 * LINE_H + 2.0).max(LINE_H);
        let card_h = if lines == 0 {
            0.0
        } else {
            24.0 + text_h + if actions { ACTIONS_H } else { 0.0 }
        };
        let height = card_h + if card_h > 0.0 { GAP } else { 0.0 } + PILL_H;

        self.pill.set_frame(NSRect::new(
            NSPoint::new((WIDTH - PILL_W) / 2.0, 0.0),
            NSSize::new(PILL_W, PILL_H),
        ));
        self.card.view.setHidden(card_h == 0.0);
        self.card.set_frame(NSRect::new(
            NSPoint::new(0.0, PILL_H + GAP),
            NSSize::new(WIDTH, card_h.max(1.0)),
        ));
        let text_y = if actions { ACTIONS_H } else { 0.0 } + 12.0;
        self.text.setFrame(NSRect::new(
            NSPoint::new(20.0, text_y),
            NSSize::new(text_w, text_h),
        ));
        // Right-aligned, the last one at the right edge.
        let shown = buttons.len().min(BUTTONS);
        for (i, button) in self.buttons.iter().enumerate() {
            button.setHidden(i >= shown);
            let Some((title, tag)) = buttons.get(i) else {
                continue;
            };
            button.setTitle(&NSString::from_str(title));
            button.setTag(*tag);
            let x = WIDTH - 20.0 - (shown - i) as f64 * (BUTTON_W + 8.0) + 8.0;
            button.setFrame(NSRect::new(
                NSPoint::new(x, 10.0),
                NSSize::new(BUTTON_W, 28.0),
            ));
        }

        // The shortcuts: small round buttons left and right of the pill; the Souffleur's one
        // grows to show its time.
        let on = self.shortcuts.get();
        let time = self.meeting.borrow().clone().unwrap_or_default();
        self.sides[1].1.setTitle(&NSString::from_str(&time));
        let pill_x = (WIDTH - PILL_W) / 2.0;
        for (i, (capsule, button)) in self.sides.iter().enumerate() {
            capsule.view.setHidden(!on);
            if !on {
                continue;
            }
            let labelled = i == 1 && !time.is_empty();
            button.setImagePosition(match labelled {
                true => NSCellImagePosition::ImageLeading,
                false => NSCellImagePosition::ImageOnly,
            });
            let w = match labelled {
                true => {
                    button.sizeToFit();
                    (button.frame().size.width + 18.0).ceil().max(SIDE_H)
                }
                false => SIDE_H,
            };
            let x = if i == 0 {
                pill_x - SIDE_GAP - w
            } else {
                pill_x + PILL_W + SIDE_GAP
            };
            let y = (PILL_H - SIDE_H) / 2.0;
            capsule.set_frame(NSRect::new(NSPoint::new(x, y), NSSize::new(w, SIDE_H)));
            button.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, SIDE_H)));
        }

        let origin = match NSScreen::mainScreen(mtm) {
            Some(screen) => {
                let visible = screen.visibleFrame();
                NSPoint::new(
                    visible.origin.x + (visible.size.width - WIDTH) / 2.0,
                    visible.origin.y + 56.0,
                )
            }
            None => self.panel.frame().origin,
        };
        self.panel
            .setFrame_display(NSRect::new(origin, NSSize::new(WIDTH, height)), true);
    }
}

/// `text` in the card's font and `color`, the `struck` byte ranges dimmed and struck through.
fn marked(
    text: &str,
    struck: &[std::ops::Range<usize>],
    color: &NSColor,
) -> Retained<NSMutableAttributedString> {
    let string = NSMutableAttributedString::from_nsstring(&NSString::from_str(text));
    let utf16 = |byte: usize| text[..byte].encode_utf16().count();
    let all = NSRange::new(0, utf16(text.len()));
    let dim = NSColor::colorWithWhite_alpha(1.0, 0.38);
    let line = NSNumber::new_isize(1);
    // SAFETY: the attribute names are AppKit's constants; values have the types they expect.
    unsafe {
        string.addAttribute_value_range(
            objc2_app_kit::NSFontAttributeName,
            &NSFont::systemFontOfSize(15.0),
            all,
        );
        string.addAttribute_value_range(objc2_app_kit::NSForegroundColorAttributeName, color, all);
        for range in struck {
            let at = NSRange::new(utf16(range.start), utf16(range.end) - utf16(range.start));
            string.addAttribute_value_range(
                objc2_app_kit::NSForegroundColorAttributeName,
                &dim,
                at,
            );
            string.addAttribute_value_range(
                objc2_app_kit::NSStrikethroughStyleAttributeName,
                &line,
                at,
            );
        }
    }
    string
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

fn label(mtm: MainThreadMarker, size: f64) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setTextColor(Some(&NSColor::whiteColor()));
    field
}
