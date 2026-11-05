//! The model families dictate can run, one engine each behind [`Engine`]: 16 kHz mono in, text
//! out. What goes beyond that is optional per family: the dictionary in the decoder (`boost.rs`,
//! TDT only so far) and a language for models that take one. The text dictionary, replacements
//! and everything after the transcript work the same for every engine.
//!
//! All but one come from parakeet-rs on ONNX Runtime (CPU); [`Family::Ane`] is FluidAudio's Core
//! ML build on the Neural Engine, in a Swift helper process (`ane.rs` in the macOS app, which
//! plugs it in with [`register_ane`]). A new family is a new [`Family`] variant, its files in
//! [`Family::detect`] and an `Engine` impl.

use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use parakeet_rs::{
    CohereASR, ExecutionConfig, Nemotron, NemotronMode, Parakeet, ParakeetTDT, ParakeetUnified,
    Transcriber,
};

use crate::audio::MODEL_RATE;

pub trait Engine {
    /// `samples` are 16 kHz mono.
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String>;

    /// For the live text while recording, re-run every 0.6 s: may skip work that only pays off
    /// once, on the final text.
    fn transcribe_live(&mut self, samples: Vec<f32>) -> Result<String> {
        self.transcribe(samples)
    }

    /// Favour `words` while decoding, `weight` per token (`None` = off). False when the family
    /// has no hook for it.
    fn set_boost(&mut self, _words: &[String], _weight: Option<f32>) -> bool {
        false
    }

    /// The spoken language as an ISO code ("de", "en") or "auto". False when the model ignores
    /// it (it detects the language itself).
    fn set_language(&mut self, _language: &str) -> Result<bool> {
        Ok(false)
    }
}

/// How the app runs [`Family::Ane`]: the Swift helper is macOS-only, so it lives in the app crate
/// and hands these in at start ([`register_ane`]).
pub struct AneHooks {
    pub load: fn(&Path) -> Result<Box<dyn Engine>>,
    /// Fetches the Core ML model into the folder.
    pub download: fn(&Path) -> Result<()>,
}

static ANE: OnceLock<AneHooks> = OnceLock::new();

/// The app calls this once at start; later calls are ignored.
pub fn register_ane(hooks: AneHooks) {
    let _ = ANE.set(hooks);
}

/// The Neural Engine family, when the app registered it.
pub fn ane() -> Result<&'static AneHooks> {
    ANE.get().context(
        "Parakeet on the Neural Engine needs the macOS app (dictate-ane); \
         on this machine use a CPU model such as parakeet-ultra",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Parakeet TDT V3 and its fine tunes (Ultra): 25 languages, detected per recording.
    Tdt,
    /// Parakeet CTC: English only.
    Ctc,
    /// Nemotron streaming (English, or 3.5 with 40 languages).
    Nemotron,
    /// Parakeet/Nemotron Unified, offline and streaming (English, or 25 languages).
    Unified,
    /// Cohere Transcribe: 14 languages, needs to be told which.
    Cohere,
    /// Parakeet (Ultra) as FluidAudio's Core ML build, on the Neural Engine (`ane.rs`).
    Ane,
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Family::Tdt => "Parakeet TDT",
            Family::Ctc => "Parakeet CTC",
            Family::Nemotron => "Nemotron",
            Family::Unified => "Unified",
            Family::Cohere => "Cohere Transcribe",
            Family::Ane => "Parakeet (Neural Engine)",
        })
    }
}

impl Family {
    /// Which family a model folder holds, from its file names (flat, or Cohere's `onnx/`).
    pub fn detect<S: AsRef<str>>(files: &[S]) -> Option<Family> {
        let has = |test: &dyn Fn(&str) -> bool| files.iter().any(|f| test(f.as_ref()));
        let named = |name: &str| has(&|f| f == name);
        let onnx = |prefix: &str| has(&|f| f.starts_with(prefix) && f.ends_with(".onnx"));
        if named("Encoder.mlmodelc") && named("parakeet_vocab.json") {
            Some(Family::Ane)
        } else if onnx("encoder_model") || onnx("onnx/encoder_model") {
            Some(Family::Cohere)
        } else if named("tokenizer.model") && named("encoder.onnx") {
            // Nemotron's export carries its streaming setup in config.json; Unified has none.
            Some(match named("config.json") {
                true => Family::Nemotron,
                false => Family::Unified,
            })
        } else if named("vocab.txt") && onnx("encoder-model") {
            Some(Family::Tdt)
        } else if named("tokenizer.json") && onnx("model") {
            Some(Family::Ctc)
        } else {
            None
        }
    }

