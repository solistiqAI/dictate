//! The recorder overlay's platform-free part: what it shows, the button tags it reports, how
//! text is cut and sized. The window itself is the platform's (`platform/macos/overlay.rs`: an
//! AppKit panel; `platform/windows/overlay.rs`: a plain layered window), as `Overlay`.

pub use crate::platform::overlay::Overlay;

pub const MAX_LINES: usize = 5;
/// Rough characters per line at 15 pt in the card; used to size the card.
#[cfg_attr(target_os = "linux", allow(dead_code))] // GTK sizes the Linux card itself
pub const CHARS_PER_LINE: usize = 64;
/// Characters of live text we show; older text scrolls out at the top.
pub const SHOWN_CHARS: usize = 300;

/// The card's text while recording before anything is recognised, drawn dimmed: the card stays
/// up from the start instead of showing, vanishing on the first empty pass and coming back. A
/// marker: the overlays show it as `text::waiting()` ("Sprich…", "Speak…").
pub const WAITING: &str = "Sprich…";

/// What the card shows for `text`: the marker in the current language, else the text.
pub fn shown(text: &str) -> &str {
    if text == WAITING {
        crate::text::waiting()
    } else {
        text
    }
}

/// Button tags reported to the callback.
pub const COPY: isize = 1;
pub const CLOSE: isize = 2;
pub const UNDO: isize = 3;
/// Learn a correction the overlay asked about.
pub const REMEMBER: isize = 4;
/// The shortcuts beside the pill while recording.
pub const SETTINGS: isize = 10;
pub const SOUFFLEUR: isize = 11;

/// What the pill shows.
pub enum Left<'a> {
    /// Recording: red dot and level bars (dBFS, oldest first).
    Meter(&'a [f32]),
    /// A short status: "…" while transcribing, "Kein Textfeld", "!".
    Symbol(&'a str),
}

/// `tail` that keeps byte ranges into the text pointing at the same words.
#[cfg_attr(windows, allow(dead_code))] // the Windows overlay strikes nothing yet
pub fn tail_marked(
    text: &str,
    struck: &[std::ops::Range<usize>],
    max: usize,
) -> (String, Vec<std::ops::Range<usize>>) {
    let shown = tail(text, max);
    if shown == text {
        return (shown, struck.to_vec());
    }
    // The shown text is "…" + a suffix of `text`.
    let rest = &shown["…".len()..];
    let from = text.len() - rest.len();
    let ranges = struck
        .iter()
        .filter(|r| r.start >= from)
        .map(|r| r.start - from + "…".len()..r.end - from + "…".len())
        .collect();
    (shown, ranges)
}

/// Bar height in points for a level: 4 pt at -60 dBFS and below, 28 pt at -15 dBFS and above.
pub fn bar_height(db: f32) -> f64 {
    let t = ((db + 60.0) / 45.0).clamp(0.0, 1.0);
    4.0 + 24.0 * t as f64
}

/// How many card lines `text` needs (0 for no text), at most [`MAX_LINES`].
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn lines_for(text: &str) -> usize {
    let chars = text.trim().chars().count();
    if chars == 0 {
        0
    } else {
        chars.div_ceil(CHARS_PER_LINE).min(MAX_LINES)
    }
}

/// The last `max` characters, starting at a word, with an ellipsis when cut.
pub fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let cut: String = text.chars().skip(count - max).collect();
    let cut = cut.split_once(' ').map(|(_, rest)| rest).unwrap_or(&cut);
    format!("…{cut}")
}

#[cfg(test)]
mod tests {
    use super::{bar_height, lines_for, tail};

    #[test]
    fn bars_grow_with_level() {
        assert_eq!(bar_height(f32::NEG_INFINITY), 4.0);
        assert_eq!(bar_height(-37.5), 16.0);
        assert_eq!(bar_height(0.0), 28.0);
    }

    #[test]
    fn card_grows_with_text_up_to_five_lines() {
        assert_eq!(lines_for(""), 0);
        assert_eq!(lines_for("Hallo"), 1);
        assert_eq!(lines_for(&"x".repeat(65)), 2);
        assert_eq!(lines_for(&"x".repeat(5000)), 5);
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn struck_words_stay_on_their_words_when_the_text_is_cut() {
        let text = "eins zwei drei vier";
        assert_eq!(
            super::tail_marked(text, &[5..9, 15..19], 9),
            ("…vier".to_string(), vec![3..7])
        );
        assert_eq!(super::tail_marked(text, &[5..9], 100).1, vec![5..9]);
    }

    #[test]
    fn tail_keeps_short_text_and_cuts_long_text_at_a_word() {
        assert_eq!(tail("kurz", 10), "kurz");
        assert_eq!(tail("eins zwei drei vier", 9), "…vier");
    }
}
