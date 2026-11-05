//! Dictate on Android: what the app in `android/` needs from `dictate-core`, behind JNI
//! (`jni.rs`, called from `app.dictate.Native` in Kotlin). Samples in, finished text out: the
//! same steps as the desktop's `dictate transcribe` (VAD trim, Parakeet on the CPU with the
//! dictionary in the decoder, then dictionary, replacements and spoken commands on the text).
//!
//! The app keeps its files as the desktop keeps its data folder: `config.toml` (the dictionary
//! as `[dictionary] added/removed`, the same format), optionally `replacements.txt` and
//! `profile.toml`; the model folder is wherever the app downloaded it. Nothing here talks to the system; the
//! Kotlin side records, decodes, downloads and shows.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use dictate_core::asr::{self, Asr};
use dictate_core::audio::{MODEL_RATE, resample};
use dictate_core::config::Config;
use dictate_core::replace::Corrector;
use dictate_core::{engine, profile, vad};

mod jni;

/// The model the app downloads: Parakeet Ultra int8 (670 MB); the fp32 build is too big for
/// phones.
pub const MODEL: &str = "parakeet-ultra-int8";

/// Long voice messages go through the model in pieces of at most this, cut at quiet moments: one
/// pass over minutes of audio needs more memory than a phone gives an app.
const PIECE_SECONDS: usize = 60;

/// The files the app keeps, under its data folder.
pub struct Files {
    pub data: PathBuf,
}

impl Files {
    pub fn new(data: impl Into<PathBuf>) -> Self {
        Files { data: data.into() }
    }

    pub fn config(&self) -> PathBuf {
        self.data.join("config.toml")
    }

    pub fn replacements(&self) -> PathBuf {
        self.data.join("replacements.txt")
    }

    pub fn profile(&self) -> PathBuf {
        self.data.join("profile.toml")
    }

    /// A team's `profile.toml` in the data folder (its default words), else the built-in one.
    fn use_profile(&self) {
        match profile::load(&self.profile()) {
            Ok(Some(found)) => profile::set(found),
            Ok(None) => {}
            Err(e) => eprintln!("profile: {e:#}; using the built-in one"),
        }
    }

    fn settings(&self) -> Config {
        self.use_profile();
        Config::load(&self.config()).unwrap_or_else(|e| {
            eprintln!("settings: {e:#}; using the defaults");
            Config::default()
        })
    }
}

/// The model's files: (URL, file name in the model folder), as `dictate model download` fetches
/// them.
pub fn model_files() -> Vec<(String, String)> {
    let model = asr::find(MODEL).expect("the int8 model is in the core's list");
    model
        .files
        .iter()
        .map(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            (format!("{}/{path}", model.url), name.to_string())
        })
        .collect()
}

/// All of the model's files are in `dir`.
pub fn model_installed(dir: &Path) -> bool {
    asr::find(MODEL).is_some_and(|model| asr::installed(model, dir))
}

/// The whole dictionary: the profile's default words with the user's additions and deletions.
pub fn dictionary(files: &Files) -> Vec<String> {
    files.settings().vocabulary()
}

/// Saves the whole word list as the difference to the defaults, like the desktop's window.
pub fn set_dictionary(files: &Files, words: &[String]) -> Result<()> {
    let mut config = files.settings();
    config.set_vocabulary(words);
    config.save(&files.config())
}

/// A loaded model with the settings around it.
pub struct Dictation {
    files: Files,
    asr: Asr,
    corrector: Corrector,
}

impl Dictation {
    pub fn load(model_dir: &Path, files: Files) -> Result<Self> {
        if !model_installed(model_dir) {
            bail!(
                "the speech model is not downloaded ({})",
                model_dir.display()
            );
        }
        let asr = Asr::load(model_dir)?;
        let corrector = Corrector::load(&files.replacements(), &[]);
        let mut dictation = Dictation {
            files,
            asr,
            corrector,
        };
        dictation.reload_settings();
        Ok(dictation)
    }

    /// After the dictionary or the settings changed.
    pub fn reload_settings(&mut self) {
        let config = self.files.settings();
        let words = config.vocabulary();
        self.asr.set_boost(&words, config.boost());
        self.asr.set_language(&config.language);
        self.corrector = Corrector::load(&self.files.replacements(), &words)
            .with_commands(config.voice_commands);
    }

