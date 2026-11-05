//! Reading aloud and talking things over: the system voice (`say`) speaks, Claude Code or an agent
//! plugin answers (the profile's `[talk] agent`).
//!
//! - `Speaker`: `say` as a child process we can stop; calls back when it ends by itself.
//! - `clean_for_speech`, `pick_voice`: what is spoken and in which voice.
//! - `Endpoint`: when someone has finished a sentence (speech, then a pause), so a conversation
//!   needs no key between the turns.
//! - `Agent`: the conversation partner. Claude Code (`claude -p`, `--resume` per turn; built in)
//!   or an agent plugin from the data folder's `agents/` (`docs/agent-plugins.md`; `agents/fleet` in the
//!   repository is one). Each turn takes a few seconds (a whole agent turn), so this is a
//!   walk-and-talk, not a phone call.
//!
//! Platform independent apart from `say`, which only exists on macOS.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};

/// What the agent is told when a conversation starts: it is heard, not read.
pub const BRIEF: &str = "Wir führen ein gesprochenes Gespräch: alles, was du schreibst, wird laut vorgelesen, und \
ich antworte mit der Stimme. Antworte deshalb kurz (zwei bis vier Sätze), in ganzen, gut sprechbaren Sätzen, \
ohne Markdown, ohne Listen, ohne Tabellen, ohne Links und ohne Code; wenn Code oder Pfade wichtig sind, beschreibe \
sie in Worten. Stelle höchstens eine Rückfrage. Antworte in der Sprache, in der ich spreche. Du hast hier keine \
Dateien und musst nichts bauen: du denkst mit mir.";

/// The brief in a line, after the first thing said in a conversation a web page handed over (it
/// started elsewhere, without the brief). A page may strip this line when it shows your words.
pub const REMINDER: &str = "(Sprachmodus: Antwort wird vorgelesen, also 2 bis 4 Sätze ohne Markdown, Listen oder Code, höchstens eine Rückfrage.)";

