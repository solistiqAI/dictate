//! Speech recognition with Parakeet (ONNX via parakeet-rs) and the model files it needs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use parakeet_rs::ExecutionConfig;
use parakeet_rs::ort::environment::Environment;

use crate::audio::MODEL_RATE;
use crate::config::Config;
use crate::engine::{Engine, Family};
use crate::replace::Corrector;

/// A model we know how to fetch. Several families (`engine.rs`); the folder's files say which.
pub struct Model {
    pub name: &'static str,
    pub about: &'static str,
    /// For the settings window.
    pub label: &'static str,
    pub family: Family,
    /// Hugging Face folder the files come from.
    pub url: &'static str,
    /// Paths below `url`; stored flat in the model folder (by file name).
    pub files: &'static [&'static str],
}

const FP32: &[&str] = &[
    "vocab.txt",
    "decoder_joint-model.onnx",
    "encoder-model.onnx",
    "encoder-model.onnx.data",
];
const INT8: &[&str] = &[
    "vocab.txt",
    "int8/decoder_joint-model.int8.onnx",
    "int8/encoder-model.int8.onnx",
];

/// Measured on 450 German TTS clips against the CPU build (README): more dictionary words, a
/// twelfth of the memory, half the CPU time.
#[cfg(target_os = "macos")]
pub const DEFAULT_MODEL: &str = "parakeet-ultra-coreml";
/// Windows and Linux have no Neural Engine: the ONNX build (CPU, or the GPU with the `cuda` /
/// `directml` features).
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_MODEL: &str = "parakeet-ultra";

pub const MODELS: &[Model] = &[
    Model {
        name: "parakeet-ultra-coreml",
        label: "Parakeet Ultra · Neural Engine (empfohlen, sparsam, 0,6 GB)",
        about: "Parakeet Ultra as FluidAudio's Core ML build (int8 encoder) on the Neural Engine, 0.6 GB (default)",
        family: Family::Ane,
        // Fetched by the helper (FluidAudio's downloader from FluidInference/parakeet-ultra-coreml).
        url: "",
        files: &[
            "Preprocessor.mlmodelc",
            "Encoder.mlmodelc",
            "Decoder.mlmodelc",
            "JointDecisionv3.mlmodelc",
            "parakeet_vocab.json",
        ],
    },
    Model {
        name: "parakeet-ultra",
        label: "Parakeet Ultra · CPU (2,5 GB)",
        about: "Moondream's post-trained Parakeet V3, fp32 on the CPU (ONNX Runtime), 2.5 GB",
        family: Family::Tdt,
        url: "https://huggingface.co/altunenes/parakeet-rs/resolve/main/parakeet-ultra",
        files: FP32,
    },
    Model {
        name: "parakeet-ultra-int8",
        label: "Parakeet Ultra int8 · CPU (0,7 GB)",
        about: "Parakeet Ultra, int8, 670 MB download, half the memory, a bit less accurate",
        family: Family::Tdt,
        // 8-bit MatMulNBits export with <unk> banned; closer to fp32 on our recordings than the
        // dynamic-quantization int8 of Masterx/parakeet-tdt-0.6b-ultra-onnx.
        url: "https://huggingface.co/Olicorne/parakeet-tdt-0.6b-v3-ultra-onnx/resolve/main",
        files: INT8,
    },
    Model {
        name: "parakeet-tdt-0.6b-v3",
        label: "NVIDIA Parakeet V3 (2,5 GB)",
        about: "NVIDIA Parakeet V3, fp32, 2.5 GB",
        family: Family::Tdt,
        url: "https://huggingface.co/altunenes/parakeet-rs/resolve/main/parakeet-tdt-0.6b-v3",
        files: FP32,
    },
    Model {
        name: "nemotron-3.5-streaming",
        label: "Nemotron 3.5 Streaming (40 Sprachen, 2,6 GB)",
        about: "NVIDIA Nemotron 3.5 ASR streaming, 40 languages, 2.6 GB (no decoder dictionary)",
        family: Family::Nemotron,
        url: "https://huggingface.co/altunenes/parakeet-rs/resolve/main/nemotron-3.5-asr-streaming-0.6b-onnx",
        files: &[
            "config.json",
            "decoder_joint.onnx",
            "encoder.onnx",
            "encoder.onnx.data",
            "tokenizer.model",
        ],
    },
    Model {
        name: "nemotron-unified",
        label: "Nemotron Unified (25 Sprachen, 2,6 GB)",
        about: "NVIDIA Nemotron Labs Unified ASR, 25 languages, 2.6 GB (no decoder dictionary)",
        family: Family::Unified,
        url: "https://huggingface.co/altunenes/parakeet-rs/resolve/main/nemotron-labs-unified-asr-0.6b",
        files: &[
            "decoder_joint.onnx",
            "encoder.onnx",
            "encoder.onnx.data",
            "tokenizer.model",
        ],
    },
    Model {
        name: "cohere-transcribe-int8",
        label: "Cohere Transcribe (14 Sprachen, 3,1 GB)",
        about: "Cohere Transcribe 2B, int8, 14 languages (set `language`, default de), 3.1 GB \
                (no decoder dictionary)",
        family: Family::Cohere,
        url: "https://huggingface.co/onnx-community/cohere-transcribe-03-2026-ONNX/resolve/main",
        files: &[
            "tokenizer.json",
            "onnx/decoder_model_merged_quantized.onnx",
            "onnx/decoder_model_merged_quantized.onnx_data",
            "onnx/encoder_model_quantized.onnx",
            "onnx/encoder_model_quantized.onnx_data",
            "onnx/encoder_model_quantized.onnx_data_1",
        ],
    },
    Model {
        name: "parakeet-ctc-0.6b",
        label: "Parakeet CTC (nur Englisch, 2,5 GB)",
        about: "NVIDIA Parakeet CTC 0.6B, English only, 2.5 GB (no decoder dictionary)",
        family: Family::Ctc,
        url: "https://huggingface.co/altunenes/parakeet-rs/resolve/main/parakeet-ctc-0.6b",
        files: &["model.onnx", "model.onnx_data", "tokenizer.json"],
    },
];

