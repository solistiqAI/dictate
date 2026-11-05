//! The anonymous usage report: `stats.jsonl` condensed into numbers (how many dictations, how long
//! the model took, how loud, what went wrong, which corrections were learned), sent where the
//! profile says (`[report]`): a comment on a standing issue of a private GitHub repo, or JSON to a
//! URL of the team's own. Whoever improves Dictate for the team reads it there. It carries no
//! dictation text, no name and no path; see `stats.rs` for what is stored at all. Platform
//! independent.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::stats::{self, Event};

/// One report per this many seconds, and one covers at most `MAX_DAYS`.
pub const EVERY: i64 = 86_400;
const MAX_DAYS: i64 = 7;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub from: String,
    pub to: String,
    pub dictations: usize,
    pub words: u64,
    pub audio_seconds: f64,
    pub speech_seconds: f64,
    /// Transcription time per second of audio, over all dictations (0.05 = 20x real time).
    pub took_per_audio_second: f64,
    pub took_median: f64,
    pub took_p95: f64,
    pub level_db_median: f64,
    pub level_db_p10: f64,
    pub skipped_quiet: usize,
    pub skipped_no_speech: usize,
    /// Length and level of the recordings dropped as too quiet: short taps or a quiet microphone?
    #[serde(default)]
    pub skipped_quiet_seconds_median: f64,
    #[serde(default)]
    pub skipped_quiet_level_db_median: f64,
    /// How many of them were shorter than a second.
    #[serde(default)]
    pub skipped_quiet_short: usize,
    #[serde(default)]
    pub skipped_no_speech_seconds_median: f64,
    #[serde(default)]
    pub skipped_no_speech_level_db_median: f64,
    pub problems: Vec<(String, usize)>,
    pub learned: Vec<(String, String)>,
    pub per_day: Vec<(String, usize)>,
    pub models: Vec<(String, usize)>,
}

/// What the report says about the install, from the settings and the build.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub install: String,
    pub version: String,
    pub macos: String,
    pub arch: String,
    pub model: String,
    pub language: String,
    pub boost: bool,
    pub learning: bool,
    pub dictionary_words: usize,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let at = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[at]
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    percentile(&values, 0.5)
}

fn counted(items: impl Iterator<Item = String>, limit: usize) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(limit);
    out
}

pub fn summarize(events: &[Event], from: i64, to: i64) -> Summary {
    let mut s = Summary {
        from: stats::day(from),
        to: stats::day(to),
        ..Summary::default()
    };
    let (mut took, mut levels) = (Vec::new(), Vec::new());
    let mut took_sum = 0.0;
    let mut days = Vec::new();
    let mut models = Vec::new();
    let mut problems = Vec::new();
    let mut learned: Vec<(String, String)> = Vec::new();
    let (mut quiet_seconds, mut quiet_levels) = (Vec::new(), Vec::new());
    let (mut no_speech_seconds, mut no_speech_levels) = (Vec::new(), Vec::new());
    for event in events {
        match event {
            Event::Dictation {
                at,
                seconds,
                level_db,
                speech_seconds,
                took: t,
                words,
                model,
            } => {
                s.dictations += 1;
                s.words += *words as u64;
                s.audio_seconds += *seconds as f64;
                s.speech_seconds += *speech_seconds as f64;
                took_sum += *t as f64;
                took.push(*t as f64);
                levels.push(*level_db as f64);
                days.push(stats::day(*at));
                models.push(model.clone());
            }
            Event::Skipped {
                seconds,
                level_db,
                reason,
                ..
            } => match reason.as_str() {
                "quiet" => {
                    s.skipped_quiet += 1;
                    if *seconds < 1.0 {
                        s.skipped_quiet_short += 1;
                    }
                    quiet_seconds.push(*seconds as f64);
                    quiet_levels.push(*level_db as f64);
                }
                _ => {
                    s.skipped_no_speech += 1;
                    no_speech_seconds.push(*seconds as f64);
                    no_speech_levels.push(*level_db as f64);
                }
            },
            Event::Problem { what, .. } => problems.push(what.clone()),
            Event::Learned { heard, meant, .. } => {
                let pair = (heard.clone(), meant.clone());
                if stats::shareable(heard) && stats::shareable(meant) && !learned.contains(&pair) {
                    learned.push(pair);
                }
            }
        }
    }
    took.sort_by(f64::total_cmp);
    levels.sort_by(f64::total_cmp);
    if s.audio_seconds > 0.0 {
        s.took_per_audio_second = took_sum / s.audio_seconds;
    }
    s.took_median = percentile(&took, 0.5);
    s.took_p95 = percentile(&took, 0.95);
    s.level_db_median = percentile(&levels, 0.5);
    s.level_db_p10 = percentile(&levels, 0.1);
    s.skipped_quiet_seconds_median = median(quiet_seconds);
    s.skipped_quiet_level_db_median = median(quiet_levels);
    s.skipped_no_speech_seconds_median = median(no_speech_seconds);
    s.skipped_no_speech_level_db_median = median(no_speech_levels);
    s.problems = counted(problems.into_iter(), 10);
    learned.truncate(20);
    s.learned = learned;
    let mut per_day = counted(days.into_iter(), usize::MAX);
    per_day.sort();
    s.per_day = per_day;
    s.models = counted(models.into_iter(), 5);
    s
}

