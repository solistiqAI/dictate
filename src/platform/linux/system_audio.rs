//! The Souffleur's "Andere" on Linux (BETA, never run): what the computer plays, recorded from
//! the default sink's monitor source by `parec` (PulseAudio, and PipeWire's pulse server; package
//! `pulseaudio-utils`), else `pw-record` capturing the default sink. No permission is asked.
//! Same interface as the macOS tap: `start`, `take` (mono f32 at `rate`), `ended`.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};

/// The rate we ask the tools for (what the models want; no resampling needed).
const RATE: u32 = 16_000;

pub struct SystemAudio {
    child: Child,
    buffer: Arc<Mutex<Vec<f32>>>,
    pub rate: u32,
}

/// The recorders to try: raw mono f32 little endian on stdout.
fn commands() -> [Command; 2] {
    let rate = RATE.to_string();
    let mut parec = Command::new("parec");
    parec.args([
        "--device=@DEFAULT_MONITOR@",
        "--raw",
        "--format=float32le",
        "--channels=1",
        &format!("--rate={rate}"),
    ]);
    let mut pw_record = Command::new("pw-record");
    pw_record.args([
        "-P",
        "{ stream.capture.sink = true }",
        "--format=f32",
        "--channels=1",
        &format!("--rate={rate}"),
        "-",
    ]);
    [parec, pw_record]
}

impl SystemAudio {
    pub fn start() -> Result<Self> {
        let mut tried = Vec::new();
        for mut command in commands() {
            let program = command.get_program().to_string_lossy().into_owned();
            let mut child = match command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    tried.push(format!("{program}: {e}"));
                    continue;
                }
            };
            // A tool that cannot connect ends right away.
            std::thread::sleep(Duration::from_millis(150));
            if let Ok(Some(status)) = child.try_wait() {
                tried.push(format!("{program} ended ({status})"));
                continue;
            }
            let output = child.stdout.take().context("system audio stdout")?;
            return Ok(SystemAudio {
                buffer: read_samples(output),
                child,
                rate: RATE,
            });
        }
        bail!(
            "cannot record what the computer plays (install pulseaudio-utils for parec): {}",
            tried.join("; ")
        )
    }

    pub fn take(&self) -> Vec<f32> {
        std::mem::take(&mut *self.buffer.lock().unwrap())
    }

    pub fn ended(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
}

/// Reads f32 samples from `output` into a buffer, on a thread of its own.
fn read_samples(mut output: impl Read + Send + 'static) -> Arc<Mutex<Vec<f32>>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let shared = buffer.clone();
    std::thread::spawn(move || {
        let mut chunk = vec![0u8; 4 * 4096];
        let mut rest: Vec<u8> = Vec::new();
        while let Ok(n) = output.read(&mut chunk) {
            if n == 0 {
                break;
            }
            rest.extend_from_slice(&chunk[..n]);
            let whole = rest.len() / 4 * 4;
            let samples: Vec<f32> = rest[..whole]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            rest.drain(..whole);
            shared.lock().unwrap().extend(samples);
        }
    });
    buffer
}

impl Drop for SystemAudio {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
