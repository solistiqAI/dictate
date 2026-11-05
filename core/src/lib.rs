//! Dictate's platform-free part: everything between the samples and the text that does not need
//! macOS. Speech recognition on the CPU (Parakeet via ONNX Runtime; the Neural Engine build is
//! the app's and plugs in through [`engine::register_ane`]), voice activity trimming, the
//! dictionary and replacements, spoken commands and the layout check, learned corrections, the
//! settings file, usage statistics and the anonymous report, and the WER eval.
//!
//! The app (`dictate`, the crate one folder up) re-exports these modules under the same names, so
//! `crate::vocab::…` reads the same there. This crate builds and tests on Linux (CI, agents, machines
//! without a Mac): `cargo test -p dictate-core`, and `cargo run --release -p dictate-core --
//! eval …` for the eval on pre-rendered clips (AGENTS.md).

pub mod asr;
pub mod audio;
pub mod boost;
pub mod commands;
pub mod config;
pub mod engine;
pub mod eval;
pub mod format;
pub mod hook;
pub mod learn;
pub mod linux;
pub mod locate;
pub mod onnxruntime;
pub mod profile;
pub mod replace;
pub mod report;
pub mod stats;
pub mod text;
pub mod trigger;
pub mod vad;
pub mod vocab;
