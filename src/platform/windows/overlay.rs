//! The recorder overlay on Windows (BETA, never on screen yet): a plain dark box at the bottom of
//! the primary screen, topmost and non-activating (`WS_EX_NOACTIVATE | WS_EX_TOPMOST |
//! WS_EX_LAYERED | WS_EX_TOOLWINDOW`), so the app you dictate into keeps the focus. Drawn with
//! GDI: the level bars (or a short status) on the left, the text beside them, the buttons
//! ("Kopieren", "Schließen", "Rückgängig") as a row of labels at the bottom; a click on a label
//! reports its tag. Not yet: struck-through words, the shortcuts beside the pill, rounded
//! corners, per-monitor DPI. Main thread only (tao's message loop runs its window procedure).

use std::cell::RefCell;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
    DT_WORDBREAK, DeleteObject, DrawTextW, EndPaint, FillRect, HGDIOBJ, InvalidateRect,
    PAINTSTRUCT, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW,
    LWA_ALPHA, LoadCursorW, MA_NOACTIVATE, RegisterClassW, SPI_GETWORKAREA, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_PAINT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

use super::MainThreadMarker;
use crate::overlay::{Left, MAX_LINES, SHOWN_CHARS, WAITING, bar_height, lines_for, tail};

const WIDTH: i32 = 560;
const LEFT_W: i32 = 120;
const LINE_H: i32 = 22;
const PAD: i32 = 12;
const BUTTONS_H: i32 = 30;
const BUTTON_W: i32 = 130;
/// Above the taskbar.
const BOTTOM: i32 = 56;

/// What the window draws, and who hears its clicks.
struct View {
    badge: String,
    levels: Option<Vec<f32>>,
    text: String,
    buttons: Vec<(String, isize)>,
    on_click: Box<dyn Fn(isize)>,
}

pub struct Overlay {
    hwnd: HWND,
    view: *const RefCell<View>,
}

impl Overlay {
    /// `on_click` gets the tag of a pressed button (`overlay::COPY`, …).
    pub fn new(_mtm: MainThreadMarker, on_click: impl Fn(isize) + 'static) -> Self {
        let view = Box::into_raw(Box::new(RefCell::new(View {
            badge: String::new(),
            levels: None,
            text: String::new(),
            buttons: Vec::new(),
            on_click: Box::new(on_click),
        })));
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("DictateOverlay"),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&class);
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
                w!("DictateOverlay"),
                w!("Dictate"),
                WS_POPUP,
                0,
                0,
                WIDTH,
                60,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap_or_default();
            if !hwnd.is_invalid() {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, view as isize);
            }
            hwnd
        };
        if hwnd.is_invalid() {
            eprintln!("overlay: could not create the window");
        }
        Overlay { hwnd, view }
    }

    pub fn show(&self, mtm: MainThreadMarker, left: Left, text: &str, buttons: &[(&str, isize)]) {
        self.show_marked(mtm, left, text, &[], buttons);
    }

    /// As `show`; struck-through words are not drawn on Windows yet (the text shows them plain).
    pub fn show_marked(
        &self,
        _mtm: MainThreadMarker,
        left: Left,
        text: &str,
        _struck: &[std::ops::Range<usize>],
        buttons: &[(&str, isize)],
    ) {
        let text = tail(text, SHOWN_CHARS);
        let lines = lines_for(&text).max(1) as i32;
        let height = PAD * 2 + lines * LINE_H + if buttons.is_empty() { 0 } else { BUTTONS_H };
        {
            let mut view = unsafe { &*self.view }.borrow_mut();
            match left {
                Left::Meter(levels) => {
                    view.levels = Some(levels.to_vec());
                    view.badge.clear();
                }
                Left::Symbol(symbol) => {
                    view.levels = None;
                    view.badge = symbol.to_string();
                }
            }
            view.text = text;
            view.buttons = buttons
                .iter()
                .map(|(t, tag)| (t.to_string(), *tag))
                .collect();
        }
        let area = work_area();
        let x = area.left + (area.right - area.left - WIDTH) / 2;
        let y = area.bottom - BOTTOM - height;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                WIDTH,
                height,
                SWP_NOACTIVATE,
            );
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    /// The shortcuts beside the pill: not on Windows yet.
    pub fn set_shortcuts(&self, _on: bool, _meeting: Option<String>) {}

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

