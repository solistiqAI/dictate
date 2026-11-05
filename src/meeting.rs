//! Souffleur: Dictate listens along in a meeting and an agent looks things up while people talk.
//!
//! Two sources: the microphone ("Ich", its own choice, `meeting_microphone`) and what the Mac plays
//! ("Andere", the people in a call: `dictate-ane system-audio`, a Core Audio tap). Each source is
//! cut into utterances at pauses ([`Segmenter`]); every utterance is transcribed by our own speech
//! model instance (the dictation one stays free), shown live while it is spoken and final after
//! the pause. Final lines go, batched, to one long-running Claude Code session
//! (`claude -p --input-format stream-json`, prompt `meeting-prompt.txt`), which answers with short
//! cards ("## title" + a few lines) or "-" when there is nothing worth showing. The transcript and
//! the cards are saved as Markdown in `meetings/` (never the audio).

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};

use crate::asr::{self, Asr};
use crate::audio::{Clip, MODEL_RATE, Recorder, resample};
use crate::config::Config;
use crate::protocol::{MeetingCard, MeetingLine, MeetingStatus};
use crate::replace::Corrector;

const PROMPT: &str = include_str!("meeting-prompt.txt");
/// After the meeting: "Das Wichtigste" once more, over the whole transcript.
const FINAL_PROMPT: &str = include_str!("meeting-final-prompt.txt");
/// How long the final pass may take before the meeting's list stays as it was.
const FINAL_TIMEOUT: Duration = Duration::from_secs(120);
/// The first message, before anyone speaks: load the tools a lookup needs most (the profile's
/// `warmup_tools`; their schemas are deferred, fetching them costs a round trip in the first
/// lookup otherwise), then wait.
fn warmup(tools: &[String]) -> String {
    if tools.is_empty() {
        return "Das Meeting beginnt gleich. Such nichts und antworte nur mit -".into();
    }
    format!(
        "Das Meeting beginnt gleich. Lade jetzt mit ToolSearch die Werkzeuge, die du am \
         häufigsten brauchst (select:{}), such nichts, und antworte dann nur mit -",
        tools.join(",")
    )
}
/// How often the sources are read, levels sent, live text refreshed.
const TICK: Duration = Duration::from_millis(100);
const STATUS_EVERY: Duration = Duration::from_millis(250);
const LIVE_EVERY: Duration = Duration::from_millis(900);
/// Lines go to the agent once it is free and someone paused this long (or a question was asked,
/// or a lot was said).
const FEED_PAUSE: Duration = Duration::from_millis(1200);
const FEED_WORDS: usize = 40;

// Read access to the profile's `souffleur.allowed_tools` goes without asking: the meeting is
// live, a permission check per lookup costs seconds. Everything else follows the user's own
// Claude Code settings.
//
// Read-only is only asked for in the prompt for now (thomas, 2026-11-03: the feature is still being
// discussed). Harder would be `--permission-mode dontAsk` with an allowlist of the read tools only
// (search_*, get_*, list_*, read_*, `Bash(gh … view/list/search)`), so a write fails whatever
// the model does — worth it before others use it, and because people in the meeting speak into
// the agent's input.
#[derive(Debug)]
pub enum Update {
    Line(MeetingLine),
    Card(MeetingCard),
    Status(MeetingStatus),
    /// "Das Wichtigste": the agent's new list of the meeting's key facts.
    Highlights(Vec<String>),
    /// The meeting stopped (the reason when it was not asked to).
    Ended(Option<String>),
}

pub type Sink = Arc<dyn Fn(Update) + Send + Sync>;

pub struct Setup {
    pub config: Config,
    pub replacements: PathBuf,
    /// Where the transcripts go (`meetings/` in the data folder).
    pub folder: PathBuf,
    /// `dictate souffleur <wav>`: this recording plays as "Andere" in real time instead of the
    /// microphone and the Mac's sound; the meeting ends after it.
    pub replay: Option<PathBuf>,
}

/// A running meeting; dropping it stops it.
pub struct Handle {
    stop: Arc<AtomicBool>,
    asks: Sender<String>,
}

impl Handle {
    /// A question typed in the panel.
    pub fn ask(&self, text: String) {
        let _ = self.asks.send(text);
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn folder() -> PathBuf {
    asr::data_dir().join("meetings")
}

// ---------------------------------------------------------------------------------------------
// Saved meetings (`dictate souffleur list` / `show`)

/// A transcript in `meetings/`, as `dictate souffleur list` shows it.
#[derive(Debug, PartialEq)]
pub struct Saved {
    pub path: PathBuf,
    /// "2026-11-03 13-28" (the file name: when it started).
    pub name: String,
    pub lines: usize,
    pub answers: usize,
    /// Clock times of the first and last line ("13:28"), if any.
    pub span: Option<(String, String)>,
    /// The first answer's title: what it was about, roughly.
    pub topic: String,
}

/// The saved meetings, newest first.
pub fn saved(folder: &Path) -> Vec<Saved> {
    let mut out: Vec<Saved> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some(summarize(path, name, &text))
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out
}

fn summarize(path: PathBuf, name: String, text: &str) -> Saved {
    let mut times = Vec::new();
    let mut answers = 0;
    let mut topic = String::new();
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix("**") {
            // "**13:28 Andere:** …"
            if let Some(time) = rest.get(..5).filter(|t| t.as_bytes().get(2) == Some(&b':')) {
                times.push(time.to_string());
            }
        } else if let Some(rest) = l.strip_prefix("> **Souffleur ") {
            answers += 1;
            if topic.is_empty() {
                topic = rest
                    .split_once(":**")
                    .map_or("", |(_, t)| t)
                    .trim()
                    .to_string();
            }
        }
    }
    Saved {
        path,
        name,
        lines: times.len(),
        answers,
        span: times
            .first()
            .map(|f| (f.clone(), times.last().unwrap().clone())),
        topic,
    }
}

/// Only what the Souffleur said: its answers (with the questions typed to it) and "Das Wichtigste".
pub fn answers_only(text: &str) -> String {
    let mut out = String::new();
    let mut in_highlights = false;
    for l in text.lines() {
        if l.starts_with("# ") {
            out.push_str(l);
            out.push_str("\n\n");
        } else if let Some(heading) = l.strip_prefix("## ") {
            in_highlights = is_highlights(heading);
            if in_highlights {
                out.push_str(l);
                out.push('\n');
            }
        } else if l.starts_with('>') || (in_highlights && !l.trim().is_empty()) {
            out.push_str(l);
            out.push('\n');
        } else if l.is_empty() && !out.ends_with("\n\n") && !out.is_empty() {
            out.push('\n');
        }
    }
    out.trim_end().to_string() + "\n"
}

pub fn start(setup: Setup, sink: Sink) -> Handle {
    let stop = Arc::new(AtomicBool::new(false));
    let (asks, asked) = mpsc::channel();
    {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let reason = match run(setup, sink.clone(), asked, &stop) {
                Ok(()) => None,
                Err(e) => {
                    eprintln!("souffleur: {e:#}");
                    Some(format!("{e:#}"))
                }
            };
            sink(Update::Ended(reason));
        });
    }
    Handle { stop, asks }
}

/// Unix seconds with fraction.
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Cutting a stream into utterances

/// 20 ms frames; speech = louder than the source's own noise floor by a margin.
const FRAME_MS: u32 = 20;
const PRE_ROLL_FRAMES: usize = 15; // 300 ms kept before the speech starts
const START_FRAMES: usize = 3; // 60 ms of speech opens an utterance
const END_FRAMES: usize = 35; // 700 ms of quiet closes it
const TAIL_FRAMES: usize = 12; // 240 ms kept after the last speech
const MIN_SPEECH_FRAMES: usize = 15; // shorter is a click or a cough
const SOFT_MAX_SECONDS: f32 = 20.0; // past this the next short pause closes it
const HARD_MAX_SECONDS: f32 = 30.0;