    /// Mono `samples` at `rate` Hz (the microphone at 16 kHz, a voice message at whatever it was
    /// recorded at) to the text to insert.
    pub fn transcribe(&mut self, samples: Vec<f32>, rate: u32) -> Result<String> {
        let samples = to_model_rate(samples, rate)?;
        let samples = match vad::enabled() {
            true => vad::trim(samples).samples,
            false => samples,
        };
        let mut parts = Vec::new();
        for piece in engine::pieces(&samples, PIECE_SECONDS * MODEL_RATE as usize) {
            let part = self.asr.transcribe(piece.to_vec())?;
            if !part.is_empty() {
                parts.push(part);
            }
        }
        Ok(self.corrector.finish(&parts.join(" ")))
    }
}

fn to_model_rate(samples: Vec<f32>, rate: u32) -> Result<Vec<f32>> {
    match rate {
        0 => bail!("sample rate 0"),
        MODEL_RATE => Ok(samples),
        _ => resample(samples, rate, MODEL_RATE).with_context(|| format!("resample {rate} Hz")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dictate-android-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_model_files_are_the_int8_build_stored_flat() {
        let files = model_files();
        let names: Vec<&str> = files.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "vocab.txt",
                "decoder_joint-model.int8.onnx",
                "encoder-model.int8.onnx"
            ]
        );
        for (url, name) in &files {
            assert!(url.starts_with("https://huggingface.co/"), "{url}");
            assert!(url.ends_with(name.as_str()), "{url}");
        }
        let dir = scratch("model");
        assert!(!model_installed(&dir));
        for (_, name) in &files {
            std::fs::write(dir.join(name), "").unwrap();
        }
        assert!(model_installed(&dir));
    }

    #[test]
    fn the_dictionary_is_kept_as_the_desktop_keeps_it() {
        let files = Files::new(scratch("dictionary"));
        let defaults = dictionary(&files);
        assert!(defaults.contains(&"GitHub".to_string()), "{defaults:?}");
        let mut words: Vec<String> = defaults
            .iter()
            .filter(|w| *w != "GitHub")
            .cloned()
            .collect();
        words.push("Wombat".into());
        set_dictionary(&files, &words).unwrap();
        let saved = std::fs::read_to_string(files.config()).unwrap();
        let config = Config::parse(&saved).unwrap();
        assert_eq!(config.dictionary.added, ["Wombat"]);
        assert_eq!(config.dictionary.removed, ["GitHub"]);
        assert_eq!(dictionary(&files), words);
    }

    #[test]
    fn voice_messages_are_resampled_to_the_model_rate() {
        assert_eq!(
            to_model_rate(vec![0.5; 1600], MODEL_RATE).unwrap().len(),
            1600
        );
        let from_48k = to_model_rate(vec![0.0; 48_000], 48_000).unwrap();
        assert!(
            (from_48k.len() as i64 - 16_000).abs() < 200,
            "{}",
            from_48k.len()
        );
        assert!(to_model_rate(vec![0.0; 10], 0).is_err());
    }

    #[test]
    fn loading_without_the_model_says_so() {
        let dir = scratch("nomodel");
        let error = Dictation::load(&dir, Files::new(&dir)).err().unwrap();
        assert!(error.to_string().contains("not downloaded"), "{error}");
    }

    /// The whole path on the host: the int8 model (`dictate-core model download
    /// parakeet-ultra-int8`) and ONNX Runtime (`model download onnxruntime` or `ORT_DYLIB_PATH`).
    /// `cargo test --release -p dictate-android -- --ignored`.
    #[test]
    #[ignore = "needs the int8 model and ONNX Runtime"]
    fn transcribes_an_eval_clip_end_to_end() {
        let clip = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../eval/clips/s01-anna.flac");
        let clip = dictate_core::audio::read(&clip).unwrap();
        let rate = clip.rate;
        let model = asr::model_dir(MODEL);
        let mut dictation = Dictation::load(&model, Files::new(scratch("e2e"))).unwrap();
        let text = dictation.transcribe(clip.samples, rate).unwrap();
        assert!(text.split_whitespace().count() >= 4, "{text}");
        println!("{text}");
    }
}
