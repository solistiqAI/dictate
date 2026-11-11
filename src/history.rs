//! The history of dictations (the window's "Verlauf"): the last transcripts with their time,
//! newest last, kept in `history.jsonl` in the data folder (one JSON object per line). Without
//! that file the transcripts already in `dictate.log` are taken over, without a time.
//! Platform independent.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Entries kept; the file is rewritten when it holds twice as many.
pub const KEEP: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds; `None` for transcripts taken over from the log.
    pub at: Option<i64>,
    pub text: String,
}

pub struct History {
    path: PathBuf,
    entries: Vec<Entry>,
    /// Lines in the file, to know when to rewrite it.
    lines: usize,
}

impl History {
    /// Reads `path`; when it does not exist yet, seeds it from the transcripts in `logs`.
    pub fn load(path: &Path, logs: &[PathBuf]) -> Self {
        let mut history = History {
            path: path.to_path_buf(),
            entries: Vec::new(),
            lines: 0,
        };
        match std::fs::read_to_string(path) {
            Ok(text) => {
                history.lines = text.lines().count();
                history.entries = text
                    .lines()
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect();
            }
            Err(_) => {
                history.entries = logs
                    .iter()
                    .filter_map(|log| std::fs::read_to_string(log).ok())
                    .flat_map(|text| from_log(&text))
                    .map(|text| Entry { at: None, text })
                    .collect();
                if let Err(e) = history.rewrite() {
                    eprintln!("history: {e:#}");
                }
            }
        }
        history.trim();
        history
    }

    /// Oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn push(&mut self, text: &str, at: i64) -> Result<()> {
        let entry = Entry {
            at: Some(at),
            text: text.to_string(),
        };
        self.entries.push(entry.clone());
        self.trim();
        if self.lines >= 2 * KEEP {
            return self.rewrite();
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("open {}", self.path.display()))?;
        writeln!(file, "{}", serde_json::to_string(&entry)?)?;
        self.lines += 1;
        Ok(())
    }

    /// After a correction: the entry keeps its time and gets the corrected text.
    pub fn replace(&mut self, index: usize, text: &str) -> Result<()> {
        let Some(entry) = self.entries.get_mut(index) else {
            return Ok(());
        };
        if entry.text == text {
            return Ok(());
        }
        entry.text = text.to_string();
        self.rewrite()
    }

    fn trim(&mut self) {
        let extra = self.entries.len().saturating_sub(KEEP);
        self.entries.drain(..extra);
    }

    fn rewrite(&mut self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = String::new();
        for entry in &self.entries {
            text.push_str(&serde_json::to_string(entry)?);
            text.push('\n');
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        std::fs::write(&tmp, text).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)?;
        self.lines = self.entries.len();
        Ok(())
    }
}

/// The transcripts in a `dictate.log`: lines like `4.2 s at -40 dBFS → 0.12 s: text`.
pub fn from_log(log: &str) -> Vec<String> {
    log.lines()
        .filter(|line| line.contains(" dBFS"))
        .filter_map(|line| line.split_once(" s: "))
        .filter(|(head, _)| head.contains('→'))
        .map(|(_, text)| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

/// "2026-11-11 08:52:29" in local time, for the log.
pub fn log_time() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    let (year, month, day, hour, minute) = crate::platform::local_time(now);
    // Time zones are whole minutes off UTC, so the seconds are UTC's.
    let second = now as u64 % 60;
    format!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dictate-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_transcripts_from_the_log() {
        let log = "model loaded\nBereit\nAufnahme…\n\
                   2.3 s at -47 dBFS → 0.06 s: Was denkst du darüber?\n\
                   1.0 s at -60 dBFS: no speech\n\
                   0.4 s too quiet (-70 dBFS), skipped\n\
                   6.7 s at -46 dBFS, trimmed 6.7 → 5.4 s → 0.13 s: Zwei: mit Doppelpunkt.\n";
        assert_eq!(
            from_log(log),
            ["Was denkst du darüber?", "Zwei: mit Doppelpunkt."]
        );
    }

    #[test]
    fn seeds_from_the_log_then_appends_and_survives_a_reload() {
        let dir = temp("seed");
        let log = dir.join("dictate.log");
        std::fs::write(&log, "1.0 s at -40 dBFS → 0.05 s: Aus dem Log\n").unwrap();
        let path = dir.join("history.jsonl");
        let mut history = History::load(&path, std::slice::from_ref(&log));
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].at, None);
        history.push("Neu diktiert", 1_793_000_000).unwrap();
        history.replace(0, "Aus dem Log, korrigiert").unwrap();

        // The log is read only once: a second load takes the file.
        std::fs::write(&log, "1.0 s at -40 dBFS → 0.05 s: Später im Log\n").unwrap();
        let again = History::load(&path, &[log]);
        let texts: Vec<_> = again.entries().iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["Aus dem Log, korrigiert", "Neu diktiert"]);
        assert_eq!(again.entries()[1].at, Some(1_793_000_000));
    }

    #[test]
    fn keeps_the_newest_and_rewrites_the_file_now_and_then() {
        let dir = temp("keep");
        let path = dir.join("history.jsonl");
        let mut history = History::load(&path, &[]);
        for n in 0..(2 * KEEP + 5) {
            history.push(&format!("Nummer {n}"), n as i64).unwrap();
        }
        assert_eq!(history.entries().len(), KEEP);
        assert_eq!(
            history.entries().last().unwrap().text,
            format!("Nummer {}", 2 * KEEP + 4)
        );
        let lines = std::fs::read_to_string(&path).unwrap().lines().count();
        assert!(lines < 2 * KEEP, "{lines} lines");
        let again = History::load(&path, &[]);
        assert_eq!(again.entries(), history.entries());
    }
}