pub fn find(name: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.name == name)
}

/// The model runs on this platform: the Neural Engine build only on macOS.
pub fn runs_here(model: &Model) -> bool {
    runs_on(model, cfg!(target_os = "macos"))
}

fn runs_on(model: &Model, macos: bool) -> bool {
    macos || model.family != Family::Ane
}

/// The models this platform can run, for `model list` and the settings: [`MODELS`] without the
/// Neural Engine build off macOS.
pub fn available() -> impl Iterator<Item = &'static Model> {
    MODELS.iter().filter(|m| runs_here(m))
}

/// Where a model file is stored: its file name, flat in the model folder.
fn local_name(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}

/// The model files are all in `dir`.
pub fn installed(model: &Model, dir: &Path) -> bool {
    model.files.iter().all(|f| dir.join(local_name(f)).exists())
}

/// Which known model to load: the one asked for when it is installed, else the default, else
/// the first installed one in [`MODELS`] (a Mac that only has the CPU build keeps working), else
/// the one asked for (loading then names the missing files). The second value says why it fell
/// back, for the log.
pub fn choose(wanted: &str, is_installed: impl Fn(&str) -> bool) -> (&str, Option<String>) {
    if is_installed(wanted) {
        return (wanted, None);
    }
    let fallback = std::iter::once(DEFAULT_MODEL)
        .chain(MODELS.iter().map(|m| m.name))
        .find(|name| *name != wanted && is_installed(name));
    match fallback {
        Some(name) => (
            name,
            Some(format!(
                "model {wanted} is not downloaded (`dictate model download {wanted}`), using {name}"
            )),
        ),
        None => (wanted, None),
    }
}

/// The folder to load for `model`: a directory as it is, a known name via [`choose`] (with the
/// note why it fell back).
pub fn resolve(model: &str) -> (PathBuf, Option<String>) {
    let path = PathBuf::from(model);
    if path.is_dir() {
        return (path, None);
    }
    let (name, note) = choose(model, |name| {
        find(name).is_some_and(|m| installed(m, &model_dir(name)))
    });
    (model_dir(name), note)
}

/// [`resolve`], saying on stderr why it fell back.
pub fn model_path(model: &str) -> PathBuf {
    let (dir, note) = resolve(model);
    if let Some(note) = note {
        eprintln!("{note}");
    }
    dir
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dictate")
}

pub fn model_dir(name: &str) -> PathBuf {
    data_dir().join("models").join(name)
}

/// `dictate model list`, the speech models: name, installed or not, what it is, where.
pub fn print_models() {
    for model in available() {
        let dir = model_dir(model.name);
        let state = if installed(model, &dir) {
            "installed"
        } else {
            "-"
        };
        println!(
            "{:22} {state:10} {}\n{:33}{} · {}",
            model.name,
            model.about,
            "",
            model.family,
            dir.display()
        );
    }
    // The Linux build loads ONNX Runtime at run time: where from, or how to get it.
    if cfg!(target_os = "linux") {
        let (state, path) = match crate::onnxruntime::resolve() {
            Some(path) => ("installed", path),
            None => ("-", crate::onnxruntime::installed_path()),
        };
        println!(
            "{:22} {state:10} ONNX Runtime {}, loaded at run time (ORT_DYLIB_PATH overrides), 11 MB\n{:33}runtime · {}",
            crate::onnxruntime::NAME,
            crate::onnxruntime::VERSION,
            "",
            path.display()
        );
    }
}

