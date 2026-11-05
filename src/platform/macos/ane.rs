//! Parakeet on the Neural Engine: FluidAudio's Core ML build of the model, run by the Swift
//! helper `dictate-ane` (`ane/`, next to `dictate` in the app bundle). A child process instead of
//! linking Swift into this binary: the Rust side stays plain, a crash of the model does not take
//! the app down, and its memory shows on its own in Activity Monitor.
//!
//! Protocol (stdin/stdout, see `ane/Sources/dictate-ane/main.swift`): "transcribe <n> <lang>
//! [live]\n" plus n little-endian f32 ("live" skips the dictionary check), "vocabulary <JSON words>\n"; answers "ok <JSON string>" or
//! "error <message>". Its logs go to stderr, which is our log.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};

use crate::engine::Engine;

/// The helper: `DICTATE_ANE`, else next to this binary (the app bundle; through the
/// `~/.local/bin/dictate` link too), else the Swift build in the checkout
/// (`swift build -c release --package-path ane`).
pub fn helper() -> PathBuf {
    if let Some(path) = std::env::var_os("DICTATE_ANE") {
        return path.into();
    }
    let beside = std::env::current_exe()
        .and_then(|exe| exe.canonicalize())
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("dictate-ane")));
    match beside {
        Some(path) if path.exists() => path,
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("ane/.build/release/dictate-ane"),
    }
}

/// Plugs the Neural Engine into `dictate-core`'s engines (`Family::Ane`): once at start.
pub fn register() {
    crate::engine::register_ane(crate::engine::AneHooks {
        load: |dir| Ok(Box::new(Ane::load(dir)?)),
        download,
    });
}

/// Fetches the Core ML model into `dir` (FluidAudio's downloader; the folder name picks it).
pub fn download(dir: &Path) -> Result<()> {
    let output = Command::new(helper())
        .arg("download")
        .arg(dir)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("run {}", helper().display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.lines().last() {
        Some("done") if output.status.success() => Ok(()),
        last => bail!("download failed: {}", last.unwrap_or("no answer")),
    }
}

pub struct Ane {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    /// "de" or "en": only steers FluidAudio's script filter (no Cyrillic).
    language: String,
}

impl Ane {
    pub fn load(dir: &Path) -> Result<Self> {
        let mut child = Command::new(helper())
            .arg("serve")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start {}", helper().display()))?;
        let input = child.stdin.take().context("helper stdin")?;
        let output = BufReader::new(child.stdout.take().context("helper stdout")?);
        let mut ane = Ane {
            child,
            input,
            output,
            language: "de".into(),
        };
        let first = ane.line()?;
        match first.strip_prefix("ready ") {
            Some(seconds) => eprintln!("Neural Engine helper ready ({seconds} s)"),
            None => bail!("{}", first.strip_prefix("error ").unwrap_or(&first)),
        }
        Ok(ane)
    }

    fn line(&mut self) -> Result<String> {
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            let status = self.child.try_wait().ok().flatten();
            bail!("the Neural Engine helper ended ({status:?})");
        }
        Ok(line.trim_end().to_string())
    }

    /// Sends a request and returns the answer's text.
    fn ask(&mut self, header: &str, body: &[u8]) -> Result<String> {
        self.input.write_all(header.as_bytes())?;
        self.input.write_all(b"\n")?;
        self.input.write_all(body)?;
        self.input.flush()?;
        let answer = self.line()?;
        match answer.split_once(' ') {
            Some(("ok", json)) => Ok(serde_json::from_str(json)?),
            Some(("error", message)) => Err(anyhow!("{message}")),
            _ => bail!("unexpected answer {answer:?}"),
        }
    }
}

impl Drop for Ane {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Ane {
    fn send_audio(&mut self, samples: &[f32], live: bool) -> Result<String> {
        let body: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut header = format!("transcribe {} {}", samples.len(), self.language);
        if live {
            header.push_str(" live");
        }
        self.ask(&header, &body)
    }
}

impl Engine for Ane {
    fn transcribe(&mut self, samples: Vec<f32>) -> Result<String> {
        self.send_audio(&samples, false)
    }

    /// Without the dictionary check (85 ms and most of the CPU time per pass, measured); the
    /// final text gets it.
    fn transcribe_live(&mut self, samples: Vec<f32>) -> Result<String> {
        self.send_audio(&samples, true)
    }

    /// FluidAudio's own dictionary boosting (a CTC model rescores the transcript); the weight
    /// only switches it on or off.
    fn set_boost(&mut self, words: &[String], weight: Option<f32>) -> bool {
        let on = weight.is_some_and(|w| w > 0.0);
        let words: &[String] = if on { words } else { &[] };
        let header = format!("vocabulary {}", serde_json::json!(words));
        match self.ask(&header, &[]) {
            Ok(answer) => eprintln!("Neural Engine dictionary: {answer}"),
            Err(e) => eprintln!("Neural Engine dictionary: {e:#}"),
        }
        true
    }

    fn set_language(&mut self, language: &str) -> Result<bool> {
        self.language = match language {
            "en" => "en".into(),
            _ => "de".into(),
        };
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// The protocol against a stand-in helper (a shell script), no model needed.
    #[test]
    fn talks_to_the_helper() {
        let dir = std::env::temp_dir().join(format!("dictate-ane-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("dictate-ane");
        std::fs::write(
            &script,
            "#!/bin/sh\n\
             echo 'ready 0.1'\n\
             while read -r cmd n lang live; do\n\
               case $cmd in\n\
                 transcribe) head -c $((n * 4)) >/dev/null; echo \"ok \\\"$n Samples, $lang$live\\\"\" ;;\n\
                 vocabulary) echo 'ok \"2 terms\"' ;;\n\
                 *) echo 'error unknown request' ;;\n\
               esac\n\
             done\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Only this test reads DICTATE_ANE.
        unsafe { std::env::set_var("DICTATE_ANE", &script) };
        let mut ane = Ane::load(&dir).unwrap();
        assert_eq!(ane.transcribe(vec![0.5; 1600]).unwrap(), "1600 Samples, de");
        ane.set_language("en").unwrap();
        assert_eq!(ane.transcribe(vec![0.5; 3]).unwrap(), "3 Samples, en");
        assert_eq!(
            ane.transcribe_live(vec![0.5; 3]).unwrap(),
            "3 Samples, enlive"
        );
        assert!(ane.set_boost(&["SOLISTIQ".into(), "Swarm".into()], Some(1.5)));
        let err = ane.ask("bogus", &[]).unwrap_err();
        assert_eq!(err.to_string(), "unknown request");
        drop(ane);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Switching models in the settings loads a second one in the same process.
    /// `cargo test --release -- --ignored loads_two_models` (needs both models downloaded).
    #[test]
    #[ignore]
    fn loads_two_models_one_after_the_other() {
        use crate::asr::{Asr, DEFAULT_MODEL, model_dir};
        use crate::audio::MODEL_RATE;
        register();
        let mut a = Asr::load(&model_dir(DEFAULT_MODEL)).unwrap();
        let mut b = Asr::load(&model_dir("parakeet-ultra-int8")).unwrap();
        let silence = vec![0.0; MODEL_RATE as usize];
        assert!(b.transcribe(silence.clone()).is_ok());
        assert!(a.transcribe(silence).is_ok());
    }
}
