//! Settings that survive a restart: `~/Library/Application Support/dictate/config.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::trigger::{Mode, Trigger, parse_trigger};

/// What a voice shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceAction {
    Read,
    Discuss,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Start/stop cues: `leise`, `mittel` or `aus`.
    #[serde(default = "default_sounds")]
    pub sounds: String,
    /// An audio file of your own to play instead of the start cue (any format `afplay` takes on
    /// macOS); empty = the built-in cue. Only in config.toml, not on the settings page.
    #[serde(default)]
    pub start_sound: String,
    /// Mute the Mac's sound output while recording.
    #[serde(default)]
    pub mute_while_recording: bool,
    /// Pause whatever is playing (music, videos, podcasts) while recording and resume afterwards.
    #[serde(default = "yes")]
    pub pause_media: bool,
    /// Speech model (`dictate model list`); falls back to parakeet-ultra when not downloaded.
    #[serde(default = "default_model")]
    pub model: String,
    /// Spoken language for models that take one ("de", "en", …); "auto" = the model detects it
    /// (Parakeet always does; Cohere Transcribe cannot and takes German then).
    #[serde(default = "default_language")]
    pub language: String,
    /// The language of the menus, the window and the Souffleur's answers: "auto" (the system's
    /// preferred language: German when it is German, else English), "de" or "en" (`text.rs`).
    #[serde(default = "default_language")]
    pub ui_language: String,
    /// The microphone to record from (its name, `dictate mic` lists them); empty = the Mac's
    /// default input (System Settings → Sound). A chosen one that is not connected falls back
    /// to the default.
    #[serde(default)]
    pub microphone: String,
    /// After pasting, watch the text field for a moment and learn the user's corrections of
    /// misheard words (`learn.rs`), reading the field via Accessibility.
    #[serde(default = "yes")]
    pub learn_from_corrections: bool,
    /// Spoken layout commands: "neue Zeile", "neuer Absatz", "Spiegelstrich" (`commands.rs`).
    #[serde(default = "yes")]
    pub voice_commands: bool,
    /// Which model proposes self-corrections for "Korrigieren": a local text model (`llm.rs`,
    /// used once downloaded) or "apple" (Apple's on-device model).
    #[serde(default = "default_correct_model")]
    pub correct_model: String,
    /// Also favour dictionary words while decoding, not only afterwards (`boost.rs`).
    #[serde(default = "yes")]
    pub boost_vocabulary: bool,
    /// How strongly: bonus on a token's logit. Measured on TTS: 1.5 gains without
    /// false positives, 3 starts hearing "Sonnet" in "Sonett".
    #[serde(default = "default_boost_weight")]
    pub boost_weight: f32,
    /// Once a day, comment an anonymous summary (numbers, no dictation text; `report.rs`) on the
    /// standing GitHub issue the profile names, so the agents that improve Dictate can see how it
    /// runs. Without one in the profile nothing is sent.
    #[serde(default = "yes")]
    pub share_reports: bool,
    /// Reads the selected text aloud (press again: stop). Empty = no shortcut, the menu still works.
    #[serde(default = "default_read_key")]
    pub read_key: String,
    /// Talk it over with an agent: you speak, the agent answers aloud, you speak again; Esc
    /// ends it. Empty = no shortcut.
    #[serde(default = "default_discuss_key")]
    pub discuss_key: String,
    /// The voice: "pocket" (PocketTTS on the device, German "juergen"; the first use fetches 450 MB)
    /// or "system" (macOS voices only). English text and any failure use the system voices.
    #[serde(default = "default_voice_engine")]
    pub voice_engine: String,
    #[serde(default = "default_voice_pocket")]
    pub voice_pocket: String,
    /// `say` voices for German and for English text, and the speed in words per minute.
    #[serde(default = "default_voice_de")]
    pub voice_de: String,
    #[serde(default = "default_voice_en")]
    pub voice_en: String,
    #[serde(default = "default_voice_rate")]
    pub voice_rate: u32,
    /// How much faster than normal both voices speak (1.0 = as made; 0.5–2.0): PocketTTS plays at
    /// this rate with its pitch kept, the system voice gets `voice_rate` × this.
    #[serde(default = "default_voice_speed")]
    pub voice_speed: f32,
    /// Handed to an agent plugin as `DICTATE_AGENT_PROFILE` (`docs/agent-plugins.md`; for `fleet` the
    /// profile id or name the session runs on); empty = the plugin's default.
    #[serde(default)]
    pub discuss_profile: String,
    /// The bar at the bottom of the screen (`bar.rs`): Diktieren, Souffleur, Einstellungen. Off by
    /// default: the recorder overlay has Einstellungen and Souffleur beside its pill.
    #[serde(default)]
    pub show_bar: bool,
    /// Souffleur (`meeting.rs`): the microphone it listens to, apart from the dictation one
    /// (name; empty = the Mac's default input).
    #[serde(default)]
    pub meeting_microphone: String,
    /// Also listen to what the Mac plays (the other people in a call), via a Core Audio tap.
    #[serde(default = "yes")]
    pub meeting_system_audio: bool,
    /// The Claude model the Souffleur's agent runs on ("sonnet", "haiku", "opus" or a full id).
    #[serde(default = "default_meeting_model")]
    pub meeting_model: String,
    /// Extra instructions for the agent, appended to `src/meeting-prompt.txt`.
    #[serde(default)]
    pub meeting_instructions: String,
    /// The folder the agent works in (its CLAUDE.md/AGENTS.md, the repos it can read); empty =
    /// the profile's folder when it exists, else the home folder.
    #[serde(default)]
    pub meeting_folder: String,
    /// Words the transcript is corrected towards (see `vocab.rs`): what the user changed in the
    /// profile's default words. `vocabulary()` is the whole list.
    #[serde(default)]
    pub dictionary: Dictionary,
    #[serde(rename = "trigger", default = "default_triggers")]
    pub triggers: Vec<TriggerConfig>,
}