/// The comment: readable for people, with the numbers as JSON underneath for the agents.
pub fn markdown(s: &Summary, meta: &Meta) -> String {
    let mut out = format!("## Dictate {} · {} bis {}\n\n", meta.version, s.from, s.to);
    out += &format!(
        "Install `{}` · macOS {} ({}) · Modell `{}` · Sprache `{}` · Wörterbuch {} Wörter, im Decoder {} · Lernen {}\n\n",
        meta.install,
        meta.macos,
        meta.arch,
        meta.model,
        meta.language,
        meta.dictionary_words,
        if meta.boost { "an" } else { "aus" },
        if meta.learning { "an" } else { "aus" },
    );
    if s.dictations == 0 {
        // Times and levels of nothing would read as 0 s and 0 dBFS (full scale): say so instead.
        out += "- **0** Diktate in diesem Zeitraum\n";
    } else {
        out += &format!(
            "- **{}** Diktate, {} Wörter, {:.0} s Aufnahme ({:.0} s Sprache nach dem Kürzen der Stille)\n",
            s.dictations, s.words, s.audio_seconds, s.speech_seconds
        );
        out += &format!(
            "- Transkription: Median {:.2} s, 95 % unter {:.2} s, {:.3} s je Sekunde Audio\n",
            s.took_median, s.took_p95, s.took_per_audio_second
        );
        out += &format!(
            "- Pegel: Median {:.0} dBFS, leisestes Zehntel unter {:.0} dBFS\n",
            s.level_db_median, s.level_db_p10
        );
    }
    if s.skipped_quiet > 0 || s.skipped_no_speech > 0 {
        let mut quiet = format!("{} zu leise", s.skipped_quiet);
        if s.skipped_quiet > 0 {
            quiet += &format!(
                " (Median {:.1} s, {:.0} dBFS; {} kürzer als 1 s)",
                s.skipped_quiet_seconds_median,
                s.skipped_quiet_level_db_median,
                s.skipped_quiet_short
            );
        }
        let mut no_speech = format!("{} ohne Sprache", s.skipped_no_speech);
        if s.skipped_no_speech > 0 {
            no_speech += &format!(
                " (Median {:.1} s, {:.0} dBFS)",
                s.skipped_no_speech_seconds_median, s.skipped_no_speech_level_db_median
            );
        }
        out += &format!("- Verworfen: {quiet}, {no_speech}\n");
    }
    if !s.per_day.is_empty() {
        let days: Vec<String> = s.per_day.iter().map(|(d, n)| format!("{d}: {n}")).collect();
        out += &format!("- Pro Tag: {}\n", days.join(", "));
    }
    if !s.problems.is_empty() {
        out += "\n**Probleme**\n\n";
        for (what, n) in &s.problems {
            out += &format!("- {n}× {what}\n");
        }
    }
    if !s.learned.is_empty() {
        out += "\n**Gelernte Korrekturen** (gehört → gemeint)\n\n";
        for (heard, meant) in &s.learned {
            out += &format!("- „{heard}“ → „{meant}“\n");
        }
    }
    let json = serde_json::json!({"meta": meta, "summary": s});
    out += &format!(
        "\n<details><summary>JSON</summary>\n\n```json\n{}\n```\n</details>\n",
        serde_json::to_string_pretty(&json).unwrap_or_default()
    );
    out
}