pub struct Segment {
    /// 16 kHz mono.
    pub samples: Vec<f32>,
    /// Unix seconds when it began and ended.
    pub start: f64,
    pub end: f64,
    pub speech_seconds: f32,
}

struct Open {
    samples: Vec<f32>,
    start_sample: u64,
    speech_frames: usize,
    quiet_frames: usize,
    /// Length of `samples` at the last speech frame.
    last_speech_len: usize,
}

pub struct Segmenter {
    rate: u32,
    frame: usize,
    /// Unix time of sample 0.
    base: f64,
    consumed: u64,
    carry: Vec<f32>,
    floor_db: f32,
    pre: VecDeque<Vec<f32>>,
    run: usize,
    open: Option<Open>,
    levels: VecDeque<f32>,
}

impl Segmenter {
    pub fn new(rate: u32, base: f64) -> Self {
        Segmenter {
            rate,
            frame: (rate * FRAME_MS / 1000) as usize,
            base,
            consumed: 0,
            carry: Vec::new(),
            floor_db: -60.0,
            pre: VecDeque::new(),
            run: 0,
            open: None,
            levels: VecDeque::new(),
        }
    }

    /// Level of the last 300 ms (dBFS), for the meter.
    pub fn level_db(&self) -> f32 {
        let n = self.levels.len().max(1) as f32;
        let mean = self
            .levels
            .iter()
            .map(|db| 10f32.powf(db / 10.0))
            .sum::<f32>()
            / n;
        if mean > 0.0 {
            10.0 * mean.log10()
        } else {
            -120.0
        }
    }

    fn time_of(&self, sample: u64) -> f64 {
        self.base + sample as f64 / self.rate as f64
    }

