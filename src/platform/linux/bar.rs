//! No bar at the bottom of the screen on Linux yet (the tray menu does its job). Same calls as
//! the macOS bar, doing nothing.

use super::MainThreadMarker;

pub struct Bar;

impl Bar {
    pub fn new(_mtm: MainThreadMarker, _on_click: impl Fn(isize) + 'static) -> Self {
        Bar
    }

    pub fn set_visible(&self, _mtm: MainThreadMarker, _on: bool) {}

    pub fn set_meeting(&self, _mtm: MainThreadMarker, _started: Option<i64>) {}

    pub fn set_recording(&self, _mtm: MainThreadMarker, _on: bool) {}

    pub fn tick(&self, _mtm: MainThreadMarker, _now: i64) {}
}