    /// [`Family::detect`] on what is in `dir`.
    pub fn of_dir(dir: &Path) -> Option<Family> {
        let mut files = Vec::new();
        for sub in ["", "onnx"] {
            let Ok(entries) = std::fs::read_dir(dir.join(sub)) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                files.push(match sub {
                    "" => name,
                    _ => format!("{sub}/{name}"),
                });
            }
        }
        Family::detect(&files)
    }

    pub fn load(self, dir: &Path, config: ExecutionConfig) -> Result<Box<dyn Engine>> {
        let context = || format!("load {self} model from {}", dir.display());
        Ok(match self {
            Family::Tdt => Box::new(Tdt::load(dir, config).with_context(context)?),
            Family::Ctc => {
                Box::new(Parakeet::from_pretrained(dir, Some(config)).with_context(context)?)
            }
            Family::Nemotron => {
                Box::new(Nemotron::from_pretrained(dir, Some(config)).with_context(context)?)
            }
            Family::Unified => {
                Box::new(ParakeetUnified::from_pretrained(dir, Some(config)).with_context(context)?)
            }
            Family::Cohere => Box::new(Cohere {
                model: CohereASR::from_pretrained(dir, Some(config)).with_context(context)?,
                language: COHERE_DEFAULT_LANGUAGE.into(),
            }),
            Family::Ane => (ane()?.load)(dir).with_context(context)?,
        })
    }
}

struct Tdt {
    model: ParakeetTDT,
    /// `vocab.txt`, to spell dictionary words in tokens (`boost.rs`).
    vocab: String,
}

impl Tdt {
    fn load(dir: &Path, config: ExecutionConfig) -> Result<Self> {
        let mut model = ParakeetTDT::from_pretrained(dir, Some(config))?;
        // Short or quiet German is often taken for Russian and comes out in Cyrillic; we only
        // dictate German and English, so Latin script it is (DICTATE_ALL_SCRIPTS=1 turns it off).
        let vocab = std::fs::read_to_string(dir.join("vocab.txt"))?;
        if std::env::var_os("DICTATE_ALL_SCRIPTS").is_none() {
            model.set_allowed_tokens(Some(latin_only(&vocab)));
        }
        Ok(Tdt { model, vocab })
    }
}

impl Engine for Tdt {
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        Ok(self
            .model
            .transcribe_samples(samples, MODEL_RATE, 1, None)?
            .text)
    }

    fn set_boost(&mut self, words: &[String], weight: Option<f32>) -> bool {
        let boost = weight
            .filter(|w| *w > 0.0)
            .map(|w| crate::boost::build(words, &self.vocab, w));
        self.model.set_boost(boost);
        true
    }
}

impl Engine for Parakeet {
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        Ok(self.transcribe_samples(samples, MODEL_RATE, 1, None)?.text)
    }
}

impl Engine for Nemotron {
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        Ok(self.transcribe_audio(&samples)?)
    }

    fn set_language(&mut self, language: &str) -> Result<bool> {
        if self.mode() == NemotronMode::EnglishOnly {
            return Ok(false);
        }
        self.set_target_lang(language)?;
        Ok(true)
    }
}

impl Engine for ParakeetUnified {
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        Ok(self.transcribe_samples(samples, MODEL_RATE, 1, None)?.text)
    }

    fn set_language(&mut self, language: &str) -> Result<bool> {
        // The English model has no language prompt and says so; that is not a problem.
        Ok(self.set_target_lang(language).is_ok())
    }
}

/// Cohere Transcribe cannot detect the language; "auto" means this one.
const COHERE_DEFAULT_LANGUAGE: &str = "de";

/// Cohere was trained on clips up to 35 s and leaves longer audio to the caller.
const COHERE_MAX_SECONDS: usize = 30;

struct Cohere {
    model: CohereASR,
    language: String,
}

impl Engine for Cohere {
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        let mut text = Vec::new();
        for piece in pieces(&samples, COHERE_MAX_SECONDS * MODEL_RATE as usize) {
            let part = self
                .model
                .transcribe_audio(piece, &self.language, true, true)?;
            text.push(part.trim().to_string());
        }
        Ok(text.join(" "))
    }

    fn set_language(&mut self, language: &str) -> Result<bool> {
        self.language = match language {
            "auto" => COHERE_DEFAULT_LANGUAGE.into(),
            other => other.into(),
        };
        Ok(true)
    }
}

