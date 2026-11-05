//! The bar at the bottom of the screen, platform-free part: its button tags and the Souffleur's
//! clock. The bar itself is the platform's (`platform/macos/bar.rs`; none on Windows yet).

pub use crate::platform::bar::Bar;

/// Button tags reported to the click callback.
pub const DICTATE: isize = 1;
pub const SOUFFLEUR: isize = 2;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // no bar on Windows and Linux yet
pub const SETTINGS: isize = 3;
pub const PANEL: isize = 4;
pub const STOP: isize = 5;

/// "4:05", "1:02:09".
pub fn elapsed(seconds: i64) -> String {
    let s = seconds.max(0);
    match s / 3600 {
        0 => format!("{}:{:02}", s / 60, s % 60),
        h => format!("{h}:{:02}:{:02}", s / 60 % 60, s % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time() {
        assert_eq!(elapsed(0), "0:00");
        assert_eq!(elapsed(245), "4:05");
        assert_eq!(elapsed(3729), "1:02:09");
        assert_eq!(elapsed(-5), "0:00");
    }
}