/// Downloads a model with curl (resumes partial files). Big: 0.7 to 3.1 GB. `onnxruntime` is
/// ONNX Runtime for the Linux build (`onnxruntime.rs`).
pub fn download(name: &str) -> Result<PathBuf> {
    if name == crate::onnxruntime::NAME {
        return crate::onnxruntime::download();
    }
    let Some(model) = find(name) else {
        bail!(
            "unknown model {name:?}; known: {}",
            MODELS.iter().map(|m| m.name).collect::<Vec<_>>().join(", ")
        );
    };
    if !runs_here(model) {
        bail!(
            "{name} runs on the Neural Engine, which only Macs have; take {} instead",
            MODELS
                .iter()
                .find(|m| runs_here(m))
                .map_or("another model", |m| m.name)
        );
    }
    let dir = model_dir(name);
    if model.family == Family::Ane {
        (crate::engine::ane()?.download)(&dir)?;
        return Ok(dir);
    }
    std::fs::create_dir_all(&dir)?;
    for path in model.files {
        let file = local_name(path);
        let target = dir.join(file);
        if target.exists() {
            continue;
        }
        eprintln!("downloading {name}/{file}");
        fetch(&format!("{}/{path}", model.url), &target)?;
    }
    Ok(dir)
}

/// Downloads `url` to `target` with curl and a progress bar: into `<target>.part` first (resumed
/// when it is there) and renamed when complete, so a present file is whole.
pub(crate) fn fetch(url: &str, target: &Path) -> Result<()> {
    let mut part = target.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let status = Command::new("curl")
        .args(["-fL", "--progress-bar", "-C", "-", "-o"])
        .arg(&part)
        .arg(url)
        .status()
        .context("run curl")?;
    if !status.success() {
        bail!("download of {url} failed ({status}); run it again to resume");
    }
    std::fs::rename(&part, target)?;
    Ok(())
}

pub struct Asr {
    engine: Box<dyn Engine>,
    pub family: Family,
}

/// The GPU when the build has one (features `cuda`, `directml`), unless `DICTATE_CPU=1` asks for
/// the CPU (to compare). ONNX Runtime falls back to the CPU for what the GPU cannot run.
fn on_gpu(config: ExecutionConfig) -> ExecutionConfig {
    if std::env::var_os("DICTATE_CPU").is_some_and(|v| v == "1") {
        return config;
    }
    #[cfg(feature = "cuda")]
    return config.with_execution_provider(parakeet_rs::ExecutionProvider::Cuda);
    #[cfg(all(feature = "directml", not(feature = "cuda")))]
    return config.with_execution_provider(parakeet_rs::ExecutionProvider::DirectML);
    #[allow(unreachable_code)]
    config
}

impl Asr {
    /// Loads whatever model family `dir` holds (`engine.rs`).
    pub fn load(dir: &Path) -> Result<Self> {
        let Some(family) = Family::of_dir(dir) else {
            bail!(
                "no model in {}; run `dictate model download`",
                dir.display()
            );
        };
        let started = Instant::now();
        let config = ExecutionConfig::new().with_environment(environment()?);
        let engine = family.load(dir, on_gpu(config))?;
        eprintln!(
            "{family} model loaded in {:.1} s",
            started.elapsed().as_secs_f32()
        );
        Ok(Asr { engine, family })
    }

    /// Favour `words` while decoding with this bonus per token (`boost.rs`); `None` or 0 = off.
    pub fn set_boost(&mut self, words: &[String], weight: Option<f32>) {
        if !self.engine.set_boost(words, weight) && weight.is_some() {
            eprintln!(
                "dictionary in the decoder: not available for {}, the text dictionary still applies",
                self.family
            );
        }
    }

    /// The spoken language for models that take one ("de", "en", "auto"); the others detect it.
    pub fn set_language(&mut self, language: &str) {
        match self.engine.set_language(language) {
            Ok(true) => eprintln!("language: {language}"),
            Ok(false) => {}
            Err(e) => eprintln!("language {language:?}: {e:#}"),
        }
    }

    /// `samples` must be 16 kHz mono.
    pub fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        // Parakeet hallucinates on near-empty input; a quarter second of audio is the floor.
        if samples.len() < MODEL_RATE as usize / 4 {
            return Ok(String::new());
        }
        Ok(self.engine.transcribe(samples)?.trim().to_string())
    }

    /// [`Asr::transcribe`] for the live text: the Neural Engine skips its dictionary check.
    pub fn transcribe_live(&mut self, samples: Vec<f32>) -> Result<String> {
        if samples.len() < MODEL_RATE as usize / 4 {
            return Ok(String::new());
        }
        Ok(self.engine.transcribe_live(samples)?.trim().to_string())
    }
}

