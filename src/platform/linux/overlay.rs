//! The recorder overlay on Linux (BETA, never on screen yet): a small dark GTK window at the
//! bottom of the primary monitor, the level bars (or a short status) on the left, the text
//! beside them (struck-through words as Pango markup), the buttons ("Kopieren", "Schließen",
//! "Rückgängig") below. On X11 a popup window (override-redirect): always on top, never takes the
//! focus. Wayland lets no app place or raise its windows: there it is an undecorated window that
//! asks to stay above and not take the focus, where the compositor puts it (often the middle of
//! the screen); compositors may ignore both wishes. Not yet: the shortcuts beside the pill.
//! Main thread only (GTK's, which tao runs).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib::prelude::*;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib, pango};

use super::MainThreadMarker;
use crate::overlay::{Left, MAX_LINES, SHOWN_CHARS, WAITING, bar_height, tail_marked};

const WIDTH: i32 = 560;
const METER_W: i32 = 110;
const METER_H: i32 = 32;
/// Above the panel or dock.
const BOTTOM: i32 = 56;

const CSS: &str = "
.dictate-overlay { background-color: rgba(28, 28, 30, 0.94); border-radius: 14px; }
.dictate-overlay label { color: #f2f2f2; font-size: 15px; }
.dictate-overlay .badge { font-weight: bold; }
.dictate-overlay button { background: none; border: none; box-shadow: none; color: #78b4ff; padding: 2px 10px; }
.dictate-overlay button label { color: #78b4ff; }
.dictate-overlay button:hover { background-color: rgba(255, 255, 255, 0.08); }
";

pub struct Overlay {
    window: gtk::Window,
    meter: gtk::DrawingArea,
    badge: gtk::Label,
    text: gtk::Label,
    buttons: gtk::Box,
    levels: Rc<RefCell<Vec<f32>>>,
    on_click: Rc<dyn Fn(isize)>,
}

/// Whether GDK talks to a Wayland compositor (else X11).
fn on_wayland() -> bool {
    gdk::Display::default().is_some_and(|d| d.type_().name() == "GdkWaylandDisplay")
}

impl Overlay {
    /// `on_click` gets the tag of a pressed button (`overlay::COPY`, …).
    pub fn new(_mtm: MainThreadMarker, on_click: impl Fn(isize) + 'static) -> Self {
        let window = match on_wayland() {
            true => {
                let window = gtk::Window::new(gtk::WindowType::Toplevel);
                window.set_decorated(false);
                window.set_keep_above(true);
                window.set_accept_focus(false);
                window.set_focus_on_map(false);
                window.set_skip_taskbar_hint(true);
                window.set_skip_pager_hint(true);
                window.set_type_hint(gdk::WindowTypeHint::Notification);
                window
            }
            false => gtk::Window::new(gtk::WindowType::Popup),
        };
        window.set_title("Dictate");
        window.set_resizable(false);
        window.set_app_paintable(true);
        // Rounded corners need a window that can be transparent (a compositor); else square.
        if let Some(screen) = WidgetExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
            let provider = gtk::CssProvider::new();
            if provider.load_from_data(CSS.as_bytes()).is_ok() {
                gtk::StyleContext::add_provider_for_screen(
                    &screen,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }
        window.style_context().add_class("dictate-overlay");

        let levels: Rc<RefCell<Vec<f32>>> = Rc::default();
        let meter = gtk::DrawingArea::new();
        meter.set_size_request(METER_W, METER_H);
        {
            let levels = levels.clone();
            meter.connect_draw(move |_, cr| {
                draw_meter(cr, &levels.borrow());
                glib::Propagation::Proceed
            });
        }
        let badge = gtk::Label::new(None);
        badge.style_context().add_class("badge");
        badge.set_size_request(METER_W, -1);
        badge.set_xalign(0.0);
        badge.set_ellipsize(pango::EllipsizeMode::End);
        let text = gtk::Label::new(None);
        text.set_xalign(0.0);
        text.set_line_wrap(true);
        text.set_line_wrap_mode(pango::WrapMode::WordChar);
        text.set_lines(MAX_LINES as i32);
        text.set_ellipsize(pango::EllipsizeMode::End);
        text.set_hexpand(true);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.pack_start(&meter, false, false, 0);
        row.pack_start(&badge, false, false, 0);
        row.pack_start(&text, true, true, 0);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.set_border_width(12);
        column.pack_start(&row, false, false, 0);
        column.pack_start(&buttons, false, false, 0);
        window.add(&column);
        window.set_size_request(WIDTH, -1);
        Overlay {
            window,
            meter,
            badge,
            text,
            buttons,
            levels,
            on_click: Rc::new(on_click),
        }
    }

    pub fn show(&self, mtm: MainThreadMarker, left: Left, text: &str, buttons: &[(&str, isize)]) {
        self.show_marked(mtm, left, text, &[], buttons);
    }

    /// As `show`, with words "Korrigieren" takes out drawn struck through.
    pub fn show_marked(
        &self,
        _mtm: MainThreadMarker,
        left: Left,
        text: &str,
        struck: &[std::ops::Range<usize>],
        buttons: &[(&str, isize)],
    ) {
        match left {
            Left::Meter(levels) => {
                *self.levels.borrow_mut() = levels.to_vec();
                self.badge.hide();
                self.meter.show();
                self.meter.queue_draw();
            }
            Left::Symbol(symbol) => {
                self.badge.set_text(symbol);
                self.meter.hide();
                self.badge.show();
            }
        }
        self.text.set_markup(&markup(text, struck));
        self.text.set_visible(!text.trim().is_empty());
        for child in self.buttons.children() {
            self.buttons.remove(&child);
        }
        for (label, tag) in buttons {
            let button = gtk::Button::with_label(label);
            button.set_can_focus(false);
            let (on_click, tag) = (self.on_click.clone(), *tag);
            button.connect_clicked(move |_| on_click(tag));
            self.buttons.pack_start(&button, false, false, 0);
            button.show();
        }
        self.buttons.set_visible(!buttons.is_empty());
        self.window.show();
        self.place();
    }

    /// Bottom centre of the primary monitor's work area, at the height the content needs
    /// (Wayland ignores the position).
    fn place(&self) {
        self.window.resize(WIDTH, 1);
        let (_, height) = self.window.preferred_height_for_width(WIDTH);
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let Some(monitor) = display.primary_monitor().or_else(|| display.monitor(0)) else {
            return;
        };
        let area = monitor.workarea();
        let x = area.x() + (area.width() - WIDTH) / 2;
        let y = area.y() + area.height() - BOTTOM - height;
        self.window.move_(x, y);
    }

    /// The shortcuts beside the pill: not on Linux yet.
    pub fn set_shortcuts(&self, _on: bool, _meeting: Option<String>) {}

    pub fn hide(&self) {
        self.window.hide();
    }
}

/// The card's text as Pango markup: the tail that fits, struck words in `<s>`, the "Sprich…"
/// marker dimmed.
fn markup(text: &str, struck: &[std::ops::Range<usize>]) -> String {
    if text == WAITING {
        let waiting = glib::markup_escape_text(crate::overlay::shown(text));
        return format!("<span foreground=\"#9a9a9a\">{waiting}</span>");
    }
    let (text, struck) = tail_marked(text, struck, SHOWN_CHARS);
    let mut out = String::new();
    let mut at = 0;
    let mut ranges = struck.clone();
    ranges.sort_by_key(|r| r.start);
    for range in ranges {
        let (Some(before), Some(word)) = (text.get(at..range.start), text.get(range.clone()))
        else {
            continue; // overlapping or not on a character boundary
        };
        out.push_str(&glib::markup_escape_text(before));
        out.push_str(&format!(
            "<span strikethrough=\"true\" foreground=\"#9a9a9a\">{}</span>",
            glib::markup_escape_text(word)
        ));
        at = range.end;
    }
    out.push_str(&glib::markup_escape_text(
        text.get(at..).unwrap_or_default(),
    ));
    out
}

/// The red dot and the level bars, newest on the right.
fn draw_meter(cr: &cairo::Context, levels: &[f32]) {
    let mid = METER_H as f64 / 2.0;
    cr.set_source_rgb(0.90, 0.22, 0.27);
    cr.arc(6.0, mid, 5.0, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
    cr.set_source_rgb(0.92, 0.92, 0.92);
    for (i, db) in levels.iter().rev().take(6).enumerate() {
        let h = bar_height(*db).min(METER_H as f64);
        let x = 96.0 - i as f64 * 12.0;
        cr.rectangle(x, mid - h / 2.0, 6.0, h);
    }
    let _ = cr.fill();
}