/// The dictionary as a difference to the profile's default words (`profile.rs`): the user's own
/// words and the default words they deleted. The defaults themselves are never stored, so words
/// added to the profile later reach every Mac.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dictionary {
    #[serde(default)]
    pub added: Vec<String>,
    #[serde(default)]
    pub removed: Vec<String>,
}

impl Dictionary {
    /// The defaults the user kept, then their own words.
    pub fn words(&self, defaults: &[String]) -> Vec<String> {
        let mut words: Vec<String> = defaults
            .iter()
            .filter(|w| !self.removed.contains(w))
            .cloned()
            .collect();
        for word in &self.added {
            if !words.contains(word) {
                words.push(word.clone());
            }
        }
        words
    }

    /// The difference that gives this whole list.
    pub fn from_words(words: &[String], defaults: &[String]) -> Self {
        let is_default = |w: &String| defaults.contains(w);
        let mut added: Vec<String> = Vec::new();
        for word in words {
            if !is_default(word) && !added.contains(word) {
                added.push(word.clone());
            }
        }
        let removed = defaults
            .iter()
            .filter(|d| !words.contains(d))
            .cloned()
            .collect();
        Dictionary { added, removed }
    }

    /// From a file of the old kind, which held the whole list: a default missing there was
    /// deleted.
    fn from_old_list(words: &[String], defaults: &[String]) -> Self {
        Dictionary::from_words(words, defaults)
    }
}

fn default_meeting_model() -> String {
    "sonnet".into()
}

fn default_read_key() -> String {
    "shift+alt+cmd+6".into()
}

fn default_discuss_key() -> String {
    "shift+alt+cmd+7".into()
}

fn default_voice_engine() -> String {
    "pocket".into()
}

fn default_voice_pocket() -> String {
    "juergen".into()
}

fn default_voice_de() -> String {
    "Anna".into()
}

fn default_voice_en() -> String {
    "Samantha".into()
}

fn default_voice_rate() -> u32 {
    190
}

fn default_voice_speed() -> f32 {
    1.25
}

impl Config {
    /// `voice_speed`, kept to what still sounds like speech.
    pub fn speed(&self) -> f32 {
        if self.voice_speed.is_finite() {
            self.voice_speed.clamp(0.5, 2.0)
        } else {
            1.0
        }
    }
}

fn default_model() -> String {
    crate::asr::DEFAULT_MODEL.into()
}

fn default_language() -> String {
    "auto".into()
}

/// The text model "Korrigieren" uses by default (`llm.rs` in the app runs it).
pub const DEFAULT_CORRECT_MODEL: &str = "gemma-4-e4b";

fn default_correct_model() -> String {
    DEFAULT_CORRECT_MODEL.into()
}

fn default_boost_weight() -> f32 {
    1.5
}

fn yes() -> bool {
    true
}

fn default_sounds() -> String {
    "leise".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// As accepted by `--trigger`: `fn`, `shift+alt+cmd+5`, `alt+cmd+fn`, `mouse3`, …
    pub key: String,
    /// `hold`, `toggle` or `hybrid`.
    pub mode: String,
    /// Let Apple's language model on the Mac lay the text out (paragraphs, lists) before pasting
    /// (`format.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub format: bool,
    /// Drop self-corrections ("um drei, äh nein, um fünf") and filler words before pasting
    /// (`correct.rs`); with Gemma in the same pass as `format` (the window offers Aus,
    /// Formatieren, Formatieren + Korrigieren).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub correct: bool,
}