/// What survives between reports: a random install id (no name, no hardware id) and when the
/// last report went out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub install: String,
    pub last_posted: i64,
}

fn state_path() -> PathBuf {
    crate::asr::data_dir().join("report.json")
}

pub fn load_state() -> State {
    let mut state: State = std::fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    if state.install.is_empty() {
        state.install = random_id();
        let _ = save_state(&state);
    }
    state
}

pub fn save_state(state: &State) -> Result<()> {
    let path = state_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string(state)?)?;
    Ok(())
}

fn random_id() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut bytes);
    }
    let fallback = stats::now() as u32 ^ std::process::id();
    let n = u32::from_le_bytes(bytes) ^ if bytes == [0; 4] { fallback } else { 0 };
    format!("{n:08x}")
}

/// The report for everything since the last one (at most a week), or `None` when nothing happened.
pub fn due(state: &State, now: i64) -> Option<(i64, Vec<Event>)> {
    let from = state.last_posted.max(now - MAX_DAYS * 86_400);
    if state.last_posted != 0 && now - state.last_posted < EVERY {
        return None;
    }
    let events = stats::load(&stats::path(), from);
    let worth = events
        .iter()
        .any(|e| matches!(e, Event::Dictation { .. } | Event::Problem { .. }));
    worth.then_some((from, events))
}