/// The primary screen without the taskbar.
fn work_area() -> RECT {
    let mut area = RECT {
        left: 0,
        top: 0,
        right: 1280,
        bottom: 720,
    };
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut area as *mut RECT).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    area
}

/// Where the button row's labels are, for drawing and clicks.
fn button_rect(height: i32, index: usize) -> RECT {
    let left = LEFT_W + index as i32 * BUTTON_W;
    RECT {
        left,
        top: height - BUTTONS_H - PAD / 2,
        right: left + BUTTON_W,
        bottom: height - PAD / 2,
    }
}

fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

fn fill(hdc: windows::Win32::Graphics::Gdi::HDC, rect: &RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(hdc, rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn draw_text(hdc: windows::Win32::Graphics::Gdi::HDC, text: &str, rect: &mut RECT, flags: u32) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        DrawTextW(
            hdc,
            &mut wide,
            rect,
            windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags),
        );
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let view = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<View>;
    match message {
        // Clicks must not take the focus from the app the text goes to.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONUP if !view.is_null() => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut client = RECT::default();
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
            }
            let view = unsafe { &*view }.borrow();
            let hit = view.buttons.iter().enumerate().find(|(i, _)| {
                let r = button_rect(client.bottom, *i);
                (r.left..r.right).contains(&x) && (r.top..r.bottom).contains(&y)
            });
            if let Some((_, (_, tag))) = hit {
                (view.on_click)(*tag);
            }
            LRESULT(0)
        }
        WM_PAINT if !view.is_null() => {
            let view = unsafe { &*view }.borrow();
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut client = RECT::default();
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
                SetBkMode(hdc, TRANSPARENT);
            }
            fill(hdc, &client, rgb(28, 28, 30));
            match &view.levels {
                Some(levels) => {
                    // Red dot and the level bars, newest on the right.
                    let mid = PAD + LINE_H / 2;
                    let dot = RECT {
                        left: PAD,
                        top: mid - 5,
                        right: PAD + 10,
                        bottom: mid + 5,
                    };
                    fill(hdc, &dot, rgb(230, 57, 70));
                    for (i, db) in levels.iter().rev().take(6).enumerate() {
                        let h = (bar_height(*db) * 0.7) as i32;
                        let x = PAD + 90 - i as i32 * 12;
                        let bar = RECT {
                            left: x,
                            top: mid - h / 2,
                            right: x + 6,
                            bottom: mid + h / 2,
                        };
                        fill(hdc, &bar, rgb(235, 235, 235));
                    }
                }
                None => {
                    let mut badge = RECT {
                        left: PAD,
                        top: PAD,
                        right: LEFT_W - 8,
                        bottom: PAD + LINE_H,
                    };
                    unsafe { SetTextColor(hdc, rgb(235, 235, 235)) };
                    draw_text(
                        hdc,
                        &view.badge,
                        &mut badge,
                        (DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS).0,
                    );
                }
            }
            let dimmed = view.text == WAITING;
            unsafe {
                SetTextColor(
                    hdc,
                    match dimmed {
                        true => rgb(150, 150, 150),
                        false => rgb(255, 255, 255),
                    },
                )
            };
            let mut text = RECT {
                left: LEFT_W,
                top: PAD,
                right: client.right - PAD,
                bottom: PAD + MAX_LINES as i32 * LINE_H,
            };
            draw_text(
                hdc,
                crate::overlay::shown(&view.text),
                &mut text,
                (DT_LEFT | DT_WORDBREAK).0,
            );
            unsafe { SetTextColor(hdc, rgb(120, 180, 255)) };
            for (i, (label, _)) in view.buttons.iter().enumerate() {
                let mut r = button_rect(client.bottom, i);
                draw_text(hdc, label, &mut r, (DT_LEFT | DT_SINGLELINE | DT_VCENTER).0);
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