    /// Feeds samples; returns the utterances that ended in them.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Segment> {
        let mut done = Vec::new();
        self.carry.extend_from_slice(samples);
        let frames = self.carry.len() / self.frame;
        let all = std::mem::take(&mut self.carry);
        for chunk in all.chunks(self.frame).take(frames) {
            if let Some(segment) = self.step(chunk) {
                done.push(segment);
            }
        }
        self.carry = all[frames * self.frame..].to_vec();
        done
    }

    fn step(&mut self, frame: &[f32]) -> Option<Segment> {
        let mean = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
        let db = if mean > 0.0 {
            10.0 * mean.log10()
        } else {
            -120.0
        };
        self.levels.push_back(db);
        if self.levels.len() > 15 {
            self.levels.pop_front();
        }
        // The floor follows quiet at once and loudness slowly (2.5 dB/s, 0.1 dB/s during speech),
        // so a long stretch of speech never becomes the floor.
        let threshold = (self.floor_db + 12.0).max(-55.0);
        let speech = db > threshold;
        self.floor_db = if db < self.floor_db {
            db.max(-90.0)
        } else {
            (self.floor_db + if speech { 0.002 } else { 0.05 }).min(-20.0)
        };
        let start_sample = self.consumed;
        self.consumed += frame.len() as u64;
        self.run = if speech { self.run + 1 } else { 0 };

        let Some(open) = &mut self.open else {
            self.pre.push_back(frame.to_vec());
            if self.pre.len() > PRE_ROLL_FRAMES + START_FRAMES {
                self.pre.pop_front();
            }
            if self.run >= START_FRAMES {
                let pre: Vec<f32> = self.pre.drain(..).flatten().collect();
                let start = start_sample + frame.len() as u64 - pre.len() as u64;
                self.open = Some(Open {
                    last_speech_len: pre.len(),
                    samples: pre,
                    start_sample: start,
                    speech_frames: self.run,
                    quiet_frames: 0,
                });
            }
            return None;
        };
        open.samples.extend_from_slice(frame);
        if speech {
            open.speech_frames += 1;
            open.quiet_frames = 0;
            open.last_speech_len = open.samples.len();
        } else {
            open.quiet_frames += 1;
        }
        let seconds = open.samples.len() as f32 / self.rate as f32;
        let close = open.quiet_frames >= END_FRAMES
            || (seconds >= SOFT_MAX_SECONDS && open.quiet_frames >= 5)
            || seconds >= HARD_MAX_SECONDS;
        if !close {
            return None;
        }
        let mut open = self.open.take()?;
        let keep = (open.last_speech_len + TAIL_FRAMES * self.frame).min(open.samples.len());
        open.samples.truncate(keep);
        if open.speech_frames < MIN_SPEECH_FRAMES {
            return None;
        }
        let start = self.time_of(open.start_sample);
        Some(Segment {
            end: start + open.samples.len() as f64 / self.rate as f64,
            start,
            speech_seconds: (open.speech_frames as u32 * FRAME_MS) as f32 / 1000.0,
            samples: open.samples,
        })
    }

    /// The utterance being spoken, when it has at least `min_speech` of speech: its start and
    /// samples so far.
    pub fn open(&self, min_speech: f32) -> Option<(f64, &[f32])> {
        let open = self.open.as_ref()?;
        let speech = (open.speech_frames as u32 * FRAME_MS) as f32 / 1000.0;
        (speech >= min_speech).then(|| (self.time_of(open.start_sample), &open.samples[..]))
    }

    /// Whatever is open, closed now (the meeting ends).
    pub fn flush(&mut self) -> Option<Segment> {
        let open = self.open.take()?;
        if open.speech_frames < MIN_SPEECH_FRAMES {
            return None;
        }
        let start = self.time_of(open.start_sample);
        Some(Segment {
            end: start + open.samples.len() as f64 / self.rate as f64,
            start,
            speech_seconds: (open.speech_frames as u32 * FRAME_MS) as f32 / 1000.0,
            samples: open.samples,
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }
}

// ---------------------------------------------------------------------------------------------
// Text helpers

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Without headphones the microphone hears the speakers too: a microphone line whose words were
/// mostly just said by the others is that echo.
pub fn is_echo(mic: &str, others: &[&str]) -> bool {
    let mine = words(mic);
    if mine.is_empty() {
        return false;
    }
    let theirs: std::collections::HashSet<String> = others.iter().flat_map(|t| words(t)).collect();
    if theirs.is_empty() {
        return false;
    }
    let found = mine.iter().filter(|w| theirs.contains(*w)).count();
    found as f32 >= 0.6 * mine.len() as f32 && (mine.len() >= 2 || found == mine.len())
}

/// What Parakeet makes of a short noise.
fn is_noise(text: &str, speech_seconds: f32) -> bool {
    let w = words(text);
    if w.is_empty() {
        return true;
    }
    let joined = w.join(" ");
    speech_seconds < 1.2
        && matches!(
            joined.as_str(),
            "thank you" | "thanks" | "danke" | "vielen dank" | "you" | "bye" | "mm" | "hmm"
        )
}

/// The agent's answer as cards: "## title" starts one; "-" or nothing = no card.
pub fn cards(answer: &str) -> Vec<(String, String)> {
    let answer = answer.trim();
    if answer.is_empty()
        || answer
            .trim_matches(|c: char| c == '-' || c.is_whitespace())
            .is_empty()
    {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = Vec::new();
    let mut title: Option<String> = None;
    let mut body = String::new();
    let mut flush = |title: &mut Option<String>, body: &mut String| {
        let text = body.trim().to_string();
        if title.is_some() || !text.is_empty() {
            out.push((title.take().unwrap_or_default(), text));
        }
        body.clear();
    };
    for line in answer.lines() {
        let trimmed = line.trim_start();
        if let Some(heading) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("### "))
            .or_else(|| trimmed.strip_prefix("# "))
        {
            flush(&mut title, &mut body);
            title = Some(heading.trim().trim_matches('*').trim().to_string());
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(&mut title, &mut body);
    out
}

/// The section of an answer that is not a card but the list pinned above them: the agent writes
/// it under the heading of the UI language (`highlights_heading`); both are read, so meetings
/// saved in the other language (and every one saved before English) still work.
pub const HIGHLIGHTS: &str = "Das Wichtigste";
pub const HIGHLIGHTS_EN: &str = "Key facts";

/// The heading the prompt asks for and saved meetings get, in the UI language.
pub fn highlights_heading() -> &'static str {
    crate::text::pick(HIGHLIGHTS, HIGHLIGHTS_EN)
}

fn is_highlights(title: &str) -> bool {
    let title = title.trim_end_matches(':').trim();
    title == HIGHLIGHTS || title.eq_ignore_ascii_case(HIGHLIGHTS_EN)
}

/// Takes "## Das Wichtigste" out of an answer's cards: the cards, and its bullets when it has it.
pub fn highlights(found: Vec<(String, String)>) -> (Vec<(String, String)>, Option<Vec<String>>) {
    let mut list = None;
    let cards = found
        .into_iter()
        .filter(|(title, body)| {
            if !is_highlights(title) {
                return true;
            }
            list = Some(
                body.lines()
                    .map(|l| {
                        let l = l.trim();
                        ["- ", "* ", "• "]
                            .iter()
                            .find_map(|p| l.strip_prefix(p))
                            .unwrap_or(l)
                            .trim()
                            .to_string()
                    })
                    .filter(|l| !l.is_empty())
                    .collect(),
            );
            false
        })
        .collect();
    (cards, list)
}

/// What the panel says the agent is doing, from the tool it calls.
pub fn activity(tool: &str, input: &Value) -> String {
    use crate::text::pick;
    let server = tool
        .strip_prefix("mcp__")
        .map(|rest| rest.split("__").next().unwrap_or(rest));
    match server {
        Some("crm") => pick("Sucht im CRM", "Searching the CRM").into(),
        Some("notion") => pick("Sucht in Notion", "Searching Notion").into(),
        Some(s) if s.contains("Slack") => pick("Sucht in Slack", "Searching Slack").into(),
        Some(s) if s.contains("Gmail") => pick("Sucht in Gmail", "Searching Gmail").into(),
        Some(s) if s.contains("Calendar") => {
            pick("Schaut in den Kalender", "Checking the calendar").into()
        }
        Some(s) if s.contains("Google") => pick("Sucht in Drive", "Searching Drive").into(),
        Some(s) => format!(
            "{} {}",
            pick("Fragt", "Asking"),
            s.replace("claude_ai_", "").replace('_', " ")
        ),
        None => match tool {
            "Bash" => {
                let command = input["command"].as_str().unwrap_or_default().trim();
                if command.starts_with("gh ") || command.starts_with("git ") {
                    pick("Schaut auf GitHub", "Checking GitHub").into()
                } else {
                    let first = command.split_whitespace().next().unwrap_or("…");
                    pick(format!("Führt {first} aus"), format!("Running {first}"))
                }
            }
            "WebSearch" | "WebFetch" => pick("Sucht im Web", "Searching the web").into(),
            "Read" | "Grep" | "Glob" => pick("Liest Dateien", "Reading files").into(),
            "ToolSearch" => pick("Sucht das passende Werkzeug", "Finding the right tool").into(),
            "Agent" | "Task" => pick("Fragt einen Helfer", "Asking a helper").into(),
            other => pick(format!("Benutzt {other}"), format!("Using {other}")),
        },
    }
}

/// "11:31" in local time.
fn clock(unix: f64) -> String {
    let (_, _, _, h, m) = local(unix);
    format!("{h:02}:{m:02}")
}

fn local(unix: f64) -> (i32, u32, u32, u32, u32) {
    crate::platform::local_time(unix)
}

fn who_label(who: &str) -> &'static str {
    if who == "me" { "Ich" } else { "Andere" }
}

// ---------------------------------------------------------------------------------------------
// The transcript file

struct Transcript {
    file: Option<File>,
    path: PathBuf,
}

impl Transcript {
    fn create(folder: &Path, started: f64) -> Self {
        let (y, mo, d, h, mi) = local(started);
        let path = folder.join(format!("{y}-{mo:02}-{d:02} {h:02}-{mi:02}.md"));
        let file = std::fs::create_dir_all(folder)
            .and_then(|_| File::create(&path))
            .map_err(|e| eprintln!("souffleur: transcript {}: {e}", path.display()))
            .ok();
        let mut t = Transcript {
            file,
            path: path.clone(),
        };
        t.write(&format!(
            "# Meeting {d:02}.{mo:02}.{y} {h:02}:{mi:02}\n\nMitschrift des Souffleurs (Dictate). Ich = Mikrofon, Andere = Ton des Macs.\n\n"
        ));
        if t.file.is_some() {
            eprintln!("souffleur: transcript {}", path.display());
        }
        t
    }

    fn write(&mut self, text: &str) {
        if let Some(f) = &mut self.file {
            let _ = f.write_all(text.as_bytes());
        }
    }

    fn line(&mut self, line: &MeetingLine) {
        self.write(&format!(
            "**{} {}:** {}\n\n",
            clock(line.at),
            who_label(&line.who),
            line.text
        ));
    }

    fn card(&mut self, card: &MeetingCard) {
        let mut text = String::new();
        if let Some(q) = &card.question {
            text.push_str(&format!("> **Frage {}:** {q}\n>\n", clock(card.at)));
        }
        let title = if card.title.is_empty() {
            String::new()
        } else {
            format!(" {}", card.title)
        };
        text.push_str(&format!("> **Souffleur {}:**{title}\n", clock(card.at)));
        for l in card.body.lines() {
            text.push_str(&format!("> {l}\n"));
        }
        text.push('\n');
        self.write(&text);
    }

    /// At the end of the file: the last "Das Wichtigste".
    fn highlights(&mut self, list: &[String]) {
        if list.is_empty() {
            return;
        }
        let mut text = format!("## {}\n\n", highlights_heading());
        for l in list {
            text.push_str(&format!("- {l}\n"));
        }
        text.push('\n');
        self.write(&text);
    }
}

/// The transcript's lines ("**12:03 Andere:** …") with self-corrections and false starts taken
/// out by Gemma ("Korrigieren", deletions only and checked as when dictating), all lines in one
/// batch after the meeting. The raw transcript stays in `.roh/` beside it. Without Gemma (not
/// downloaded) the transcript stays as it is: Apple's model is too slow for hundreds of lines.
fn tidy_transcript(path: &Path, model: &str) {
    if !crate::correct::live_ready() {
        crate::correct::prepare(model);
    }
    if !crate::correct::live_ready() {
        eprintln!(
            "souffleur: transcript not tidied: no Gemma (`dictate model download gemma-4-e4b`)"
        );
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let started = Instant::now();
    let (out, tidied) = tidy_lines(&text, |lines| {
        crate::correct::correct(&lines.join("\n"))
            .0
            .split('\n')
            .map(String::from)
            .collect()
    });
    if tidied == 0 {
        eprintln!("souffleur: transcript tidy: nothing to take out");
        return;
    }
    let raw = path.parent().map(|d| d.join(".roh")).unwrap_or_default();
    let saved = std::fs::create_dir_all(&raw)
        .and_then(|_| std::fs::copy(path, raw.join(path.file_name().unwrap_or_default())))
        .and_then(|_| std::fs::write(path.with_extension("md.tmp"), &out))
        .and_then(|_| std::fs::rename(path.with_extension("md.tmp"), path));
    match saved {
        Ok(()) => eprintln!(
            "souffleur: transcript tidied in {:.1} s, {tidied} lines changed (raw in .roh/)",
            started.elapsed().as_secs_f32()
        ),
        Err(e) => eprintln!("souffleur: transcript tidy: {e}"),
    }
}

/// `text` with the speech of its "**12:03 Who:** …" lines replaced by `clean(speech)` (one call,
/// same order), and how many changed. Cards, questions and "Das Wichtigste" stay as they are.
fn tidy_lines(text: &str, clean: impl FnOnce(&[String]) -> Vec<String>) -> (String, usize) {
    let spoken = |l: &str| {
        l.starts_with("**")
            .then(|| l.find(":** "))
            .flatten()
            .map(|at| at + ":** ".len())
    };
    let speech: Vec<String> = text
        .lines()
        .filter_map(|l| spoken(l).map(|at| l[at..].to_string()))
        .collect();
    if speech.is_empty() {
        return (text.to_string(), 0);
    }
    let cleaned = clean(&speech);
    if cleaned.len() != speech.len() {
        return (text.to_string(), 0);
    }
    let mut next = cleaned.into_iter();
    let mut changed = 0;
    let mut out = String::with_capacity(text.len());
    for l in text.split_inclusive('\n') {
        let body = l.trim_end_matches('\n');
        match spoken(body) {
            Some(at) => {
                let new = next.next().unwrap_or_default();
                if new != body[at..] {
                    changed += 1;
                }
                out.push_str(&body[..at]);
                out.push_str(&new);
                out.push_str(&l[body.len()..]);
            }
            None => out.push_str(l),
        }
    }
    (out, changed)
}

// ---------------------------------------------------------------------------------------------
// The agent

enum Turn {
    Warmup,
    Listen,
    Ask {
        card: usize,
        question: String,
        at: f64,
    },
}

struct Shared {
    sink: Sink,
    status: Mutex<MeetingStatus>,
    turns: Mutex<VecDeque<Turn>>,
    cards: AtomicUsize,
    transcript: Mutex<Transcript>,
    draft: Mutex<Draft>,
    /// "Das Wichtigste" as the agent last gave it.
    highlights: Mutex<Vec<String>>,
    /// The meeting is ending: the agent's exit is no problem.
    stopping: AtomicBool,
}

impl Shared {
    fn status(&self, change: impl FnOnce(&mut MeetingStatus)) {
        let status = {
            let mut s = self.status.lock().unwrap();
            change(&mut s);
            s.clone()
        };
        (self.sink)(Update::Status(status));
    }

    fn card(&self, card: MeetingCard) {
        if !card.pending && !(card.title.is_empty() && card.body.is_empty()) {
            self.transcript.lock().unwrap().card(&card);
        }
        (self.sink)(Update::Card(card));
    }

    fn highlights(&self, list: Vec<String>) {
        *self.highlights.lock().unwrap() = list.clone();
        (self.sink)(Update::Highlights(list));
    }

    fn busy(&self) -> bool {
        !self.turns.lock().unwrap().is_empty()
    }
}

/// Claude Code: `DICTATE_CLAUDE`, its usual install places, else the PATH. Also the spoken
/// conversation's agent (`voice.rs`).
pub fn claude_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DICTATE_CLAUDE") {
        return Some(path.into());
    }
    dictate_core::locate::claude(&dictate_core::locate::Env::current())
        .into_iter()
        .find(|p| p.is_file())
}

