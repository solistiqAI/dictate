//! `dictate-core`: the subcommands of `dictate` that need no Mac, for Linux machines (CI, agents)
//! and checks without the app. They do what `dictate eval`, `dictate transcribe` and `dictate model`
//! do, with the CPU build of Parakeet Ultra as the default model (the Neural Engine is the app's).
//!
//!   cargo run --release -p dictate-core -- model download parakeet-ultra
//!   cargo run --release -p dictate-core -- eval list.tsv --baseline eval/baseline-cpu.json
//!   scripts/eval-clips.sh   # the same on the pre-rendered clips in eval/clips/

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use dictate_core::{asr, config, eval, linux, profile, text};

/// The CPU build: what runs on every machine.
const CPU_MODEL: &str = "parakeet-ultra";

#[derive(Parser)]
#[command(
    version,
    about = "Dictate's platform-free subcommands (eval, transcribe, models) without the Mac app"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Measure recognition on clips with known text (`scripts/eval-clips.sh` makes the list).
    Eval {
        /// Lines of `wav-or-flac<TAB>dict<TAB>reference text`.
        list: PathBuf,
        #[arg(long, default_value = CPU_MODEL)]
        model: String,
        /// Write the numbers here as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Compare with such a file; fails when WER or the dictionary words got clearly worse.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// The profile whose dictionary is used (the scripts pass eval/profile.toml, the words
        /// eval/sentences.tsv is written for); default the installed one.
        #[arg(long)]
        profile: Option<PathBuf>,
    },
    /// Transcribe a WAV or FLAC file as live dictation would (dictionary, trimming, replacements).
    Transcribe {
        file: PathBuf,
        #[arg(long, default_value = CPU_MODEL)]
        model: String,
        /// Spoken language for models that take one ("de", "en", "auto"); default from config.toml.
        #[arg(long)]
        language: Option<String>,
    },
    /// Check a profile file (TOML, `core/src/profile.rs`) and print what it says.
    Profile { file: PathBuf },
    /// Manage the speech models.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
}

#[derive(Subcommand)]
enum ModelCommand {
    /// Show the known speech models and where they live.
    List,
    /// Download a speech model (the CPU ones; parakeet-ultra is 2.5 GB), or `onnxruntime` (Linux:
    /// ONNX Runtime into the data folder, found there without ORT_DYLIB_PATH).
    Download {
        #[arg(default_value = CPU_MODEL)]
        name: String,
    },
}

fn main() -> Result<()> {
    profile::init();
    // Messages from `text.rs` (e.g. ONNX Runtime missing) in the app's language: the settings'
    // `ui_language`, "auto" = the environment's (`LANG`, …), as `dictate` does.
    let ui_language = config::Config::load(&config::Config::path())
        .unwrap_or_default()
        .ui_language;
    let preferred = linux::preferred_languages(|name| std::env::var(name).ok());
    text::set(text::Lang::resolve(&ui_language, &preferred));
    match Cli::parse().command {
        Command::Eval {
            list,
            model,
            json,
            baseline,
            profile: chosen,
        } => {
            if let Some(path) = chosen {
                profile::use_file(&path)?;
            }
            eval::run(&list, &model, json.as_deref(), baseline.as_deref())
        }
        Command::Transcribe {
            file,
            model,
            language,
        } => {
            println!("{}", asr::transcribe_file(&file, &model, language)?);
            Ok(())
        }
        Command::Profile { file } => {
            let parsed = profile::load(&file)?
                .ok_or_else(|| anyhow::anyhow!("no profile at {}", file.display()))?;
            print!(
                "{}",
                profile::describe_from(&parsed, &file.display().to_string())
            );
            Ok(())
        }
        Command::Model {
            command: ModelCommand::List,
        } => {
            asr::print_models();
            Ok(())
        }
        Command::Model {
            command: ModelCommand::Download { name },
        } => {
            let dir = asr::download(&name)?;
            println!("{name} ready in {}", dir.display());
            Ok(())
        }
    }
}
