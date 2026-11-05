//! The voice: PocketTTS (FluidAudio) in the Swift helper `dictate-ane tts-serve german`, German
//! "juergen" by default. Better than the system voice and still on the device. The helper starts
//! on first use (0.7 s once its 450 MB pack is in FluidAudio's cache; the first use fetches the pack
//! in the background and the system voice speaks meanwhile) and ends after ten idle minutes, so it
//! costs no memory while nobody listens.
//!
//! Text goes sentence by sentence: the helper makes a WAV per sentence (about 3x faster than it
//! plays), `afplay` plays them one after the other while the next is made, so the voice starts
//! after the first sentence. Protocol: see `ane/Sources/dictate-ane/main.swift`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};

use crate::ane::helper;

/// The language pack (6 layers: about 3x real time; the 24-layer one is twice as slow as real time).
const LANGUAGE: &str = "german";
const IDLE_END: Duration = Duration::from_secs(600);

struct Proc {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

pub struct Pocket {
    proc: Mutex<Option<Proc>>,
    last_used: Mutex<Instant>,
    generation: AtomicU64,
    player: Mutex<Option<Child>>,
    /// A fetch of the pack is running (or failed): not started twice.
    fetching: Mutex<bool>,
}

/// Where FluidAudio keeps the pack once it is downloaded.
fn pack() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".cache/fluidaudio/Models/pocket-tts/v2.1")
            .join(LANGUAGE)
            .join("manifest.json"),
    )
}

/// Sentences for the voice: cut after . ! ? : ; and line ends, short pieces joined with the next,
/// long ones cut at commas, so each piece takes about a second to make.
pub fn sentences(text: &str) -> Vec<String> {
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        let next_is_break = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        let ends = matches!(c, '.' | '!' | '?' | ':' | ';' | '\n') && next_is_break;
        // "z. B." and "Nr. 5": a short lower-case word before the dot is no sentence end.
        let abbreviation = c == '.'
            && current
                .trim_end_matches('.')
                .rsplit(' ')
                .next()
                .is_some_and(|w| w.chars().count() <= 2 && !w.is_empty());
        if ends && !abbreviation {
            pieces.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        pieces.push(current);
    }
    let mut out: Vec<String> = Vec::new();
    let mut carry = String::new();
    for piece in pieces {
        carry.push_str(piece.trim());
        carry.push(' ');
        if carry.trim().chars().count() >= 15 {
            out.extend(split_long(carry.trim()));
            carry.clear();
        }
    }
    if !carry.trim().is_empty() {
        match out.last_mut() {
            Some(last) if carry.trim().chars().count() < 12 => {
                last.push(' ');
                last.push_str(carry.trim());
            }
            _ => out.push(carry.trim().to_string()),
        }
    }
    out.retain(|s| s.chars().any(|c| c.is_alphanumeric()));
    out
}