/// The PATH for an agent: started from Finder the app has a bare one; `gh`, `git` and friends
/// live in these places.
pub fn agent_path() -> std::ffi::OsString {
    let home = dirs::home_dir().unwrap_or_default();
    let mut dirs = vec![home.join(".local/bin")];
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if !cfg!(windows) {
        dirs.extend(
            [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ]
            .map(PathBuf::from),
        );
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

fn agent_folder(config: &Config) -> PathBuf {
    crate::profile::current().agent_folder(&config.meeting_folder)
}

/// `meeting-prompt.txt` in the UI language (the answers' language, the heading of the key facts),
/// then what the profile says about the user and their sources, then the user's own
/// instructions.
pub fn prompt(config: &Config) -> String {
    let (language, suggestion) =
        crate::text::pick(("German", "Vorschlag"), ("English", "Suggestion"));
    let mut prompt = PROMPT
        .replace("{language}", language)
        .replace("{highlights}", highlights_heading())
        .replace("{suggestion}", suggestion);
    let context = crate::profile::current()
        .souffleur
        .context
        .trim()
        .to_string();
    if !context.is_empty() {
        prompt.push_str("\n\nAbout the person and their sources:\n");
        prompt.push_str(&context);
        prompt.push('\n');
    }
    let extra = config.meeting_instructions.trim();
    if !extra.is_empty() {
        prompt.push_str("\n\nMore instructions from the user:\n");
        prompt.push_str(extra);
        prompt.push('\n');
    }
    prompt
}

struct Agent {
    child: Child,
    input: ChildStdin,
}

impl Agent {
    fn start(config: &Config, shared: Arc<Shared>) -> Result<Self> {
        let binary = claude_binary().ok_or_else(|| {
            anyhow!(
                "{}",
                crate::text::pick(
                    "Claude Code nicht gefunden (~/.local/bin/claude)",
                    "Claude Code not found (~/.local/bin/claude)"
                )
            )
        })?;
        let path = agent_path();
        let mut command = Command::new(&binary);
        command
            .args([
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
            ])
            .args(["--verbose", "--include-partial-messages"])
            .args(["--model", config.meeting_model.trim()])
            .args(["--append-system-prompt", &prompt(config)])
            .arg("--allowedTools")
            .args(&crate::profile::current().souffleur.allowed_tools)
            .current_dir(agent_folder(config))
            .env("PATH", path)
            // Thinking costs seconds before every answer; a lookup in a meeting needs speed.
            .env(
                "MAX_THINKING_TOKENS",
                std::env::var("DICTATE_MEETING_THINKING").unwrap_or_else(|_| "0".into()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("start {}", binary.display()))?;
        let input = child.stdin.take().context("claude stdin")?;
        let output = child.stdout.take().context("claude stdout")?;
        let errors = child.stderr.take().context("claude stderr")?;
        let tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
        {
            let tail = tail.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(errors).lines().map_while(Result::ok) {
                    eprintln!("souffleur agent: {line}");
                    let mut tail = tail.lock().unwrap();
                    tail.push_back(line);
                    if tail.len() > 3 {
                        tail.pop_front();
                    }
                }
            });
        }
        std::thread::spawn(move || read_agent(output, shared, tail));
        eprintln!(
            "souffleur: agent {} ({}) in {}",
            binary.display(),
            config.meeting_model,
            agent_folder(config).display()
        );
        Ok(Agent { child, input })
    }

    fn send(&mut self, text: &str) -> Result<()> {
        let message = json!({"type": "user", "message": {"role": "user", "content": text}});
        writeln!(self.input, "{message}")?;
        self.input.flush()?;
        Ok(())
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_agent(output: impl Read, shared: Arc<Shared>, tail: Arc<Mutex<VecDeque<String>>>) {
    for line in BufReader::new(output).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match event["type"].as_str() {
            // Partial messages: the answer shows while it is written.
            Some("stream_event") => {
                let inner = &event["event"];
                match inner["type"].as_str() {
                    Some("message_start") => shared.draft.lock().unwrap().text.clear(),
                    Some("content_block_delta") if inner["delta"]["type"] == "text_delta" => {
                        let text = inner["delta"]["text"].as_str().unwrap_or_default();
                        draft(&shared, text);
                    }
                    _ => {}
                }
            }
            Some("assistant") => {
                let content = event["message"]["content"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for item in content {
                    if item["type"] == "tool_use" {
                        let name = item["name"].as_str().unwrap_or_default();
                        let what = activity(name, &item["input"]);
                        eprintln!("souffleur agent: {name}");
                        shared.status(|s| {
                            s.agent = "thinking".into();
                            s.activity = what;
                        });
                    }
                }
            }
            Some("result") => finish_turn(&shared, &event),
            _ => {}
        }
    }
    if shared.stopping.load(Ordering::SeqCst) {
        return;
    }
    let why = tail
        .lock()
        .unwrap()
        .back()
        .cloned()
        .unwrap_or_else(|| "ohne Meldung".into());
    shared.status(|s| {
        s.agent = "problem".into();
        s.activity = String::new();
        s.problem = format!(
            "{}: {why}",
            crate::text::pick("Agent beendet", "Agent ended")
        );
    });
}

/// The turn's answer so far: cards as they are written ("## …" first, so a "-" never shows).
struct Draft {
    text: String,
    /// The cards shown for this turn so far, in order.
    ids: Vec<usize>,
}

/// The typed question the running turn answers, if it is one.
fn asked(shared: &Shared) -> Option<(usize, String, f64)> {
    match shared.turns.lock().unwrap().front() {
        Some(Turn::Ask { card, question, at }) => Some((*card, question.clone(), *at)),
        _ => None,
    }
}

fn draft(shared: &Shared, delta: &str) {
    let warmup = matches!(
        shared.turns.lock().unwrap().front(),
        Some(Turn::Warmup) | None
    );
    let ask = asked(shared);
    let mut draft = shared.draft.lock().unwrap();
    draft.text.push_str(delta);
    if warmup || !draft.text.trim_start().starts_with('#') {
        return;
    }
    let (mut found, _) = highlights(cards(&draft.text));
    // "## Das Wi…" while it is being written is no card either.
    if found.last().is_some_and(|(title, body)| {
        body.is_empty()
            && [HIGHLIGHTS, HIGHLIGHTS_EN]
                .iter()
                .any(|h| h.starts_with(title.as_str()))
    }) {
        found.pop();
    }
    for (i, (title, body)) in found.into_iter().enumerate() {
        let id = match draft.ids.get(i) {
            Some(id) => *id,
            None => {
                let id = match (&ask, i) {
                    (Some((card, _, _)), 0) => *card,
                    _ => shared.cards.fetch_add(1, Ordering::SeqCst),
                };
                draft.ids.push(id);
                id
            }
        };
        let question = ask
            .as_ref()
            .filter(|(card, _, _)| *card == id)
            .map(|(_, q, _)| q.clone());
        (shared.sink)(Update::Card(MeetingCard {
            id,
            at: ask
                .as_ref()
                .filter(|_| question.is_some())
                .map_or_else(now, |a| a.2),
            title,
            body,
            question,
            pending: true,
        }));
    }
}

fn finish_turn(shared: &Shared, event: &Value) {
    let turn = shared.turns.lock().unwrap().pop_front();
    let shown = std::mem::take(&mut shared.draft.lock().unwrap().ids);
    shared.draft.lock().unwrap().text.clear();
    let text = event["result"].as_str().unwrap_or_default();
    let failed = event["is_error"].as_bool().unwrap_or(false)
        || event["subtype"].as_str().is_some_and(|s| s != "success");
    eprintln!(
        "souffleur agent: turn {:.1} s, {}",
        event["duration_ms"].as_f64().unwrap_or_default() / 1000.0,
        if failed { "failed" } else { "ok" }
    );
    let at = now();
    let mut ids = shown.into_iter();
    let next_id = |ids: &mut std::vec::IntoIter<usize>| {
        ids.next()
            .unwrap_or_else(|| shared.cards.fetch_add(1, Ordering::SeqCst))
    };
    let (mut found, mut list) = if failed {
        (Vec::new(), None)
    } else {
        highlights(cards(text))
    };
    let mut question = None;
    let mut first_at = at;
    match turn {
        Some(Turn::Warmup) | None => {
            found.clear();
            list = None;
        }
        Some(Turn::Listen) => {}
        Some(Turn::Ask {
            card,
            question: q,
            at: asked,
        }) => {
            if failed {
                found = vec![(String::new(), format!("Fehler: {}", text.trim()))];
            } else if found.is_empty() {
                found = vec![(String::new(), "Nichts dazu gefunden.".into())];
            }
            // The question's card is the first; drafts already used it.
            ids = std::iter::once(card)
                .chain(ids.filter(|id| *id != card))
                .collect::<Vec<_>>()
                .into_iter();
            question = Some(q);
            first_at = asked;
        }
    }
    for (i, (title, body)) in found.into_iter().enumerate() {
        shared.card(MeetingCard {
            id: next_id(&mut ids),
            at: if i == 0 { first_at } else { at },
            title,
            body,
            question: if i == 0 { question.take() } else { None },
            pending: false,
        });
    }
    if let Some(list) = list {
        shared.highlights(list);
    }
    // Drafts the final answer no longer has (written before a lookup changed its mind) go.
    for id in ids {
        (shared.sink)(Update::Card(MeetingCard {
            id,
            at,
            title: String::new(),
            body: String::new(),
            question: None,
            pending: false,
        }));
    }
    let busy = shared.busy();
    shared.status(|s| {
        if failed {
            s.problem = text.trim().chars().take(200).collect();
        } else if s.agent != "problem" {
            s.problem.clear();
        }
        s.agent = if busy { "thinking" } else { "listening" }.into();
        s.activity = String::new();
    });
}

/// A recording standing in for the Mac's sound, delivered as fast as it would play.
struct Replay {
    samples: Vec<f32>,
    rate: u32,
    pos: usize,
    started: Instant,
}

enum Others {
    Tap(crate::platform::system_audio::SystemAudio),
    Replay(Replay),
}

impl Others {
    fn take(&mut self) -> Vec<f32> {
        match self {
            Others::Tap(t) => t.take(),
            Others::Replay(r) => {
                let due = ((r.started.elapsed().as_secs_f64() * r.rate as f64) as usize)
                    .min(r.samples.len());
                let out = r.samples[r.pos..due].to_vec();
                r.pos = due;
                out
            }
        }
    }

    fn ended(&mut self) -> bool {
        match self {
            Others::Tap(t) => t.ended(),
            Others::Replay(r) => r.pos >= r.samples.len(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The meeting loop

/// One source: its segmenter and the line the utterance being spoken shows as.
struct Source {
    who: &'static str,
    cut: Segmenter,
    line: Option<usize>,
    live_at: Instant,
    live_text: String,
}

impl Source {
    fn new(who: &'static str, rate: u32, base: f64) -> Self {
        Source {
            who,
            cut: Segmenter::new(rate, base),
            line: None,
            live_at: Instant::now(),
            live_text: String::new(),
        }
    }
}

struct Lines {
    next: usize,
    /// Final lines not yet sent to the agent.
    unsent: Vec<String>,
    last_final: Instant,
    /// Recent final lines of the others, for the echo check: (end, text).
    others: VecDeque<(f64, String)>,
}

fn to_model(samples: &[f32], rate: u32) -> Vec<f32> {
    resample(samples.to_vec(), rate, MODEL_RATE).unwrap_or_default()
}

fn run(setup: Setup, sink: Sink, asked: Receiver<String>, stop: &AtomicBool) -> Result<()> {
    let config = setup.config;
    let started = now();
    let shared = Arc::new(Shared {
        sink: sink.clone(),
        status: Mutex::new(MeetingStatus {
            agent: "starting".into(),
            system_audio: if config.meeting_system_audio {
                "on"
            } else {
                "off"
            }
            .into(),
            ..MeetingStatus::default()
        }),
        turns: Mutex::new(VecDeque::new()),
        cards: AtomicUsize::new(0),
        transcript: Mutex::new(Transcript::create(&setup.folder, started)),
        draft: Mutex::new(Draft {
            text: String::new(),
            ids: Vec::new(),
        }),
        highlights: Mutex::new(Vec::new()),
        stopping: AtomicBool::new(false),
    });
    shared.status(|_| {});

    // The agent first: it takes a few seconds to connect its tools, the audio is quick.
    let mut agent = match Agent::start(&config, shared.clone()) {
        Ok(mut agent) => {
            shared.turns.lock().unwrap().push_back(Turn::Warmup);
            agent.send(&warmup(&crate::profile::current().souffleur.warmup_tools))?;
            Some(agent)
        }
        Err(e) => {
            shared.status(|s| {
                s.agent = "problem".into();
                s.problem = format!("{e:#}");
            });
            None
        }
    };

    let (model_dir, fallback) = asr::resolve(&config.model);
    if let Some(note) = fallback {
        eprintln!("souffleur: {note}");
    }
    let mut asr = Asr::load(&model_dir)?;
    asr.set_language(&config.language);
    asr.set_boost(
        &config.vocabulary(),
        config.boost_vocabulary.then_some(config.boost_weight),
    );
    let corrector = Corrector::load(&setup.replacements, &config.vocabulary());

    let replay = setup.replay.is_some();
    let mut system = None;
    let mut others = None;
    let recorder = match &setup.replay {
        Some(path) => {
            let clip = crate::audio::read_wav(path)?;
            others = Some(Source::new("others", clip.rate, now()));
            system = Some(Others::Replay(Replay {
                samples: clip.samples,
                rate: clip.rate,
                pos: 0,
                started: Instant::now(),
            }));
            None
        }
        None => {
            let (recorder, name) = Recorder::start_named(&config.meeting_microphone)?;
            eprintln!("souffleur: microphone {name}");
            Some(recorder)
        }
    };
    let rate = recorder.as_ref().map_or(MODEL_RATE, |r| r.take().rate);
    let mut mic = Source::new("me", rate, now());
    if config.meeting_system_audio && !replay {
        match crate::platform::system_audio::SystemAudio::start() {
            Ok(s) => {
                others = Some(Source::new("others", s.rate, now()));
                system = Some(Others::Tap(s));
            }
            Err(e) => {
                eprintln!("souffleur: system audio: {e:#}");
                shared.status(|s| {
                    s.system_audio = "problem".into();
                    s.problem =
                        format!("{}: {e:#}", crate::text::pick("Systemton", "System audio"));
                });
            }
        }
    }

    let mut lines = Lines {
        next: 0,
        unsent: Vec::new(),
        last_final: Instant::now(),
        others: VecDeque::new(),
    };
    let mut status_at = Instant::now();
    let mut system_heard = false;
    let mut silent_since = Instant::now();
    // A replay ends once the recording is through and the agent has answered it.
    let mut replayed: Option<Instant> = None;
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(TICK);
        if let Some(recorder) = &recorder {
            let clip: Clip = recorder.take();
            for segment in mic.cut.push(&clip.samples) {
                finish(&mut mic, segment, &mut asr, &corrector, &shared, &mut lines);
            }
        }
        if let (Some(sys), Some(src)) = (&mut system, &mut others) {
            let samples = sys.take();
            if samples.iter().any(|s| *s != 0.0) {
                system_heard = true;
            }
            for segment in src.cut.push(&samples) {
                finish(src, segment, &mut asr, &corrector, &shared, &mut lines);
            }
            if sys.ended() {
                system = None;
                if replay {
                    if let Some(segment) = src.cut.flush() {
                        finish(src, segment, &mut asr, &corrector, &shared, &mut lines);
                    }
                    replayed = Some(Instant::now());
                    // The rest goes now, not after a pause.
                    lines.last_final = Instant::now() - FEED_PAUSE;
                } else {
                    shared.status(|s| s.system_audio = "problem".into());
                }
            }
        }
        if let Some(at) = replayed
            && ((lines.unsent.is_empty() && !shared.busy()) || at.elapsed().as_secs() > 120)
        {
            break;
        }
        for src in [Some(&mut mic), others.as_mut()].into_iter().flatten() {
            live(src, &mut asr, &corrector, &shared, &mut lines);
        }

        // Questions typed in the panel go at once (they queue behind a running turn).
        while let Ok(question) = asked.try_recv() {
            let Some(agent) = &mut agent else { break };
            let card = shared.cards.fetch_add(1, Ordering::SeqCst);
            let at = now();
            shared.card(MeetingCard {
                id: card,
                at,
                title: String::new(),
                body: String::new(),
                question: Some(question.clone()),
                pending: true,
            });
            let mut message = std::mem::take(&mut lines.unsent).join("\n");
            if !message.is_empty() {
                message.push_str("\n\n");
            }
            message.push_str(&format!("[Frage] {question}"));
            shared
                .turns
                .lock()
                .unwrap()
                .push_back(Turn::Ask { card, question, at });
            agent.send(&message)?;
            shared.status(|s| s.agent = "thinking".into());
        }

        // Said lines go once the agent is free and someone paused (or asked something).
        if let Some(agent) = &mut agent
            && !lines.unsent.is_empty()
            && !shared.busy()
        {
            let words: usize = lines
                .unsent
                .iter()
                .map(|l| l.split_whitespace().count())
                .sum();
            let asked = lines
                .unsent
                .last()
                .is_some_and(|l| l.trim_end().ends_with('?'));
            if asked || words >= FEED_WORDS || lines.last_final.elapsed() >= FEED_PAUSE {
                let message = std::mem::take(&mut lines.unsent).join("\n");
                shared.turns.lock().unwrap().push_back(Turn::Listen);
                agent.send(&message)?;
                shared.status(|s| s.agent = "thinking".into());
            }
        }

        if status_at.elapsed() >= STATUS_EVERY {
            status_at = Instant::now();
            if system_heard {
                silent_since = Instant::now();
            }
            let mic_db = mic.cut.level_db();
            let system_db = others.as_ref().map(|o| o.cut.level_db());
            let silent =
                system.is_some() && !system_heard && silent_since.elapsed().as_secs() >= 20;
            shared.status(|s| {
                s.mic_db = Some(mic_db);
                s.system_db = system_db;
                if system.is_some() {
                    s.system_audio = if silent { "silent" } else { "on" }.into();
                }
            });
        }
    }

    // The last words still count.
    if let Some(segment) = mic.cut.flush() {
        finish(&mut mic, segment, &mut asr, &corrector, &shared, &mut lines);
    }
    if let Some(src) = &mut others
        && let Some(segment) = src.cut.flush()
    {
        finish(src, segment, &mut asr, &corrector, &shared, &mut lines);
    }
    shared.stopping.store(true, Ordering::SeqCst);
    drop(agent);
    shared.status(|s| {
        s.agent = "off".into();
        s.activity = String::new();
        s.mic_db = None;
        s.system_db = None;
    });
    let mut list = shared.highlights.lock().unwrap().clone();
    let path = shared.transcript.lock().unwrap().path.clone();
    if lines.next > 0 {
        shared.status(|s| {
            s.agent = "thinking".into();
            s.activity =
                crate::text::pick("Das Wichtigste zum Schluss …", "Key facts at the end …").into();
        });
        let transcript = std::fs::read_to_string(&path).unwrap_or_default();
        match final_highlights(&config, &transcript, &list) {
            Ok(Some(last)) => {
                list = last;
                shared.highlights(list.clone());
            }
            Ok(None) => eprintln!("souffleur: nothing to keep at the end"),
            Err(e) => eprintln!("souffleur: key facts at the end: {e:#}"),
        }
        shared.status(|s| {
            s.agent = "off".into();
            s.activity = String::new();
        });
    }
    let path = {
        let mut transcript = shared.transcript.lock().unwrap();
        transcript.highlights(&list);
        transcript.file = None;
        transcript.path.clone()
    };
    eprintln!("souffleur: ended after {:.0} min", (now() - started) / 60.0);
    // Self-corrections out of the saved transcript, once, now that nothing is listening.
    let model = config.correct_model.clone();
    let tidying = std::thread::Builder::new()
        .name("souffleur-tidy".into())
        .spawn(move || tidy_transcript(&path, &model));
    // Replays (`dictate souffleur x.wav`) end the process right after: wait for it there.
    if replay && let Ok(t) = tidying {
        let _ = t.join();
    }
    Ok(())
}

/// The final "Das Wichtigste" over the whole transcript (lines, cards, the list so far): one
/// `claude -p` without tools. `None` when the agent finds nothing worth keeping.
fn final_highlights(
    config: &Config,
    transcript: &str,
    list: &[String],
) -> Result<Option<Vec<String>>> {
    let binary = claude_binary().context("Claude Code not found")?;
    let language = crate::text::pick("German", "English");
    let mut input = FINAL_PROMPT
        .replace("{language}", language)
        .replace("{highlights}", highlights_heading());
    input.push_str("\n<transcript>\n");
    input.push_str(transcript.trim());
    input.push_str("\n</transcript>\n");
    if !list.is_empty() {
        input.push_str("\nPinned during the meeting:\n");
        for l in list {
            input.push_str(&format!("- {l}\n"));
        }
    }
    let mut child = Command::new(&binary)
        .args(["-p", "--output-format", "text", "--no-session-persistence"])
        .args(["--tools", ""])
        .args(["--model", config.meeting_model.trim()])
        .current_dir(std::env::temp_dir())
        .env("PATH", agent_path())
        .env("MAX_THINKING_TOKENS", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {}", binary.display()))?;
    let mut stdin = child.stdin.take().context("claude stdin")?;
    std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut stdout = child.stdout.take().context("claude stdout")?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        out
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > FINAL_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            bail!("no answer within {} s", FINAL_TIMEOUT.as_secs());
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let out = reader.join().unwrap_or_default();
    ensure!(
        status.success(),
        "claude: {}",
        out.lines().last().unwrap_or("failed")
    );
    Ok(final_list(&out))
}

/// The final pass's answer: the list under the heading, `None` for "-" or no list.
fn final_list(answer: &str) -> Option<Vec<String>> {
    highlights(cards(answer)).1.filter(|l| !l.is_empty())
}

/// Refreshes the line of the utterance being spoken.
fn live(
    src: &mut Source,
    asr: &mut Asr,
    corrector: &Corrector,
    shared: &Shared,
    lines: &mut Lines,
) {
    if src.live_at.elapsed() < LIVE_EVERY {
        return;
    }
    let rate = src.cut.rate();
    let Some((start, samples)) = src.cut.open(0.8) else {
        return;
    };
    src.live_at = Instant::now();
    let text = match asr.transcribe_live(to_model(samples, rate)) {
        Ok(text) => crate::correct::drop_filler_words(&corrector.apply(&text)),
        Err(e) => {
            eprintln!("souffleur: live: {e:#}");
            return;
        }
    };
    if text.is_empty() || text == src.live_text {
        return;
    }
    src.live_text = text.clone();
    let id = *src.line.get_or_insert_with(|| {
        lines.next += 1;
        lines.next - 1
    });
    (shared.sink)(Update::Line(MeetingLine {
        id,
        who: src.who.into(),
        at: start,
        text,
        live: true,
    }));
}

/// An utterance ended: its final line, unless it is noise or the speakers' echo.
fn finish(
    src: &mut Source,
    segment: Segment,
    asr: &mut Asr,
    corrector: &Corrector,
    shared: &Shared,
    lines: &mut Lines,
) {
    let line = src.line.take();
    src.live_text.clear();
    let text = match asr.transcribe(to_model(&segment.samples, src.cut.rate())) {
        Ok(text) => crate::correct::drop_filler_words(&corrector.apply(&text)),
        Err(e) => {
            eprintln!("souffleur: transcribe: {e:#}");
            String::new()
        }
    };
    while lines
        .others
        .front()
        .is_some_and(|(end, _)| *end < segment.start - 5.0)
    {
        lines.others.pop_front();
    }
    let echo = src.who == "me" && {
        let recent: Vec<&str> = lines.others.iter().map(|(_, t)| t.as_str()).collect();
        is_echo(&text, &recent)
    };
    if echo {
        eprintln!("souffleur: dropped an echo of the speakers");
    }
    if is_noise(&text, segment.speech_seconds) || echo {
        // A line shown live goes away again (empty text = removed).
        if let Some(id) = line {
            (shared.sink)(Update::Line(MeetingLine {
                id,
                who: src.who.into(),
                at: segment.start,
                text: String::new(),
                live: false,
            }));
        }
        return;
    }
    let id = line.unwrap_or_else(|| {
        lines.next += 1;
        lines.next - 1
    });
    let line = MeetingLine {
        id,
        who: src.who.into(),
        at: segment.start,
        text: text.clone(),
        live: false,
    };
    shared.transcript.lock().unwrap().line(&line);
    (shared.sink)(Update::Line(line));
    if src.who == "others" {
        lines.others.push_back((segment.end, text.clone()));
    }
    lines.unsent.push(format!(
        "[{} {}] {text}",
        who_label(src.who),
        clock(segment.start)
    ));
    lines.last_final = Instant::now();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidying_a_transcript_touches_only_spoken_lines() {
        let text = "# Meeting 03.11.2026 15:18\n\n**15:18 Andere:** Wir treffen uns um drei, nein, um fünf.\n\n> **Souffleur 15:18:** Termin\n> um drei, nein\n\n**15:19 Ich:** Gut so.\n\n## Das Wichtigste\n\n- um drei, nein\n";
        let (out, changed) = tidy_lines(text, |lines| {
            assert_eq!(
                lines,
                ["Wir treffen uns um drei, nein, um fünf.", "Gut so."]
            );
            vec!["Wir treffen uns um fünf.".into(), "Gut so.".into()]
        });
        assert_eq!(changed, 1);
        assert_eq!(
            out,
            text.replace(
                "**15:18 Andere:** Wir treffen uns um drei, nein, um fünf.",
                "**15:18 Andere:** Wir treffen uns um fünf."
            )
        );
        // A clean that loses lines changes nothing.
        assert_eq!(tidy_lines(text, |_| vec![]).1, 0);
    }

    fn tone(seconds: f32, amplitude: f32, rate: u32) -> Vec<f32> {
        (0..(seconds * rate as f32) as usize)
            .map(|i| amplitude * (i as f32 * 220.0 * std::f32::consts::TAU / rate as f32).sin())
            .collect()
    }

    fn noise(seconds: f32, amplitude: f32, rate: u32) -> Vec<f32> {
        // Deterministic hiss.
        let mut x: u32 = 12345;
        (0..(seconds * rate as f32) as usize)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                amplitude * ((x >> 16) as f32 / 32768.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn das_wichtigste_is_no_card() {
        let answer = "## Nordlicht AG: Stand\n- Phase PoC\n\n## Das Wichtigste\n- Termin **5.11., 16 Uhr**\n* Kontakt: Vogt\n\n";
        let (cards, list) = highlights(cards(answer));
        assert_eq!(
            cards,
            vec![("Nordlicht AG: Stand".into(), "- Phase PoC".into())]
        );
        assert_eq!(
            list,
            Some(vec![
                "Termin **5.11., 16 Uhr**".into(),
                "Kontakt: Vogt".into()
            ])
        );
        let (cards, list) = highlights(super::cards("## Nur eine Karte\nText"));
        assert_eq!(cards.len(), 1);
        assert_eq!(list, None);
        // Only the list, no card: a turn that just updates it.
        let (cards, list) = highlights(super::cards("## Das Wichtigste:\n- A"));
        assert!(cards.is_empty());
        assert_eq!(list, Some(vec!["A".into()]));
    }

    #[test]
    fn key_facts_in_english_too() {
        let (cards, list) = highlights(cards(
            "## Northwind: status\n- PoC\n\n## Key facts\n- Next call **Nov 5**\n",
        ));
        assert_eq!(cards.len(), 1);
        assert_eq!(list, Some(vec!["Next call **Nov 5**".into()]));
        let (cards, list) = highlights(super::cards("## Key Facts:\n- A"));
        assert!(cards.is_empty());
        assert_eq!(list, Some(vec!["A".into()]));
        let saved = "# Meeting\n\n**13:29 Andere:** Hallo\n\n## Key facts\n\n- A\n";
        assert!(answers_only(saved).ends_with("## Key facts\n\n- A\n"));
    }

    const SAVED: &str = "# Meeting 03.11.2026 13:28\n\nMitschrift des Souffleurs (Dictate). Ich = Mikrofon, Andere = Ton des Macs.\n\n\
**13:29 Andere:** Wir wollten über Nordlicht sprechen.\n\n\
> **Souffleur 13:29:** Nordlicht AG: Stand\n> - PoC Prognosen\n\n\
**13:31 Ich:** Wer ist Ansprechpartner?\n\n\
> **Frage 13:31:** was ist die mail\n>\n> **Souffleur 13:31:** Mail Vogt\n> - j.vogt@nordlicht.example\n\n\
## Das Wichtigste\n\n- Termin 5.11.\n\n";

    #[test]
    fn summarizes_a_saved_meeting() {
        let s = summarize(PathBuf::from("x.md"), "2026-11-03 13-28".into(), SAVED);
        assert_eq!(s.lines, 2);
        assert_eq!(s.answers, 2);
        assert_eq!(s.span, Some(("13:29".into(), "13:31".into())));
        assert_eq!(s.topic, "Nordlicht AG: Stand");
    }

    #[test]
    fn answers_only_drops_the_transcript() {
        let out = answers_only(SAVED);
        assert!(out.starts_with("# Meeting 03.11.2026 13:28\n\n> **Souffleur 13:29:**"));
        assert!(!out.contains("Andere:"));
        assert!(!out.contains("Ich:"));
        assert!(out.contains("> **Frage 13:31:** was ist die mail"));
        assert!(out.ends_with("## Das Wichtigste\n\n- Termin 5.11.\n"));
        assert!(!out.contains("\n\n\n"));
    }

    #[test]
    fn cuts_utterances_at_pauses() {
        let rate = 48_000;
        let mut cut = Segmenter::new(rate, 1000.0);
        let mut audio = noise(1.0, 0.001, rate);
        audio.extend(tone(1.5, 0.2, rate));
        audio.extend(noise(1.0, 0.001, rate));
        audio.extend(tone(0.8, 0.2, rate));
        audio.extend(noise(1.0, 0.001, rate));
        let mut segments = Vec::new();
        // In uneven pieces, like the microphone delivers them.
        for piece in audio.chunks(4801) {
            segments.extend(cut.push(piece));
        }
        assert_eq!(segments.len(), 2);
        let first = &segments[0];
        assert!((first.start - 1000.7).abs() < 0.05, "start {}", first.start);
        let length = first.end - first.start;
        assert!((1.7..2.2).contains(&length), "length {length}");
        assert!((first.speech_seconds - 1.5).abs() < 0.1);
        assert!((segments[1].start - (1000.0 + 3.5 - 0.3)).abs() < 0.05);
        assert!(cut.level_db() < -55.0);
    }

    #[test]
    fn clicks_are_no_utterance_and_silence_is_quiet() {
        let rate = 16_000;
        let mut cut = Segmenter::new(rate, 0.0);
        let mut audio = vec![0.0; rate as usize];
        audio.extend(tone(0.1, 0.5, rate));
        audio.extend(vec![0.0; rate as usize * 2]);
        assert!(cut.push(&audio).is_empty());
        assert!(cut.flush().is_none());
        assert_eq!(cut.level_db(), -120.0);
    }

    #[test]
    fn long_speech_is_cut_and_open_shows_it() {
        let rate = 16_000;
        let mut cut = Segmenter::new(rate, 0.0);
        let mut found = cut.push(&tone(2.0, 0.2, rate));
        assert!(found.is_empty());
        let (start, samples) = cut.open(1.0).unwrap();
        assert!(start < 0.1 && samples.len() > rate as usize);
        found.extend(cut.push(&tone(29.0, 0.2, rate)));
        assert_eq!(found.len(), 1, "hard cut at 30 s");
        assert!(cut.flush().is_some(), "the rest is closed at the end");
    }

    #[test]
    fn echo_of_the_speakers() {
        assert!(is_echo(
            "wie weit ist Nordlicht mit dem Pilot",
            &["Wie weit ist eigentlich Nordlicht mit dem Pilot?"]
        ));
        assert!(!is_echo(
            "das schauen wir uns morgen an",
            &["Wie weit ist eigentlich Nordlicht mit dem Pilot?"]
        ));
        assert!(!is_echo("ja", &["nein"]));
        assert!(is_echo("Pilot", &["der Pilot"]));
        assert!(!is_echo("hallo", &[]));
    }

    #[test]
    fn noise_words() {
        assert!(is_noise("Thank you.", 0.5));
        assert!(!is_noise("Thank you.", 2.0));
        assert!(is_noise("…", 3.0));
        assert!(!is_noise("Nordlicht", 0.5));
    }

    #[test]
    fn answers_become_cards() {
        assert!(cards("-").is_empty());
        assert!(cards("  -\n").is_empty());
        assert!(cards("").is_empty());
        let found = cards(
            "## Nordlicht: Pilot läuft\n- seit 12.10.\n- [CRM](https://x)\n\n## **Offene Aufgabe**\nAngebot schicken\n",
        );
        assert_eq!(
            found,
            vec![
                (
                    "Nordlicht: Pilot läuft".to_string(),
                    "- seit 12.10.\n- [CRM](https://x)".to_string()
                ),
                ("Offene Aufgabe".to_string(), "Angebot schicken".to_string()),
            ]
        );
        assert_eq!(
            cards("Kein Treffer in Notion."),
            vec![(String::new(), "Kein Treffer in Notion.".to_string())]
        );
    }

    #[test]
    fn activities() {
        assert_eq!(
            activity("mcp__crm__get_account", &Value::Null),
            "Sucht im CRM"
        );
        assert_eq!(
            activity("mcp__claude_ai_Slack__slack_search_public", &Value::Null),
            "Sucht in Slack"
        );
        assert_eq!(
            activity("mcp__claude_ai_Google_Drive__search_files", &Value::Null),
            "Sucht in Drive"
        );
        assert_eq!(
            activity("Bash", &json!({"command": "gh pr list -R x"})),
            "Schaut auf GitHub"
        );
        assert_eq!(activity("Grep", &Value::Null), "Liest Dateien");
    }

    #[test]
    fn prompt_says_read_only_and_takes_extra_instructions() {
        let mut config = Config::default();
        assert!(prompt(&config).contains("READ ONLY"));
        assert!(prompt(&config).contains("The meeting itself counts"));
        // In the UI language (German in tests), every placeholder filled.
        assert!(prompt(&config).contains("Answer format (German, Markdown)"));
        assert!(prompt(&config).contains("\n## Das Wichtigste\n"));
        assert!(
            !prompt(&config).contains("{highlights}") && !prompt(&config).contains("{language}")
        );
        config.meeting_instructions = "Antworte auf Englisch.".into();
        assert!(prompt(&config).ends_with("Antworte auf Englisch.\n"));
        assert!(
            !prompt(&config).contains("SOLISTIQ"),
            "the built-in profile is neutral"
        );
    }

    #[test]
    fn warmup_loads_the_profiles_tools() {
        let tools = [
            "mcp__crm__search".to_string(),
            "mcp__notion__notion-search".into(),
        ];
        assert!(warmup(&tools).contains("(select:mcp__crm__search,mcp__notion__notion-search)"));
        assert!(!warmup(&[]).contains("select:"));
    }

    #[test]
    fn the_last_key_facts_come_from_the_final_answer() {
        assert_eq!(
            final_list(
                "## Das Wichtigste\n- Lieferung am Donnerstag\n- Angebot bis Freitag (Kai)\n"
            ),
            Some(vec![
                "Lieferung am Donnerstag".to_string(),
                "Angebot bis Freitag (Kai)".to_string()
            ])
        );
        assert_eq!(
            final_list("## Key facts\n- Pilot starts in May\n")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(final_list("-"), None);
        assert_eq!(final_list("## Das Wichtigste\n"), None);
        assert!(
            !FINAL_PROMPT
                .replace("{highlights}", "x")
                .replace("{language}", "y")
                .contains('{')
        );
    }
}