fn default_triggers() -> Vec<TriggerConfig> {
    // Windows and Linux have no Fn key (`hook.rs`, `linux.rs`): right Alt, and F13 for
    // keyboards or mice that have it.
    let keys = match cfg!(target_os = "macos") {
        true => ["fn", "shift+alt+cmd+5"],
        false => ["right-alt", "f13"],
    };
    keys.iter()
        .map(|key| TriggerConfig {
            key: key.to_string(),
            mode: "hybrid".into(),
            // "Formatieren + Korrigieren" is the default for new setups.
            format: true,
            correct: true,
        })
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sounds: default_sounds(),
            start_sound: String::new(),
            mute_while_recording: false,
            pause_media: true,
            model: default_model(),
            language: default_language(),
            ui_language: default_language(),
            microphone: String::new(),
            learn_from_corrections: true,
            voice_commands: true,
            correct_model: default_correct_model(),
            boost_vocabulary: true,
            boost_weight: default_boost_weight(),
            share_reports: true,
            read_key: default_read_key(),
            discuss_key: default_discuss_key(),
            voice_engine: default_voice_engine(),
            voice_pocket: default_voice_pocket(),
            voice_de: default_voice_de(),
            voice_en: default_voice_en(),
            voice_rate: default_voice_rate(),
            voice_speed: default_voice_speed(),
            discuss_profile: String::new(),
            show_bar: false,
            meeting_microphone: String::new(),
            meeting_system_audio: true,
            meeting_model: default_meeting_model(),
            meeting_instructions: String::new(),
            meeting_folder: String::new(),
            dictionary: Dictionary::default(),
            triggers: default_triggers(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        crate::asr::data_dir().join("config.toml")
    }

    /// Missing file = defaults.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text).with_context(|| format!("in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    /// A config file's text. Files from before `[dictionary]` held the whole word list as
    /// `vocabulary`; that becomes the difference here and goes away with the next save.
    pub fn parse(text: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Old {
            vocabulary: Option<Vec<String>>,
            dictionary: Option<toml::Value>,
        }
        let mut config: Config = toml::from_str(text)?;
        if let Old {
            vocabulary: Some(words),
            dictionary: None,
        } = toml::from_str(text)?
        {
            config.dictionary =
                Dictionary::from_old_list(&words, &crate::profile::current().dictionary);
        }
        Ok(config)
    }

    /// The whole dictionary: the profile's default words the user kept, then their own.
    pub fn vocabulary(&self) -> Vec<String> {
        self.dictionary.words(&crate::profile::current().dictionary)
    }

    /// Replaces the whole dictionary (the page sends it like that).
    pub fn set_vocabulary(&mut self, words: &[String]) {
        self.dictionary = Dictionary::from_words(words, &crate::profile::current().dictionary);
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, toml::to_string(self)?)?;
        Ok(())
    }

    /// Parsed triggers; an error names the bad one.
    pub fn parsed(&self) -> Result<Vec<Trigger>> {
        self.triggers
            .iter()
            .map(|t| {
                let mode: Mode = t.mode.parse()?;
                parse_trigger(&t.key, mode)
            })
            .collect()
    }

    /// The shortcuts for reading aloud and talking it over, in the order the tap gets them after the
    /// dictation triggers. A bad or empty key leaves its shortcut out.
    pub fn voice_triggers(&self) -> Vec<(VoiceAction, Trigger)> {
        [
            (VoiceAction::Read, &self.read_key),
            (VoiceAction::Discuss, &self.discuss_key),
        ]
        .into_iter()
        .filter(|(_, key)| !key.trim().is_empty())
        .filter_map(|(action, key)| match parse_trigger(key, Mode::Toggle) {
            Ok(trigger) => Some((action, trigger)),
            Err(e) => {
                eprintln!("{e:#}");
                None
            }
        })
        .collect()
    }

    /// Everything the keyboard tap watches: the dictation triggers, then `voice_triggers`.
    pub fn tap_triggers(&self) -> Result<Vec<Trigger>> {
        let mut all = self.parsed()?;
        all.extend(self.voice_triggers().into_iter().map(|(_, t)| t));
        Ok(all)
    }

    /// The decoder bonus for dictionary words, `None` = off. `DICTATE_BOOST=<weight>` overrides
    /// the settings for testing (0 = off).
    pub fn boost(&self) -> Option<f32> {
        let weight = match std::env::var("DICTATE_BOOST")
            .ok()
            .and_then(|w| w.parse().ok())
        {
            Some(weight) => weight,
            None if self.boost_vocabulary => self.boost_weight,
            None => 0.0,
        };
        (weight > 0.0).then_some(weight)
    }

    /// "fn, shift+alt+cmd+5 (hybrid)" for the menu.
    pub fn describe(&self) -> String {
        let keys: Vec<String> = self
            .triggers
            .iter()
            .map(|t| format!("{} ({})", t.key, t.mode))
            .collect();
        keys.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_and_parse() {
        let config = Config::default();
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("[[trigger]]"), "{text}");
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
        assert_eq!(config.parsed().unwrap().len(), 2);
    }

    #[test]
    fn empty_file_means_defaults_and_bad_keys_are_reported() {
        assert_eq!(toml::from_str::<Config>("").unwrap(), Config::default());
        let bad = Config {
            sounds: default_sounds(),
            mute_while_recording: false,
            pause_media: true,
            triggers: vec![TriggerConfig {
                key: "hyper+x".into(),
                mode: "hold".into(),
                format: false,
                correct: false,
            }],
            ..Config::default()
        };
        assert!(format!("{:#}", bad.parsed().unwrap_err()).contains("hyper"));
    }

    #[test]
    fn old_files_without_sound_settings_still_load() {
        let old = "[[trigger]]\nkey = \"fn\"\nmode = \"hold\"\n";
        let config: Config = toml::from_str(old).unwrap();
        assert_eq!(config.sounds, "leise");
        assert!(!config.mute_while_recording);
        assert!(config.pause_media);
        assert_eq!(config.model, crate::asr::DEFAULT_MODEL);
        assert_eq!(config.vocabulary(), crate::profile::current().dictionary);
        assert!(config.boost_vocabulary);
        assert_eq!(config.triggers.len(), 1);
    }

    /// A team's real word list (the one eval/profile.toml holds), whatever profile the tests run with.
    fn ours() -> Vec<String> {
        crate::vocab::tests::WORDS
            .iter()
            .map(|w| w.to_string())
            .collect()
    }

    #[test]
    fn new_default_words_reach_a_saved_dictionary() {
        let mut config = Config::default();
        let mut words = config.vocabulary();
        words.retain(|w| w != "Commit");
        words.push("Merge Request".into());
        config.set_vocabulary(&words);
        let text = toml::to_string(&config).unwrap();
        assert!(!text.contains("GitHub"), "defaults are not stored: {text}");
        assert!(text.contains("[dictionary]"), "{text}");
        let back = Config::parse(&text).unwrap();
        assert_eq!(back, config);
        assert_eq!(back.vocabulary(), words);
        // Whatever the profile's words are now, a file without changes has all of them.
        let unchanged = Config::parse("[dictionary]\nadded = []\nremoved = []\n").unwrap();
        assert_eq!(unchanged.vocabulary(), crate::profile::current().dictionary);
        // And a word the profile gains later shows up in a saved dictionary.
        let mut later = ours();
        later.push("Souffleur".into());
        let saved = Dictionary::from_words(&["SOLISTIQ".into(), "Merge Request".into()], &ours());
        assert!(saved.words(&later).contains(&"Souffleur".to_string()));
        assert!(!saved.words(&later).contains(&"Swarm".to_string()));
    }

    #[test]
    fn old_whole_lists_become_the_difference() {
        // "Kubernetes" deleted, one own word.
        let old = ["SOLISTIQ", "Swarm", "Merge Request"].map(String::from);
        let dictionary = Dictionary::from_old_list(&old, &ours());
        assert_eq!(dictionary.added, ["Merge Request"]);
        assert!(dictionary.removed.contains(&"Kubernetes".to_string()));
        let words = dictionary.words(&ours());
        assert!(words.contains(&"Swarm".to_string()));
        assert!(!words.contains(&"Kubernetes".to_string()));
        assert_eq!(words.last().unwrap(), "Merge Request");
        // Read from a file, then the next save writes the new form only.
        let config = Config::parse("vocabulary = [\"Merge Request\"]\n").unwrap();
        assert_eq!(config.dictionary.added, ["Merge Request"]);
        let text = toml::to_string(&config).unwrap();
        assert!(!text.contains("\nvocabulary ="), "{text}");
        assert_eq!(Config::parse(&text).unwrap(), config);
    }

    #[test]
    fn duplicates_and_own_copies_of_defaults_are_not_stored() {
        let mut words = ours();
        words.push("Merge Request".into());
        words.push("Merge Request".into());
        words.push("SOLISTIQ".into());
        let dictionary = Dictionary::from_words(&words, &ours());
        assert_eq!(dictionary.added, ["Merge Request"]);
        assert!(dictionary.removed.is_empty());
    }
}