/// Markdown and the like out of a text that is going to be spoken: no "Sternchen Sternchen".
pub fn clean_for_speech(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") {
            in_code = !in_code;
            if in_code {
                out.push("(Codebeispiel ausgelassen)".to_string());
            }
            continue;
        }
        if in_code || line.is_empty() || line.chars().all(|c| "-*_=|: ".contains(c)) {
            continue;
        }
        let line = line
            .trim_start_matches(['#', '>', ' '])
            .trim_start_matches("- ")
            .trim_start_matches("* ");
        let mut clean = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' | '_' | '`' | '|' | '~' => {}
                '[' => {
                    // [label](url) → label
                    let label: String = chars.by_ref().take_while(|&c| c != ']').collect();
                    if chars.peek() == Some(&'(') {
                        for c in chars.by_ref() {
                            if c == ')' {
                                break;
                            }
                        }
                    }
                    clean.push_str(&label);
                }
                c => clean.push(c),
            }
        }
        let words: Vec<&str> = clean
            .split_whitespace()
            .map(|w| {
                if w.starts_with("http://") || w.starts_with("https://") {
                    "Link"
                } else {
                    w
                }
            })
            .collect();
        let clean = words.join(" ");
        if !clean.is_empty() {
            out.push(clean);
        }
    }
    out.join(". ").replace("..", ".").replace(":.", ":")
}

/// German or English voice by a rough look at the words; German is the default.
pub fn pick_voice<'a>(text: &str, german: &'a str, english: &'a str) -> &'a str {
    const EN: &[&str] = &[
        "the", "and", "is", "are", "of", "to", "that", "with", "for", "you", "this", "it", "not",
        "have", "was", "be",
    ];
    const DE: &[&str] = &[
        "der", "die", "das", "und", "ist", "nicht", "ein", "eine", "ich", "zu", "mit", "für",
        "auf", "den", "dem", "es", "wir", "sie", "auch", "von", "sich",
    ];
    let lower = text.to_lowercase();
    let (mut en, mut de) = (0, 0);
    for word in lower.split(|c: char| !c.is_alphabetic()) {
        en += usize::from(EN.contains(&word));
        de += usize::from(DE.contains(&word));
    }
    if en > de + 1 && !english.is_empty() {
        english
    } else {
        german
    }
}

/// `say` as a child process, so reading can be stopped (a key, Esc, a barge-in).
#[derive(Clone, Default)]
pub struct Speaker {
    child: Arc<Mutex<Option<Child>>>,
    generation: Arc<AtomicU64>,
}

impl Speaker {
    /// Starts speaking `text` (replacing what is spoken now); `on_done` runs once the voice has
    /// finished by itself, not after `stop` or a newer `speak`.
    pub fn speak(
        &self,
        text: &str,
        voice: &str,
        rate: u32,
        on_done: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        self.stop();
        let mut command = crate::platform::say(voice, rate)?;
        command.stdin(Stdio::piped());
        let mut child = command.spawn().context("start say")?;
        child
            .stdin
            .take()
            .context("say stdin")?
            .write_all(text.as_bytes())?;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.child.lock().unwrap() = Some(child);
        let (slot, current) = (self.child.clone(), self.generation.clone());
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(120));
                if current.load(Ordering::SeqCst) != generation {
                    return; // stopped or replaced
                }
                let mut slot = slot.lock().unwrap();
                match slot.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(None)) => {}
                    _ => {
                        *slot = None;
                        drop(slot);
                        if current.load(Ordering::SeqCst) == generation {
                            on_done();
                        }
                        return;
                    }
                }
            }
        });
        Ok(())
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    #[cfg(test)]
    pub fn is_speaking(&self) -> bool {
        self.child.lock().unwrap().is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    /// Nothing yet, or still talking.
    Going,
    /// Spoke, then paused long enough: the turn is over.
    Done,
    /// Nobody said anything for too long.
    Nothing,
}

/// Finds the end of a spoken turn from the level meter, fed every tick.
#[derive(Debug)]
pub struct Endpoint {
    tick: Duration,
    peak: f32,
    voiced: Duration,
    quiet: Duration,
    waited: Duration,
    spoke: bool,
}

/// Speech needs this much sound (dBFS over 300 ms) to count; room noise stays below.
const SPEECH_DB: f32 = -50.0;
/// A pause is a level this far below the loudest the speaker got (or below `SPEECH_DB`).
const PAUSE_BELOW_PEAK: f32 = 16.0;
/// The pause that ends a turn, how long speech must last to count, how long to wait for it.
const PAUSE: Duration = Duration::from_millis(1500);
const MIN_SPEECH: Duration = Duration::from_millis(400);
const GIVE_UP: Duration = Duration::from_secs(12);

impl Endpoint {
    pub fn new(tick: Duration) -> Self {
        Endpoint {
            tick,
            peak: f32::NEG_INFINITY,
            voiced: Duration::ZERO,
            quiet: Duration::ZERO,
            waited: Duration::ZERO,
            spoke: false,
        }
    }

    pub fn feed(&mut self, level_db: f32) -> Heard {
        self.waited += self.tick;
        if level_db > SPEECH_DB {
            self.peak = self.peak.max(level_db);
        }
        let pause = level_db < (self.peak - PAUSE_BELOW_PEAK).max(SPEECH_DB - 4.0);
        if level_db > SPEECH_DB && !pause {
            self.voiced += self.tick;
            self.quiet = Duration::ZERO;
            if self.voiced >= MIN_SPEECH {
                self.spoke = true;
            }
        } else if self.spoke {
            self.quiet += self.tick;
        }
        if self.spoke && self.quiet >= PAUSE {
            Heard::Done
        } else if !self.spoke && self.waited >= GIVE_UP {
            Heard::Nothing
        } else {
            Heard::Going
        }
    }
}

/// The other side of a conversation: Claude Code on this machine (`claude -p`, each turn resumes
/// the same Claude session; built in) or an agent plugin (`docs/agent-plugins.md`), as the profile says
/// (`[talk] agent`). Conversations are numbered; a plugin's numbers are its own (a web page's Talk
/// button may hand one over).
#[derive(Clone, Debug)]
pub enum Agent {
    Plugin(Plugin),
    Claude(Claude),
}

impl Agent {
    /// The profile's agent.
    pub fn for_conversation(config: &crate::config::Config) -> Result<Agent> {
        match crate::profile::current().talk.plugin() {
            Some(name) => Ok(Agent::Plugin(Plugin::find(name, &config.discuss_profile)?)),
            None => Ok(Agent::Claude(Claude::find(config)?)),
        }
    }

    /// For a conversation a web page hands over by its number: the profile's plugin.
    pub fn handed_over(config: &crate::config::Config) -> Result<Agent> {
        match crate::profile::current().talk.plugin() {
            Some(name) => Ok(Agent::Plugin(Plugin::find(name, &config.discuss_profile)?)),
            None => bail!("the profile's agent (Claude Code) cannot take over a conversation"),
        }
    }

    /// Starts the conversation with the brief and the first thing said; returns its number.
    pub fn start(&self, first: &str) -> Result<u64> {
        match self {
            Agent::Plugin(p) => p.start(first),
            Agent::Claude(c) => c.start(first),
        }
    }

    /// Says `text` in the conversation and waits for the answer. Without `text`: the answer to
    /// the start.
    pub fn ask(&self, id: u64, text: Option<&str>) -> Result<String> {
        match self {
            Agent::Plugin(p) => p.ask(id, text),
            Agent::Claude(c) => c.ask(id, text),
        }
    }

    pub fn close(&self, id: u64) {
        match self {
            Agent::Plugin(p) => p.close(id),
            Agent::Claude(c) => c.close(id),
        }
    }
}

/// Claude Code as the conversation partner: one `claude -p` per turn, `--resume` with the session
/// it returned, the brief as system prompt, the Souffleur's read-only tools (the profile's), its
/// folder and model, thinking off (a spoken turn should take seconds).
#[derive(Clone, Debug)]
pub struct Claude {
    claude: PathBuf,
    model: String,
    folder: PathBuf,
    tools: Vec<String>,
    /// Conversation number → Claude's session id and, right after the start, its first answer.
    sessions: Arc<Mutex<Vec<Conversation>>>,
}

/// Conversation number, Claude's session id, the first answer until it was read.
type Conversation = (u64, String, Option<String>);

static CONVERSATIONS: AtomicU64 = AtomicU64::new(1);

impl Claude {
    fn find(config: &crate::config::Config) -> Result<Claude> {
        let claude =
            crate::meeting::claude_binary().context("Claude Code (claude) is not installed")?;
        let profile = crate::profile::current();
        Ok(Claude {
            claude,
            model: config.meeting_model.trim().to_string(),
            folder: profile.agent_folder(&config.meeting_folder),
            tools: profile.souffleur.allowed_tools.clone(),
            sessions: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// One turn: `text` in the session `resume` (or a new one). Returns (session id, answer).
    fn turn(&self, text: &str, resume: Option<&str>) -> Result<(String, String)> {
        let mut command = Command::new(&self.claude);
        command
            .args(["-p", "--output-format", "json"])
            .args(["--append-system-prompt", BRIEF]);
        if !self.model.is_empty() {
            command.args(["--model", &self.model]);
        }
        if let Some(id) = resume {
            command.args(["--resume", id]);
        }
        if !self.tools.is_empty() {
            command.arg("--allowedTools").args(&self.tools);
        }
        let mut child = command
            // The prompt on stdin: `--allowedTools` takes every argument after it.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&self.folder)
            .env("PATH", crate::meeting::agent_path())
            .env("MAX_THINKING_TOKENS", "0")
            .spawn()
            .context("start claude")?;
        child
            .stdin
            .take()
            .context("claude stdin")?
            .write_all(text.as_bytes())?;
        let out = child.wait_with_output()?;
        let parsed = parse_claude(&String::from_utf8_lossy(&out.stdout));
        match parsed {
            Ok(answer) => Ok(answer),
            Err(e) if out.status.success() => Err(e),
            Err(_) => bail!(
                "claude: {}",
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .last()
                    .unwrap_or("failed")
            ),
        }
    }

    fn start(&self, first: &str) -> Result<u64> {
        let (session, answer) = self.turn(first, None)?;
        let id = CONVERSATIONS.fetch_add(1, Ordering::Relaxed);
        self.sessions
            .lock()
            .unwrap()
            .push((id, session, Some(answer)));
        Ok(id)
    }

    fn ask(&self, id: u64, text: Option<&str>) -> Result<String> {
        let session = {
            let mut sessions = self.sessions.lock().unwrap();
            let entry = sessions
                .iter_mut()
                .find(|(n, ..)| *n == id)
                .context("no such conversation")?;
            if text.is_none()
                && let Some(answer) = entry.2.take()
            {
                return Ok(answer);
            }
            entry.1.clone()
        };
        let (session, answer) = self.turn(text.unwrap_or("Weiter."), Some(&session))?;
        if let Some(entry) = self
            .sessions
            .lock()
            .unwrap()
            .iter_mut()
            .find(|(n, ..)| *n == id)
        {
            entry.1 = session;
        }
        Ok(answer)
    }

    fn close(&self, id: u64) {
        self.sessions.lock().unwrap().retain(|(n, ..)| *n != id);
    }
}

/// `claude -p --output-format json`'s answer: `{"result": …, "session_id": …, "is_error": …}`.
fn parse_claude(json: &str) -> Result<(String, String)> {
    let value: serde_json::Value =
        serde_json::from_str(json.trim()).context("claude's answer is no JSON")?;
    let result = value["result"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    if value["is_error"].as_bool() == Some(true) {
        bail!(
            "claude: {}",
            if result.is_empty() { "failed" } else { &result }
        );
    }
    let session = value["session_id"]
        .as_str()
        .context("no session id in claude's answer")?
        .to_string();
    ensure!(!result.is_empty(), "claude answered nothing");
    Ok((session, result))
}

/// An agent plugin as the other side of a conversation: an executable in the data folder's
/// `agents/` (`docs/agent-plugins.md`), named by the profile's `[talk] agent`. Three calls:
/// `start` (the brief and the first thing said on stdin; prints the conversation's number),
/// `ask <id>` (what was said on stdin, empty for the answer to the start; prints the answer) and
/// `close <id>`. `DICTATE_AGENT_PROFILE` carries the setting `discuss_profile`.
#[derive(Clone, Debug)]
pub struct Plugin {
    name: String,
    program: PathBuf,
    profile: String,
}

impl Plugin {
    fn find(name: &str, profile: &str) -> Result<Plugin> {
        let dir = crate::asr::data_dir().join("agents");
        let program = dictate_core::locate::agent_plugin(&dir, name, cfg!(windows))
            .into_iter()
            .find(|p| p.is_file())
            .with_context(|| format!("no agent plugin {name:?} in {}", dir.display()))?;
        Ok(Plugin {
            name: name.to_string(),
            program,
            profile: profile.trim().to_string(),
        })
    }

    fn run(&self, args: &[&str], stdin: &str) -> Result<String> {
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PATH", crate::meeting::agent_path())
            .env("DICTATE_AGENT_PROFILE", &self.profile)
            .spawn()
            .with_context(|| format!("start agent plugin {}", self.name))?;
        child
            .stdin
            .take()
            .context("plugin stdin")?
            .write_all(stdin.as_bytes())?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!(
                "{}: {}",
                self.name,
                String::from_utf8_lossy(&out.stderr)
                    .lines()
                    .last()
                    .unwrap_or("failed")
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Starts the conversation with the brief and the first thing said; returns its number.
    pub fn start(&self, first: &str) -> Result<u64> {
        parse_id(&self.run(&["start"], &format!("{BRIEF}\n\n{first}"))?)
    }

    /// Says `text` in the conversation (none: the answer to the start) and returns the answer.
    pub fn ask(&self, id: u64, text: Option<&str>) -> Result<String> {
        let answer = self.run(&["ask", &id.to_string()], text.unwrap_or(""))?;
        ensure!(!answer.is_empty(), "no answer from the agent");
        Ok(answer)
    }

    pub fn close(&self, id: u64) {
        let _ = self.run(&["close", &id.to_string()], "");
    }
}

/// A plugin's `start` output: the number on its first line, or `{"id": n}`.
fn parse_id(out: &str) -> Result<u64> {
    let first = out.lines().next().unwrap_or("").trim();
    if let Ok(id) = first.parse() {
        return Ok(id);
    }
    serde_json::from_str::<serde_json::Value>(out)
        .ok()
        .and_then(|v| v["id"].as_u64())
        .with_context(|| format!("no conversation number in the plugin's answer: {first:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_not_read_out() {
        let text = "## Titel\n\nDas ist **wichtig** und `code`.\n\n- erstens\n- zweitens\n\n```rust\nfn main() {}\n```\n\nSiehe [die Doku](https://x.y/z) oder https://a.b/c.";
        let spoken = clean_for_speech(text);
        assert!(
            !spoken.contains('*') && !spoken.contains('`') && !spoken.contains('#'),
            "{spoken}"
        );
        assert!(!spoken.contains("fn main"), "{spoken}");
        assert!(!spoken.contains("https"), "{spoken}");
        assert!(spoken.contains("Das ist wichtig und code."), "{spoken}");
        assert!(spoken.contains("die Doku"), "{spoken}");
        assert!(spoken.contains("erstens"), "{spoken}");
    }

    #[test]
    fn a_plugin_names_its_conversation() {
        assert_eq!(parse_id("42\n").unwrap(), 42);
        assert_eq!(parse_id("{\"id\": 7, \"url\": \"x\"}").unwrap(), 7);
        assert!(parse_id("started").is_err());
    }

    #[test]
    fn claude_answers_are_read_from_its_json() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"Gute Frage. Was meinst du?","session_id":"5f1c-…"}"#;
        assert_eq!(
            parse_claude(ok).unwrap(),
            (
                "5f1c-…".to_string(),
                "Gute Frage. Was meinst du?".to_string()
            )
        );
        let failed = r#"{"type":"result","is_error":true,"result":"Credit balance is too low","session_id":"x"}"#;
        assert!(format!("{:#}", parse_claude(failed).unwrap_err()).contains("Credit balance"));
        assert!(parse_claude("not json").is_err());
        assert!(parse_claude(r#"{"result":"","session_id":"x"}"#).is_err());
    }

    #[test]
    fn the_voice_follows_the_language() {
        assert_eq!(
            pick_voice("Das ist nicht der Plan, den wir haben.", "Anna", "Samantha"),
            "Anna"
        );
        assert_eq!(
            pick_voice("This is the plan that we have for you.", "Anna", "Samantha"),
            "Samantha"
        );
        assert_eq!(pick_voice("Pull Request", "Anna", "Samantha"), "Anna");
        assert_eq!(pick_voice("This is the plan", "Anna", ""), "Anna");
    }

    fn feed(e: &mut Endpoint, levels: &[(f32, usize)]) -> Heard {
        let mut last = Heard::Going;
        for &(db, n) in levels {
            for _ in 0..n {
                last = e.feed(db);
                if last != Heard::Going {
                    return last;
                }
            }
        }
        last
    }

    #[test]
    fn a_pause_after_speech_ends_the_turn() {
        let tick = Duration::from_millis(100);
        let mut e = Endpoint::new(tick);
        // 1 s of room noise, 3 s of speech with small dips, then silence: done after ~1.5 s.
        assert_eq!(
            feed(&mut e, &[(-62.0, 10), (-30.0, 20), (-45.0, 3), (-28.0, 7)]),
            Heard::Going
        );
        assert_eq!(feed(&mut e, &[(-60.0, 14)]), Heard::Going);
        assert_eq!(feed(&mut e, &[(-60.0, 3)]), Heard::Done);
    }

    #[test]
    fn a_quiet_speaker_counts_and_noise_alone_gives_up() {
        let tick = Duration::from_millis(100);
        let mut e = Endpoint::new(tick);
        assert_eq!(feed(&mut e, &[(-44.0, 10), (-62.0, 20)]), Heard::Done);
        let mut e = Endpoint::new(tick);
        assert_eq!(feed(&mut e, &[(-64.0, 200)]), Heard::Nothing);
        // A click is not speech.
        let mut e = Endpoint::new(tick);
        assert_eq!(feed(&mut e, &[(-20.0, 1), (-64.0, 200)]), Heard::Nothing);
    }

    #[test]
    fn say_can_be_stopped_and_reports_its_end() {
        if !std::path::Path::new("/usr/bin/say").exists() {
            return;
        }
        let speaker = Speaker::default();
        let (tx, rx) = std::sync::mpsc::channel();
        // An empty text ends at once: the callback must come.
        speaker
            .speak("", "", 180, move || tx.send(()).unwrap())
            .unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
        // A long text, stopped: no callback.
        let (tx, rx) = std::sync::mpsc::channel();
        speaker
            .speak(
                &"Das ist ein langer Satz. ".repeat(30),
                "",
                180,
                move || tx.send(()).unwrap(),
            )
            .unwrap();
        assert!(speaker.is_speaking());
        speaker.stop();
        assert!(!speaker.is_speaking());
        assert!(rx.recv_timeout(Duration::from_millis(600)).is_err());
    }
}