/// `samples` cut into pieces of at most `max` samples, each cut in the quietest 100 ms of the
/// piece's last fifth, so a word is rarely split.
fn pieces(samples: &[f32], max: usize) -> Vec<&[f32]> {
    let window = MODEL_RATE as usize / 10;
    let mut out = Vec::new();
    let mut rest = samples;
    while rest.len() > max {
        let energy = |at: usize| rest[at..at + window].iter().map(|s| s * s).sum::<f32>();
        let cut = (max - max / 5..=max - window)
            .step_by(window / 2)
            .min_by(|a, b| energy(*a).total_cmp(&energy(*b)))
            .map_or(max, |at| at + window / 2);
        out.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    out.push(rest);
    out
}

/// Per token of `vocab.txt` (`<token> <id>` per line): may it be emitted? No for tokens with
/// Cyrillic or Greek letters, the two other scripts Parakeet V3 speaks.
pub fn latin_only(vocab: &str) -> Vec<bool> {
    let foreign = |c: char| {
        matches!(c as u32,
            0x0370..=0x03FF | 0x1F00..=0x1FFF          // Greek
            | 0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F) // Cyrillic
    };
    let mut allowed = Vec::new();
    for line in vocab.lines() {
        let Some((token, id)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(id) = id.parse::<usize>() else {
            continue;
        };
        if allowed.len() <= id {
            allowed.resize(id + 1, true);
        }
        allowed[id] = !token.chars().any(foreign);
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_each_family_from_its_files() {
        let tdt = [
            "vocab.txt",
            "encoder-model.onnx",
            "encoder-model.onnx.data",
            "decoder_joint-model.onnx",
        ];
        assert_eq!(Family::detect(&tdt), Some(Family::Tdt));
        let tdt_int8 = [
            "vocab.txt",
            "encoder-model.int8.onnx",
            "decoder_joint-model.int8.onnx",
        ];
        assert_eq!(Family::detect(&tdt_int8), Some(Family::Tdt));
        let ctc = ["model.onnx", "model.onnx_data", "tokenizer.json"];
        assert_eq!(Family::detect(&ctc), Some(Family::Ctc));
        let nemotron = [
            "config.json",
            "decoder_joint.onnx",
            "encoder.onnx",
            "encoder.onnx.data",
            "tokenizer.model",
        ];
        assert_eq!(Family::detect(&nemotron), Some(Family::Nemotron));
        let unified = [
            "decoder_joint.onnx",
            "encoder.onnx",
            "encoder.onnx.data",
            "tokenizer.model",
            "vocab.txt",
        ];
        assert_eq!(Family::detect(&unified), Some(Family::Unified));
        let cohere = [
            "tokenizer.json",
            "encoder_model_quantized.onnx",
            "decoder_model_merged_quantized.onnx",
        ];
        assert_eq!(Family::detect(&cohere), Some(Family::Cohere));
        let cohere_nested = [
            "tokenizer.json",
            "onnx/encoder_model.onnx",
            "onnx/decoder_model_merged.onnx",
        ];
        assert_eq!(Family::detect(&cohere_nested), Some(Family::Cohere));
        let ane = [
            "Preprocessor.mlmodelc",
            "Encoder.mlmodelc",
            "Decoder.mlmodelc",
            "JointDecisionv3.mlmodelc",
            "parakeet_vocab.json",
        ];
        assert_eq!(Family::detect(&ane), Some(Family::Ane));
        assert_eq!(Family::detect(&["vocab.txt", "README.md"]), None);
    }

    #[test]
    fn pieces_cut_long_audio_in_a_pause() {
        let rate = MODEL_RATE as usize;
        assert_eq!(pieces(&vec![0.5; 10 * rate], 30 * rate).len(), 1);
        // 70 s of "speech" with a pause at 27 s and one at 55 s.
        let mut audio = vec![0.5; 70 * rate];
        for pause in [27, 55] {
            audio[pause * rate..pause * rate + rate / 5].fill(0.0);
        }
        let parts = pieces(&audio, 30 * rate);
        assert_eq!(parts.iter().map(|p| p.len()).sum::<usize>(), audio.len());
        assert!(parts.iter().all(|p| p.len() <= 30 * rate));
        let first = parts[0].len() as f32 / rate as f32;
        assert!((27.0..27.2).contains(&first), "cut at {first} s");
        let second = first + parts[1].len() as f32 / rate as f32;
        assert!((55.0..55.2).contains(&second), "cut at {second} s");
    }

    #[test]
    fn latin_only_blocks_cyrillic_and_greek_tokens() {
        let vocab = "<unk> 0\n▁test 1\n▁Тест 2\nä 3\nλ 4\n<blk> 5\n";
        assert_eq!(
            latin_only(vocab),
            vec![true, true, false, true, false, true]
        );
    }
}
