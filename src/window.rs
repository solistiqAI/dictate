//! The main window: a tao window with a wry webview showing the page from `ui/` (Svelte), embedded
//! in the binary by `build.rs` and served as `dictate://localhost/…`. The page talks to the app
//! through `protocol.rs`. The webview exists only while the window is open. Main thread only.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};

use anyhow::{Context, Result};
use tao::dpi::LogicalSize;
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};
use wry::http::{Response, header::CONTENT_TYPE};
use wry::{WebView, WebViewBuilder};

use crate::protocol::{self, Event, Request};

mod assets {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

const NOT_BUILT: &str = "<!doctype html><meta charset=utf-8><body style='font:13px -apple-system;padding:40px'>\
    <h3>Die Oberfläche ist nicht gebaut</h3><p><code>cd ui &amp;&amp; npm ci &amp;&amp; npm run build</code>, \
    dann <code>cargo build --release</code> (oder <code>./scripts/bundle.sh</code>).</p>";

pub struct MainWindow {
    // Dropped before the window it lives in.
    webview: WebView,
    window: Window,
    /// Events sent before the page asked for its state ("load") wait here.
    loaded: Cell<bool>,
    pending: RefCell<Vec<Event>>,
}

impl MainWindow {
    /// Opens the window; `on_request` gets every message the page sends.
    pub fn open<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        on_request: impl Fn(Request) + 'static,
    ) -> Result<Self> {
        let builder = WindowBuilder::new()
            .with_title("Dictate")
            .with_inner_size(LogicalSize::new(980.0, 680.0))
            .with_min_inner_size(LogicalSize::new(760.0, 480.0));
        Self::build(
            target,
            builder,
            "dictate://localhost/index.html",
            on_request,
        )
    }

    /// The Souffleur's side panel: narrow, at the right edge of the screen, above other windows
    /// so it stays visible next to the call.
    pub fn open_panel<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        on_request: impl Fn(Request) + 'static,
    ) -> Result<Self> {
        let (width, height): (f64, f64) = (380.0, 760.0);
        let mut builder = WindowBuilder::new()
            .with_title("Souffleur")
            .with_inner_size(LogicalSize::new(width, height))
            .with_min_inner_size(LogicalSize::new(320.0, 420.0))
            .with_always_on_top(true);
        // On the screen you work on (where the mouse is), at its right edge below the menu bar.
        if let Some((x, y, w, h)) = crate::platform::mouse_screen() {
            let height = height.min(h - 24.0);
            builder = builder
                .with_inner_size(LogicalSize::new(width, height))
                .with_position(tao::dpi::LogicalPosition::new(
                    x + w - width - 16.0,
                    y + 12.0,
                ));
        }
        Self::build(
            target,
            builder,
            "dictate://localhost/index.html?view=panel",
            on_request,
        )
    }

    fn build<T: 'static>(
        target: &EventLoopWindowTarget<T>,
        builder: WindowBuilder,
        url: &str,
        on_request: impl Fn(Request) + 'static,
    ) -> Result<Self> {
        // The page draws its own title bar area on macOS.
        #[cfg(target_os = "macos")]
        let builder = {
            use tao::platform::macos::WindowBuilderExtMacOS;
            builder
                .with_titlebar_transparent(true)
                .with_title_hidden(true)
                .with_fullsize_content_view(true)
        };
        let window = builder.build(target).context("open the window")?;
        // WebView2 serves custom protocols as http://<name>.localhost/.
        let url = match cfg!(windows) {
            true => url.replace("dictate://localhost/", "http://dictate.localhost/"),
            false => url.to_string(),
        };
        let webview = WebViewBuilder::new()
            .with_custom_protocol("dictate".into(), |_, request| serve(request.uri().path()))
            .with_url(&url)
            .with_ipc_handler(move |message| match serde_json::from_str(message.body()) {
                Ok(request) => on_request(request),
                Err(e) => eprintln!("window: bad message {}: {e}", message.body()),
            })
            .with_devtools(cfg!(debug_assertions));
        // On Linux WebKitGTK goes into the window's GTK box (`build(&window)` would be X11 only).
        #[cfg(target_os = "linux")]
        let webview = {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;
            let vbox = window.default_vbox().context("the window's GTK box")?;
            webview.build_gtk(vbox)
        };
        #[cfg(not(target_os = "linux"))]
        let webview = webview.build(&window);
        let webview = webview.context("open the webview")?;
        window.set_focus();
        Ok(MainWindow {
            webview,
            window,
            loaded: Cell::new(false),
            pending: RefCell::new(Vec::new()),
        })
    }

    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    pub fn send(&self, event: &Event) {
        if !self.loaded.get() {
            self.pending.borrow_mut().push(event.clone());
            return;
        }
        if let Err(e) = self.webview.evaluate_script(&protocol::script(event)) {
            eprintln!("window: {e}");
        }
    }

    /// The page is up (it sent "load"): hand it what waited.
    pub fn loaded(&self) {
        self.loaded.set(true);
        for event in self.pending.take() {
            self.send(&event);
        }
    }

    pub fn focus(&self) {
        self.window.set_visible(true);
        self.window.set_minimized(false);
        self.window.set_focus();
    }

    pub fn drag(&self) {
        let _ = self.window.drag_window();
    }
}

/// A file of the embedded page.
fn serve(path: &str) -> Response<Cow<'static, [u8]>> {
    let name = path.trim_start_matches('/');
    let name = if name.is_empty() { "index.html" } else { name };
    let found = assets::ASSETS.iter().find(|(n, _)| *n == name);
    let (status, body, kind): (u16, Cow<'static, [u8]>, &str) = match found {
        Some((_, bytes)) => (200, Cow::Borrowed(bytes), mime(name)),
        None if name == "index.html" => (200, Cow::Borrowed(NOT_BUILT.as_bytes()), "text/html"),
        None => (404, Cow::Borrowed(b"not found"), "text/plain"),
    };
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, kind)
        .body(body)
        .expect("response")
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}