fn split_long(text: &str) -> Vec<String> {
    const MAX: usize = 220;
    if text.chars().count() <= MAX {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut current = String::new();
    for part in text.split_inclusive(", ") {
        if !current.is_empty() && current.chars().count() + part.chars().count() > MAX {
            out.push(current.trim().to_string());
            current.clear();
        }
        current.push_str(part);
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

impl Pocket {
    pub fn new() -> Arc<Self> {
        let pocket = Arc::new(Pocket {
            proc: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
            generation: AtomicU64::new(0),
            player: Mutex::new(None),
            fetching: Mutex::new(false),
        });
        let weak = Arc::downgrade(&pocket);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(60));
                let Some(pocket) = weak.upgrade() else { return };
                if pocket.last_used.lock().unwrap().elapsed() > IDLE_END
                    && let Some(mut p) = pocket.proc.lock().unwrap().take()
                {
                    let _ = p.child.kill();
                    let _ = p.child.wait();
                    eprintln!("voice: helper ended after idle time");
                }
            }
        });
        pocket
    }

    pub fn cached() -> bool {
        pack().is_some_and(|p| p.exists())
    }

    /// Fetches the pack in the background (once); the helper downloads it when it starts.
    pub fn fetch(self: &Arc<Self>) {
        let mut fetching = self.fetching.lock().unwrap();
        if *fetching {
            return;
        }
        *fetching = true;
        let me = self.clone();
        std::thread::spawn(move || {
            eprintln!("voice: fetching the PocketTTS pack (about 450 MB)");
            match me.ensure(&mut me.proc.lock().unwrap()) {
                Ok(()) => eprintln!("voice: PocketTTS ready"),
                Err(e) => {
                    eprintln!("voice: PocketTTS pack failed: {e:#}");
                    *me.fetching.lock().unwrap() = false; // the next use tries again
                }
            }
        });
    }

    fn ensure(&self, slot: &mut Option<Proc>) -> Result<()> {
        if let Some(p) = slot.as_mut()
            && matches!(p.child.try_wait(), Ok(None))
        {
            return Ok(());
        }
        let mut child = Command::new(helper())
            .args(["tts-serve", LANGUAGE])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start {}", helper().display()))?;
        let input = child.stdin.take().context("helper stdin")?;
        let mut output = BufReader::new(child.stdout.take().context("helper stdout")?);
        let mut first = String::new();
        output.read_line(&mut first)?;
        if !first.starts_with("ready") {
            let _ = child.kill();
            bail!("voice helper: {}", first.trim());
        }
        *slot = Some(Proc {
            child,
            input,
            output,
        });
        Ok(())
    }

    /// One sentence as WAV (24 kHz mono).
    pub fn synth(&self, text: &str, voice: &str) -> Result<Vec<u8>> {
        let mut slot = self.proc.lock().unwrap();
        self.ensure(&mut slot)?;
        *self.last_used.lock().unwrap() = Instant::now();
        let p = slot.as_mut().ok_or_else(|| anyhow!("no voice helper"))?;
        let result = (|| -> Result<Vec<u8>> {
            writeln!(p.input, "speak {voice} {}", serde_json::to_string(text)?)?;
            p.input.flush()?;
            let mut head = String::new();
            p.output.read_line(&mut head)?;
            let Some(n) = head.trim().strip_prefix("ok ") else {
                bail!("voice helper: {}", head.trim());
            };
            let mut wav = vec![0u8; n.parse()?];
            p.output.read_exact(&mut wav)?;
            Ok(wav)
        })();
        if result.is_err() {
            // The pipe is out of step or the helper died: start afresh next time.
            if let Some(mut p) = slot.take() {
                let _ = p.child.kill();
                let _ = p.child.wait();
            }
        }
        result
    }

    /// Speaks `text`; the first sentence is made before this returns (so a broken helper is an
    /// error the caller can answer with the system voice), the rest follows while it plays.
    /// `on_done` runs when everything has been played, not after `stop` or a newer `speak`.
    pub fn speak(
        self: &Arc<Self>,
        text: &str,
        voice: &str,
        speed: f32,
        on_done: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        self.stop();
        let parts = sentences(text);
        let Some(first) = parts.first() else {
            on_done();
            return Ok(());
        };
        let generation = self.generation.load(Ordering::SeqCst);
        let first_wav = self.synth(first, voice)?;
        let dir = std::env::temp_dir();
        let file = move |i: usize| {
            dir.join(format!(
                "dictate-voice-{}-{generation}-{i}.wav",
                std::process::id()
            ))
        };
        let (tx, rx) = sync_channel::<PathBuf>(3);
        std::fs::write(file(0), first_wav)?;
        tx.send(file(0)).ok();
        let rest: Vec<String> = parts.into_iter().skip(1).collect();
        let (me, voice) = (self.clone(), voice.to_string());
        std::thread::spawn(move || {
            for (i, part) in rest.iter().enumerate() {
                if me.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                match me.synth(part, &voice) {
                    Ok(wav) => {
                        let path = file(i + 1);
                        if std::fs::write(&path, wav).is_ok() {
                            let _ = tx.send(path);
                        }
                    }
                    Err(e) => {
                        eprintln!("voice: {e:#}");
                        return;
                    }
                }
            }
        });
        let me = self.clone();
        std::thread::spawn(move || {
            for path in rx {
                if me.generation.load(Ordering::SeqCst) != generation {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                // -r with -q 1: faster with the pitch kept (rate-scaled playback, high quality).
                let mut play = Command::new("/usr/bin/afplay");
                if (speed - 1.0).abs() > 0.01 {
                    play.args(["-r", &format!("{speed:.2}"), "-q", "1"]);
                }
                match play.arg(&path).spawn() {
                    Ok(child) => *me.player.lock().unwrap() = Some(child),
                    Err(e) => eprintln!("voice: afplay: {e}"),
                }
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                    if me.generation.load(Ordering::SeqCst) != generation {
                        break;
                    }
                    let mut player = me.player.lock().unwrap();
                    match player.as_mut().map(|c| c.try_wait()) {
                        Some(Ok(None)) => {}
                        _ => {
                            *player = None;
                            break;
                        }
                    }
                }
                let _ = std::fs::remove_file(&path);
            }
            if me.generation.load(Ordering::SeqCst) == generation {
                on_done();
            }
        });
        Ok(())
    }

    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(mut child) = self.player.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cut_into_pieces_of_a_sentence() {
        let parts = sentences(
            "Das ist der erste Satz. Und hier der zweite, etwas längere Satz! Ja. Kurz? Ende.",
        );
        assert_eq!(parts[0], "Das ist der erste Satz.");
        assert!(parts[1].starts_with("Und hier der zweite"), "{parts:?}");
        // Short bits ("Ja.", "Kurz?") stick to a neighbour instead of being spoken alone.
        assert!(parts.iter().all(|p| p.chars().count() >= 12), "{parts:?}");
        assert!(sentences("   ").is_empty());
        assert!(sentences("...").is_empty());
    }

    #[test]
    fn abbreviations_and_long_sentences() {
        let parts = sentences("Das gilt z. B. für den Agent Swarm und alle anderen Projekte hier.");
        assert_eq!(parts.len(), 1, "{parts:?}");
        let long = format!("{}.", "ein recht langer Satzteil, ".repeat(20));
        let parts = sentences(&long);
        assert!(
            parts.len() >= 2 && parts.iter().all(|p| p.chars().count() <= 260),
            "{parts:?}"
        );
    }
}
