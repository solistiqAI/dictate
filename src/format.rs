//! "Formatieren": a dictation trigger with `format = true` lays the text out (paragraphs, lists)
//! before pasting, with Apple's on-device language model (`dictate-ane format`, macOS 26 with
//! Apple Intelligence). How its proposal is carried onto the transcript (only the layout, never
//! its words) is platform-free and lives in `dictate-core` (`core/src/format.rs`).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

/// Shorter dictations have nothing to lay out.
const MIN_WORDS: usize = 8;
/// Longer ones are not worth the wait (and the model's context is 4096 tokens).
const MAX_WORDS: usize = 600;
const TIMEOUT: Duration = Duration::from_secs(15);

/// The transcript laid out, or why it stays as it is (for the log).
pub fn format(text: &str) -> Result<String, String> {
    let words = text.split_whitespace().count();
    if !(MIN_WORDS..=MAX_WORDS).contains(&words) || text.contains('\n') {
        // Too short, too long, or already laid out with spoken commands.
        return Ok(text.to_string());
    }
    let proposal = propose("format", text).map_err(|e| format!("{e:#}"))?;
    layout(text, &proposal).ok_or_else(|| "proposal rewords the text, not used".to_string())
}

/// The model's version of `text`; `mode` is `format` or `correct` (`correct.rs`).
pub fn propose(mode: &str, text: &str) -> Result<String> {
    let helper = crate::ane::helper();
    let mut child = Command::new(&helper)
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("run {}", helper.display()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("no stdin"))?
        .write_all(text.as_bytes())?;
    let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut answer = String::new();
        let _ = tx.send(stdout.read_to_string(&mut answer).map(|_| answer));
    });
    let answer = match rx.recv_timeout(TIMEOUT) {
        Ok(answer) => answer?,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("no answer within {} s", TIMEOUT.as_secs());
        }
    };
    let _ = child.wait();
    let line = answer.lines().last().unwrap_or_default();
    match line.split_once(' ') {
        Some(("ok", json)) => Ok(serde_json::from_str(json)?),
        Some(("error", message)) => bail!("{message}"),
        _ => bail!("unexpected answer {line:?}"),
    }
}

pub use dictate_core::format::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_or_laid_out_text_skips_the_model() {
        assert_eq!(format("Okay."), Ok("Okay.".into()));
        assert_eq!(
            format("Liste\n- Milch\n- Eier"),
            Ok("Liste\n- Milch\n- Eier".into())
        );
    }
}