/// The GitHub CLI, also where an app started from Finder has a bare PATH.
pub fn gh() -> Option<PathBuf> {
    ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

/// Sends the report where the profile says ([`crate::profile::ReportTarget`]). Returns where it
/// went.
pub fn post(summary: &Summary, meta: &Meta) -> Result<String> {
    use crate::profile::ReportTarget;
    let profile = crate::profile::current();
    let Some(target) = profile.report_target() else {
        bail!("the profile names no target for reports ([report] in profile.toml)");
    };
    match target {
        ReportTarget::Issue { repo, issue } => comment(repo, issue, &markdown(summary, meta))?,
        ReportTarget::Url(url) => send(url, &payload(summary, meta))?,
    }
    Ok(target.to_string())
}

/// What a URL target receives: the numbers and the same text an issue gets.
pub fn payload(summary: &Summary, meta: &Meta) -> String {
    serde_json::json!({"meta": meta, "summary": summary, "markdown": markdown(summary, meta)})
        .to_string()
}

/// Runs `program` with `body` on stdin; its error output (scrubbed) on failure.
fn pipe(program: &std::path::Path, args: &[&str], body: &str) -> Result<Vec<u8>> {
    let name = program.file_stem().unwrap_or_default().to_string_lossy();
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {name}"))?;
    child
        .stdin
        .take()
        .with_context(|| format!("{name} stdin"))?
        .write_all(body.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!(
            "{name}: {}",
            stats::scrub(&String::from_utf8_lossy(&out.stderr))
        );
    }
    Ok(out.stdout)
}

/// Comments on a GitHub issue with `gh`, but only in a private repo: learned word pairs and
/// problem messages are the team's, and a repo that turned public would show them to everyone.
fn comment(repo: &str, issue: u32, body: &str) -> Result<()> {
    let Some(gh) = gh() else {
        bail!("the GitHub CLI (gh) is not installed");
    };
    let private = pipe(
        &gh,
        &["api", &format!("repos/{repo}"), "--jq", ".private"],
        "",
    )?;
    if String::from_utf8_lossy(&private).trim() != "true" {
        bail!("{repo} is not a private repository; reports go to private ones only");
    }
    let issue = issue.to_string();
    pipe(
        &gh,
        &["issue", "comment", &issue, "-R", repo, "--body-file", "-"],
        body,
    )?;
    Ok(())
}

/// POSTs JSON to the profile's URL with curl (also on Windows 10 and later).
fn send(url: &str, json: &str) -> Result<()> {
    let args = [
        "-fsS",
        "--max-time",
        "30",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
        "--data-binary",
        "@-",
        url,
    ];
    pipe(std::path::Path::new("curl"), &args, json).map_err(|e| {
        // curl's message may repeat the URL, which can hold a token.
        anyhow::anyhow!("{}", e.to_string().replace(url, "<report url>"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictation(at: i64, took: f32, level: f32) -> Event {
        Event::Dictation {
            at,
            seconds: 10.0,
            level_db: level,
            speech_seconds: 8.0,
            took,
            words: 20,
            model: "parakeet-ultra-coreml".into(),
        }
    }

    #[test]
    fn numbers_add_up() {
        let day = 1_793_491_200;
        let events = vec![
            dictation(day, 0.1, -40.0),
            dictation(day + 10, 0.3, -50.0),
            dictation(day + 86_400, 0.2, -30.0),
            Event::Skipped {
                at: day,
                seconds: 1.0,
                level_db: -70.0,
                reason: "quiet".into(),
            },
            Event::Skipped {
                at: day,
                seconds: 2.0,
                level_db: -40.0,
                reason: "no_speech".into(),
            },
            Event::Problem {
                at: day,
                what: "buffer underrun".into(),
            },
            Event::Problem {
                at: day,
                what: "buffer underrun".into(),
            },
            Event::Learned {
                at: day,
                heard: "deckläuen".into(),
                meant: "deployen".into(),
            },
            Event::Learned {
                at: day,
                heard: "Anna".into(),
                meant: "anna@x.de".into(),
            },
        ];
        let s = summarize(&events, day, day + 86_400);
        assert_eq!(s.dictations, 3);
        assert_eq!(s.words, 60);
        assert_eq!(s.audio_seconds, 30.0);
        assert!((s.took_per_audio_second - 0.02).abs() < 1e-9);
        assert_eq!(s.took_median, 0.2f32 as f64);
        assert_eq!((s.skipped_quiet, s.skipped_no_speech), (1, 1));
        assert_eq!(s.problems, vec![("buffer underrun".to_string(), 2)]);
        assert_eq!(
            s.learned,
            vec![("deckläuen".to_string(), "deployen".to_string())]
        );
        assert_eq!(
            s.per_day,
            vec![("2026-11-01".to_string(), 2), ("2026-11-02".to_string(), 1)]
        );
    }

    #[test]
    fn empty_period_is_all_zeros() {
        let s = summarize(&[], 0, 86_400);
        assert_eq!(s.dictations, 0);
        assert_eq!(s.took_p95, 0.0);
    }

    #[test]
    fn a_url_gets_the_numbers_and_the_text() {
        let summary = summarize(&[dictation(1_000, 0.2, -30.0)], 0, 2_000);
        let meta = Meta {
            install: "abcd1234".into(),
            ..Meta::default()
        };
        let json: serde_json::Value = serde_json::from_str(&payload(&summary, &meta)).unwrap();
        assert_eq!(json["meta"]["install"], "abcd1234");
        assert_eq!(json["summary"]["dictations"], 1);
        assert!(json["markdown"].as_str().unwrap().contains("abcd1234"));
    }

    #[test]
    fn markdown_has_the_numbers_and_no_text() {
        let events = vec![dictation(1_793_491_200, 0.1, -40.0)];
        let s = summarize(&events, 1_793_491_200, 1_793_577_600);
        let meta = Meta {
            install: "ab12cd34".into(),
            version: "0.1.0-abc".into(),
            model: "parakeet-ultra-coreml".into(),
            ..Meta::default()
        };
        let text = markdown(&s, &meta);
        assert!(text.contains("**1** Diktate"), "{text}");
        assert!(text.contains("ab12cd34"), "{text}");
        assert!(text.contains("```json"), "{text}");
    }

    fn skipped(seconds: f32, level: f32, reason: &str) -> Event {
        Event::Skipped {
            at: 1_793_491_200,
            seconds,
            level_db: level,
            reason: reason.into(),
        }
    }

    #[test]
    fn a_report_without_dictations_claims_no_times_or_levels() {
        let events = vec![Event::Problem {
            at: 1_793_491_200,
            what: "microphone gone".into(),
        }];
        let s = summarize(&events, 1_793_491_200, 1_793_577_600);
        let text = markdown(&s, &Meta::default());
        assert!(text.contains("**0** Diktate in diesem Zeitraum"), "{text}");
        let prose = text.split("<details>").next().unwrap();
        for absent in ["Transkription", "Pegel", "dBFS", "Verworfen"] {
            assert!(!prose.contains(absent), "{absent} in {prose}");
        }
        assert!(prose.contains("1× microphone gone"), "{prose}");
        // The JSON keeps every field for the agents.
        assert!(text.contains("\"level_db_median\""), "{text}");

        let events = vec![skipped(0.3, -70.0, "quiet")];
        let text = markdown(&summarize(&events, 0, 1), &Meta::default());
        assert!(text.contains("- Verworfen: 1 zu leise ("), "{text}");
    }

    #[test]
    fn quiet_skips_say_how_long_and_how_loud() {
        let events = vec![
            dictation(1_793_491_200, 0.1, -40.0),
            skipped(0.2, -65.0, "quiet"),
            skipped(0.4, -63.0, "quiet"),
            skipped(0.6, -70.0, "quiet"),
            skipped(3.0, -61.0, "quiet"),
            skipped(2.0, -45.0, "no_speech"),
        ];
        let s = summarize(&events, 1_793_491_200, 1_793_577_600);
        assert_eq!((s.skipped_quiet, s.skipped_no_speech), (4, 1));
        assert_eq!(s.skipped_quiet_short, 3);
        assert_eq!(s.skipped_quiet_seconds_median, 0.6f32 as f64);
        assert_eq!(s.skipped_quiet_level_db_median, -63.0);
        assert_eq!(s.skipped_no_speech_seconds_median, 2.0);
        assert_eq!(s.skipped_no_speech_level_db_median, -45.0);
        let text = markdown(&s, &Meta::default());
        assert!(
            text.contains(
                "- Verworfen: 4 zu leise (Median 0.6 s, -63 dBFS; 3 kürzer als 1 s), \
                 1 ohne Sprache (Median 2.0 s, -45 dBFS)\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn old_reports_still_parse() {
        let mut old = serde_json::to_value(summarize(&[], 0, 1)).unwrap();
        let fields = old.as_object_mut().unwrap();
        fields.retain(|k, _| !k.starts_with("skipped_") || !k.contains("_median"));
        fields.remove("skipped_quiet_short");
        let s: Summary = serde_json::from_value(old).unwrap();
        assert_eq!(s.skipped_quiet_short, 0);
    }

    #[test]
    fn nothing_is_due_within_a_day_or_without_events() {
        let state = State {
            install: "x".into(),
            last_posted: stats::now() - 3600,
        };
        assert!(due(&state, stats::now()).is_none());
    }
}