/// `dictate transcribe`: a WAV file through what live dictation does after the microphone (the
/// settings' dictionary and boost, trimming, replacements, spoken commands). Notes on stderr.
pub fn transcribe_file(file: &Path, model: &str, language: Option<String>) -> Result<String> {
    let mut asr = Asr::load(&model_path(model))?;
    // Decoder boost as in the app (`DICTATE_BOOST=0` compares without it).
    let config = Config::load(&Config::path())?;
    asr.set_boost(&config.vocabulary(), config.boost());
    asr.set_language(&language.unwrap_or(config.language));
    let clip = crate::audio::read(file)?;
    let seconds = clip.seconds();
    let started = Instant::now();
    let mut samples = clip.into_model_rate()?;
    if crate::vad::enabled() {
        let trimmed = crate::vad::trim(samples);
        eprintln!("{}", trimmed.describe());
        samples = trimmed.samples;
    }
    let text = asr.transcribe(samples)?;
    // The same corrections as live dictation: dictionary from the settings, then replacements.
    let config = Config::load(&Config::path()).unwrap_or_default();
    let text = Corrector::load(&crate::replace::default_path(), &config.vocabulary())
        .with_commands(config.voice_commands)
        .finish(&text);
    eprintln!(
        "{seconds:.1} s of audio in {:.2} s",
        started.elapsed().as_secs_f32()
    );
    Ok(text)
}

/// ONNX Runtime allows one environment per process; a model switch reuses it.
fn environment() -> Result<Environment> {
    static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();
    if let Some(env) = ENVIRONMENT.get() {
        return Ok(env.clone());
    }
    // The Linux and Android builds load the library themselves (`load-dynamic`): find it first.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let builder = crate::onnxruntime::init()?;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let builder = parakeet_rs::ort::init();
    let env = builder.build().context("start ONNX Runtime")?;
    Ok(ENVIRONMENT.get_or_init(|| env).clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_falls_back_to_an_installed_model() {
        let only_default = |name: &str| name == DEFAULT_MODEL;
        let (name, note) = choose("parakeet-ultra-int8", only_default);
        assert_eq!(name, DEFAULT_MODEL);
        assert!(
            note.unwrap()
                .contains("dictate model download parakeet-ultra-int8")
        );
        assert_eq!(
            choose("parakeet-ultra-int8", |_| true),
            ("parakeet-ultra-int8", None)
        );
        // The default missing too: the first installed one (a Mac with only the CPU build).
        let only_cpu = |name: &str| name == "parakeet-ultra";
        assert_eq!(choose(DEFAULT_MODEL, only_cpu).0, "parakeet-ultra");
        assert_eq!(
            choose("cohere-transcribe-int8", only_cpu).0,
            "parakeet-ultra"
        );
        // Nothing installed: keep the wish, loading says what is missing.
        assert_eq!(
            choose("parakeet-ultra-int8", |_| false),
            ("parakeet-ultra-int8", None)
        );
        assert_eq!(choose(DEFAULT_MODEL, |_| false), (DEFAULT_MODEL, None));
    }

    #[test]
    fn the_neural_engine_model_only_on_macos() {
        let ane = find("parakeet-ultra-coreml").unwrap();
        assert!(runs_on(ane, true));
        assert!(!runs_on(ane, false));
        let cpu = find("parakeet-ultra").unwrap();
        assert!(runs_on(cpu, true) && runs_on(cpu, false));
        // Every family but the Neural Engine's is offered everywhere.
        let listed: Vec<&str> = available().map(|m| m.name).collect();
        for model in MODELS {
            assert_eq!(
                listed.contains(&model.name),
                cfg!(target_os = "macos") || model.family != Family::Ane,
                "{}",
                model.name
            );
        }
        // The default is one this platform runs.
        assert!(runs_here(find(DEFAULT_MODEL).unwrap()));
    }

    #[test]
    fn downloading_the_neural_engine_model_off_macos_says_why() {
        if cfg!(target_os = "macos") {
            return;
        }
        let error = download("parakeet-ultra-coreml").unwrap_err().to_string();
        assert!(error.contains("only Macs"), "{error}");
        assert!(error.contains("parakeet-ultra instead"), "{error}");
    }

    #[test]
    fn known_models_are_found_as_their_family_once_downloaded() {
        assert!(find(DEFAULT_MODEL).is_some());
        for model in MODELS {
            let names: Vec<&str> = model.files.iter().map(|f| local_name(f)).collect();
            assert_eq!(Family::detect(&names), Some(model.family), "{}", model.name);
        }
    }
}
