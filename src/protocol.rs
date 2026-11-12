//! The messages between the main window's page (`ui/`, Svelte) and the app, as JSON. The page
//! sends a [`Request`] through `window.ipc.postMessage`, the app answers with an [`Event`] through
//! `dictate.receive(…)`. `ui/src/lib/protocol.ts` mirrors these types; the samples in
//! `ui/src/lib/protocol.fixtures.json` are checked against both sides. Platform independent.

use serde::{Deserialize, Serialize};

use crate::config::{Config, TriggerConfig};
use crate::history;
use crate::learn::{Candidate, Kind};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Load,
    Copy {
        text: String,
    },
    /// The words that differ between a dictation and its edited text.
    Find {
        original: String,
        edited: String,
    },
    /// Saves an edited dictation and learns the chosen corrections.
    SaveEntry {
        id: usize,
        text: String,
        learn: Vec<PageCandidate>,
    },
    Config {
        config: PageConfig,
    },
    Words {
        words: Vec<String>,
    },
    Replacements {
        replacements: Vec<Replacement>,
    },
    /// What the dictionary makes of a sentence.
    Try {
        text: String,
    },
    RecordTrigger {
        row: usize,
    },
    DownloadModel {
        name: String,
    },
    MicTest,
    OpenPrivacy {
        what: String,
    },
    OpenFolder,
    /// Souffleur: start listening (opens the side panel), stop.
    StartMeeting,
    StopMeeting,
    /// "Fortsetzen": the meeting that stopped last goes on (same transcript, same agent).
    ResumeMeeting,
    /// A question typed in the side panel, straight to the agent.
    AskMeeting {
        text: String,
    },
    /// Shows the side panel (the meeting does not have to run).
    OpenPanel,
    /// Shows the main window at `page` (the panel's settings button).
    OpenPage {
        page: String,
    },
    /// A link in an answer: opens in the browser (http/https/notion/slack only).
    OpenUrl {
        url: String,
    },
    /// The meeting transcripts folder in Finder.
    OpenMeetings,
    /// Mouse down on the title bar area: move the window.
    Drag,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    State {
        state: Box<PageState>,
    },
    Candidates {
        original: String,
        edited: String,
        candidates: Vec<PageCandidate>,
    },
    Tried {
        text: String,
        result: String,
    },
    Captured {
        row: usize,
        key: Option<String>,
        message: String,
    },
    MicLevel {
        db: Option<f32>,
        done: bool,
    },
    Note {
        text: String,
    },
    /// Go to a page, optionally with a dictation open for correcting (menu items).
    Show {
        page: String,
        entry: Option<usize>,
    },
    /// Souffleur: everything so far (on load and when a meeting starts).
    Meeting {
        meeting: Box<MeetingView>,
    },
    /// A transcript line, new or changed (`live` → final under the same id).
    MeetingLine {
        line: MeetingLine,
    },
    /// An answer of the agent, new or changed (same id).
    MeetingCard {
        card: MeetingCard,
    },
    /// Levels and what the agent does, a few times a second while the meeting runs.
    MeetingStatus {
        status: MeetingStatus,
    },
    /// "Das Wichtigste": the agent's current list of the meeting's key facts (replaces the last).
    MeetingHighlights {
        highlights: Vec<String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeetingView {
    pub running: bool,
    /// Unix seconds when it started; None before the first meeting.
    pub started: Option<i64>,
    pub lines: Vec<MeetingLine>,
    pub cards: Vec<MeetingCard>,
    pub status: MeetingStatus,
    /// "Das Wichtigste" so far: short Markdown lines, the most important first.
    #[serde(default)]
    pub highlights: Vec<String>,
    /// Stopped, and it can go on ("Fortsetzen").
    #[serde(default)]
    pub resumable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeetingLine {
    pub id: usize,
    /// "me" (the microphone) or "others" (what the Mac plays).
    pub who: String,
    /// Unix seconds (with fraction) when the speech began.
    pub at: f64,
    pub text: String,
    /// Still being spoken: the text may change.
    pub live: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeetingCard {
    pub id: usize,
    pub at: f64,
    pub title: String,
    /// Markdown: paragraphs, "- " lists, **bold**, [links](https://…), `code`.
    pub body: String,
    /// The question typed in the panel this answers; None when the agent spoke up by itself.
    pub question: Option<String>,
    /// Still being written.
    pub pending: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MeetingStatus {
    /// off | starting | listening | thinking | problem
    pub agent: String,
    /// What the agent does right now ("Sucht im CRM …"), empty when nothing.
    pub activity: String,
    /// Level of the last 300 ms in dBFS; None when that source is off.
    pub mic_db: Option<f32>,
    pub system_db: Option<f32>,
    /// on | off | silent (no permission?) | problem
    pub system_audio: String,
    /// Shown in the panel when not empty.
    pub problem: String,
    /// Asked to stop: the last words and the final key facts are still being done.
    #[serde(default)]
    pub ending: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageState {
    /// loading | ready | recording | transcribing | problem
    pub status: String,
    pub status_text: String,
    /// granted | missing | unknown
    pub accessibility: String,
    pub microphone: String,
    pub microphone_name: String,
    /// Microphones connected now (names), the default first.
    pub microphones: Vec<String>,
    pub config: PageConfig,
    pub words: Vec<Word>,
    pub replacements: Vec<Replacement>,
    pub models: Vec<PageModel>,
    /// What "Korrigieren" can use: local text models and "apple".
    pub correct_models: Vec<PageModel>,
    /// Oldest first; `id` is the position.
    pub history: Vec<PageEntry>,
    /// The language the page shows ("de" or "en"): `config.ui_language` resolved by the app.
    pub lang: String,
    /// Where the anonymous reports go ("owner/repo#7"); None when the profile names no target,
    /// then the page hides the switch.
    pub report_target: Option<String>,
    /// The profile's name beside the wordmark ("dictate / SOLISTIQ"); empty = none.
    #[serde(default)]
    pub profile: String,
    /// The OS the app runs on ([`platform`]): the page names keys and system settings after it.
    pub platform: String,
    /// Linux without a tray (`linux::Tray::Missing`): the window stands in for the menu and
    /// closing it quits; the page says so in a banner. Always false on macOS and Windows.
    #[serde(default)]
    pub no_tray: bool,
}

/// "macos" | "windows" | "linux": the OS this build is for, as `PageState.platform` names it.
pub fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "macos"
    }
}

/// The settings the page shows and changes; the rest of `Config` stays as it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageConfig {
    pub triggers: Vec<TriggerConfig>,
    pub model: String,
    /// Spoken language for models that take one ("auto", "de", "en").
    pub language: String,
    /// Language of the window, menus and Souffleur ("auto", "de", "en").
    pub ui_language: String,
    /// Chosen microphone name, "" = the Mac's default.
    pub microphone: String,
    pub sounds: String,
    pub pause_media: bool,
    pub mute_while_recording: bool,
    pub learn_from_corrections: bool,
    pub voice_commands: bool,
    /// The model for "Korrigieren" (a local text model or "apple").
    pub correct_model: String,
    pub boost_vocabulary: bool,
    pub share_reports: bool,
    pub start_at_login: bool,
    /// The bar at the bottom of the screen.
    pub show_bar: bool,
    /// Souffleur: its own microphone ("" = the Mac's default), the Mac's sound too, the model,
    /// extra instructions.
    pub meeting_microphone: String,
    pub meeting_system_audio: bool,
    pub meeting_model: String,
    pub meeting_instructions: String,
}

impl PageConfig {
    pub fn from(config: &Config, start_at_login: bool) -> Self {
        PageConfig {
            triggers: config.triggers.clone(),
            model: config.model.clone(),
            language: config.language.clone(),
            ui_language: config.ui_language.clone(),
            microphone: config.microphone.clone(),
            sounds: config.sounds.clone(),
            pause_media: config.pause_media,
            mute_while_recording: config.mute_while_recording,
            learn_from_corrections: config.learn_from_corrections,
            voice_commands: config.voice_commands,
            correct_model: config.correct_model.clone(),
            boost_vocabulary: config.boost_vocabulary,
            share_reports: config.share_reports,
            start_at_login,
            show_bar: config.show_bar,
            meeting_microphone: config.meeting_microphone.clone(),
            meeting_system_audio: config.meeting_system_audio,
            meeting_model: config.meeting_model.clone(),
            meeting_instructions: config.meeting_instructions.clone(),
        }
    }

    /// `base` with the page's settings applied.
    pub fn apply(&self, base: &Config) -> Config {
        Config {
            triggers: self.triggers.clone(),
            model: self.model.clone(),
            language: self.language.clone(),
            ui_language: self.ui_language.clone(),
            microphone: self.microphone.clone(),
            sounds: self.sounds.clone(),
            pause_media: self.pause_media,
            mute_while_recording: self.mute_while_recording,
            learn_from_corrections: self.learn_from_corrections,
            voice_commands: self.voice_commands,
            correct_model: self.correct_model.clone(),
            boost_vocabulary: self.boost_vocabulary,
            share_reports: self.share_reports,
            show_bar: self.show_bar,
            meeting_microphone: self.meeting_microphone.clone(),
            meeting_system_audio: self.meeting_system_audio,
            meeting_model: self.meeting_model.clone(),
            meeting_instructions: self.meeting_instructions.clone(),
            ..base.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub word: String,
    /// default | manual | learned
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replacement {
    pub heard: String,
    pub meant: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageModel {
    pub name: String,
    pub label: String,
    pub size: String,
    pub downloaded: bool,
    pub progress: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageEntry {
    pub id: usize,
    pub at: Option<i64>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageCandidate {
    pub heard: String,
    pub meant: String,
    /// dictionary | replacement
    pub kind: String,
}

impl From<&Candidate> for PageCandidate {
    fn from(c: &Candidate) -> Self {
        PageCandidate {
            heard: c.heard.clone(),
            meant: c.meant.clone(),
            kind: match c.kind {
                Kind::Dictionary => "dictionary",
                Kind::Replacement => "replacement",
            }
            .into(),
        }
    }
}

impl From<&PageCandidate> for Candidate {
    fn from(c: &PageCandidate) -> Self {
        Candidate {
            heard: c.heard.clone(),
            meant: c.meant.clone(),
            kind: if c.kind == "dictionary" {
                Kind::Dictionary
            } else {
                Kind::Replacement
            },
        }
    }
}

/// The dictionary for the page: the profile's default words marked as such, the rest as the
/// user's.
pub fn words(vocabulary: &[String], defaults: &[String]) -> Vec<Word> {
    vocabulary
        .iter()
        .map(|word| Word {
            word: word.clone(),
            source: if defaults.contains(word) {
                "default"
            } else {
                "manual"
            }
            .into(),
        })
        .collect()
}

pub fn entries(history: &[history::Entry]) -> Vec<PageEntry> {
    history
        .iter()
        .enumerate()
        .map(|(id, e)| PageEntry {
            id,
            at: e.at,
            text: e.text.clone(),
        })
        .collect()
}

/// "Parakeet Ultra (genau, 2,5 GB)" → ("Parakeet Ultra", "genau, 2,5 GB").
pub fn split_label(label: &str) -> (String, String) {
    match label.split_once(" (") {
        Some((name, rest)) => (name.to_string(), rest.trim_end_matches(')').to_string()),
        None => (label.to_string(), String::new()),
    }
}

/// The script that hands `event` to the page.
pub fn script(event: &Event) -> String {
    let json = serde_json::to_string(event).unwrap_or_else(|_| "null".into());
    format!("window.dictate && window.dictate.receive({json})")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample messages written by hand in `ui/`, used by its tests and mock too: every request the
    /// page sends must parse here, and every event must parse and come out the same.
    #[test]
    fn page_and_app_agree_on_the_messages() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../ui/src/lib/protocol.fixtures.json")).unwrap();
        let requests = fixtures["requests"].as_array().unwrap();
        assert!(requests.len() >= 14);
        for sample in requests {
            let parsed: Request = serde_json::from_value(sample.clone())
                .unwrap_or_else(|e| panic!("request {sample}: {e}"));
            assert_eq!(serde_json::to_value(&parsed).unwrap(), *sample);
        }
        let events = fixtures["events"].as_array().unwrap();
        assert!(events.len() >= 8);
        for sample in events {
            let parsed: Event = serde_json::from_value(sample.clone())
                .unwrap_or_else(|e| panic!("event {sample}: {e}"));
            assert_eq!(serde_json::to_value(&parsed).unwrap(), *sample);
        }
    }

    #[test]
    fn page_settings_keep_the_rest_of_the_config() {
        let mut base = Config::default();
        base.set_vocabulary(&["SOLISTIQ".into(), "Merge Request".into()]);
        let mut page = PageConfig::from(&base, true);
        page.sounds = "aus".into();
        page.triggers.truncate(1);
        let new = page.apply(&base);
        assert_eq!(new.sounds, "aus");
        assert_eq!(new.triggers.len(), 1);
        assert_eq!(new.vocabulary(), base.vocabulary());
        assert_eq!(new.boost_weight, base.boost_weight);
    }

    #[test]
    fn words_and_labels() {
        let listed = words(
            &["SOLISTIQ".into(), "Merge Request".into()],
            &["SOLISTIQ".into(), "Swarm".into()],
        );
        assert_eq!(listed[0].source, "default");
        assert_eq!(listed[1].source, "manual");
        assert_eq!(
            split_label("Parakeet Ultra (genau, 2,5 GB)"),
            ("Parakeet Ultra".into(), "genau, 2,5 GB".into())
        );
        assert_eq!(split_label("X"), ("X".into(), String::new()));
        let event = Event::Note {
            text: "a</script>\"".into(),
        };
        assert_eq!(
            script(&event),
            r#"window.dictate && window.dictate.receive({"type":"note","text":"a</script>\""})"#
        );
    }
}
