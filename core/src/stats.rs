//! What happened, without what was said: one JSON line per dictation, skipped recording, problem
//! and learned correction in `stats.jsonl` in the data folder. `report.rs` condenses it into the
//! anonymous report that goes to the standing GitHub issue. Dictation text is never stored here;
//! the only words are the ones of a learned correction ("heard → meant") and short problem
//! messages with paths and addresses cut out. Platform independent.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Older events are dropped when the file is trimmed.
pub const KEEP_DAYS: i64 = 45;
/// The same problem is written at most this often (a microphone that stutters says so 50 times a
/// second).
const PROBLEM_EVERY: f32 = 10.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// A transcript that was delivered.
    Dictation {
        at: i64,
        /// Length of the recording.
        seconds: f32,
        level_db: f32,
        /// What is left after trimming silence.
        speech_seconds: f32,
        /// Wall time of the final transcription.
        took: f32,
        words: u32,
        model: String,
    },
    /// A recording nothing came of: `quiet` (below the noise floor) or `no_speech`.
    Skipped {
        at: i64,
        seconds: f32,
        level_db: f32,
        reason: String,
    },
    /// Something that went wrong: microphone, model, paste. Scrubbed, short.
    Problem { at: i64, what: String },
    /// A correction the app learned (dictionary word or replacement rule).
    Learned {
        at: i64,
        heard: String,
        meant: String,
    },
}

impl Event {
    pub fn at(&self) -> i64 {
        match self {
            Event::Dictation { at, .. }
            | Event::Skipped { at, .. }
            | Event::Problem { at, .. }
            | Event::Learned { at, .. } => *at,
        }
    }
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn path() -> PathBuf {
    crate::asr::data_dir().join("stats.jsonl")
}

/// Appends to the file in the data folder; a failure only goes to the log.
pub fn record(event: Event) {
    if let Event::Problem { what, .. } = &event {
        static LAST: Mutex<Option<(String, Instant)>> = Mutex::new(None);
        let mut last = LAST.lock().unwrap();
        if let Some((text, when)) = last.as_ref()
            && text == what
            && when.elapsed().as_secs_f32() < PROBLEM_EVERY
        {
            return;
        }
        *last = Some((what.clone(), Instant::now()));
    }
    if let Err(e) = append(&path(), &event) {
        eprintln!("stats: {e}");
    }
}

pub fn problem(what: &str) {
    record(Event::Problem {
        at: now(),
        what: scrub(what),
    });
}

fn append(path: &Path, event: &Event) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    file.write_all(line.as_bytes())
}

/// The events at or after `since` (unix seconds), oldest first; lines that do not parse are skipped.
pub fn load(path: &Path, since: i64) -> Vec<Event> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Event>(line).ok())
        .filter(|e| e.at() >= since)
        .collect()
}

/// Drops what is older than `KEEP_DAYS`. Called once at start-up.
pub fn trim(path: &Path, now: i64) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let cut = now - KEEP_DAYS * 86_400;
    let keep: Vec<&str> = text
        .lines()
        .filter(|line| {
            serde_json::from_str::<Event>(line)
                .map(|e| e.at() >= cut)
                .unwrap_or(false)
        })
        .collect();
    if keep.len() < text.lines().count() {
        let mut out = keep.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        let _ = std::fs::write(path, out);
    }
}

/// A message safe to send: the home folder becomes `~`, anything that looks like an address, a
/// link or a path is cut, and it is short.
pub fn scrub(message: &str) -> String {
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    let words: Vec<String> = message
        .split_whitespace()
        .map(|word| {
            let word = match &home {
                Some(home) => word.replace(home.as_str(), "~"),
                None => word.to_string(),
            };
            if word.contains('@') || word.contains("://") {
                "<…>".to_string()
            } else if word.starts_with('/') || word.starts_with("~/") {
                "<path>".to_string()
            } else {
                word
            }
        })
        .collect();
    let mut out = words.join(" ");
    if out.chars().count() > 160 {
        out = out.chars().take(160).collect::<String>() + "…";
    }
    out
}

/// A word pair of a learned correction fit to share: short, no digits, nothing that looks like an
/// address or a path. Names and numbers are what a report must not carry.
pub fn shareable(word: &str) -> bool {
    !word.is_empty()
        && word.chars().count() <= 40
        && !word
            .chars()
            .any(|c| c.is_ascii_digit() || "@/:\\#".contains(c))
}

/// "2026-11-01" for unix seconds (UTC), without a date crate.
pub fn day(at: i64) -> String {
    let z = at.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictation(at: i64) -> Event {
        Event::Dictation {
            at,
            seconds: 4.0,
            level_db: -40.0,
            speech_seconds: 3.0,
            took: 0.2,
            words: 9,
            model: "m".into(),
        }
    }

    #[test]
    fn days_are_right() {
        assert_eq!(day(0), "1970-01-01");
        assert_eq!(day(1_793_491_200), "2026-11-01");
        assert_eq!(day(1_709_164_800), "2024-02-29");
    }

    #[test]
    fn events_round_trip_and_old_ones_go() {
        let dir = std::env::temp_dir().join(format!("dictate-stats-{}", std::process::id()));
        let file = dir.join("stats.jsonl");
        let _ = std::fs::remove_dir_all(&dir);
        let now = 100 * 86_400;
        append(&file, &dictation(now - 60 * 86_400)).unwrap();
        append(&file, &dictation(now - 3600)).unwrap();
        append(
            &file,
            &Event::Problem {
                at: now,
                what: "x".into(),
            },
        )
        .unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        assert_eq!(load(&file, 0).len(), 3);
        assert_eq!(load(&file, now - 86_400).len(), 2);
        trim(&file, now);
        assert_eq!(load(&file, 0).len(), 2);
        assert!(!std::fs::read_to_string(&file).unwrap().contains("not json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dictation_text_has_no_place_in_the_file() {
        let line = serde_json::to_string(&dictation(1)).unwrap();
        assert!(!line.contains("text"), "{line}");
    }

    #[test]
    fn messages_lose_paths_and_addresses() {
        let home = dirs::home_dir().unwrap();
        let message = format!(
            "open {}/Library/x failed for a.b@example.com at https://x.y /tmp/a",
            home.display()
        );
        let out = scrub(&message);
        assert!(!out.contains(home.to_str().unwrap()), "{out}");
        assert!(!out.contains('@') && !out.contains("https"), "{out}");
        assert!(out.contains("<path>"), "{out}");
        assert!(scrub(&"a ".repeat(200)).chars().count() <= 161);
    }

    #[test]
    fn only_plain_words_are_shared() {
        assert!(shareable("deployen"));
        assert!(shareable("Pull Request"));
        assert!(!shareable("anna@x.de"));
        assert!(!shareable("IBAN DE12"));
        assert!(!shareable("/Users/a"));
        assert!(!shareable(""));
    }
}
