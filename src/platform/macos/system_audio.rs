//! The Souffleur's "Andere": what the Mac plays, from the Swift helper's Core Audio process tap
//! (`dictate-ane system-audio`, macOS 14.2+, permission "Systemaudioaufnahme"). The helper writes
//! `rate <hz>` (or `error …`), then mono f32 samples, little endian, until it is stopped.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};

pub struct SystemAudio {
    child: Child,
    buffer: Arc<Mutex<Vec<f32>>>,
    pub rate: u32,
}

impl SystemAudio {
    pub fn start() -> Result<Self> {
        let mut child = Command::new(super::ane::helper())
            .arg("system-audio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start dictate-ane system-audio")?;
        let mut output = child.stdout.take().context("system audio stdout")?;
        // The header line, byte by byte so no audio is read past it.
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        while output.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {
            header.push(byte[0]);
        }
        let header = String::from_utf8_lossy(&header).to_string();
        let rate: u32 = match header.strip_prefix("rate ") {
            Some(rate) => rate.trim().parse().context("system audio rate")?,
            None => bail!("{}", header.strip_prefix("error ").unwrap_or(&header)),
        };
        let buffer = Arc::new(Mutex::new(Vec::new()));
        {
            let buffer = buffer.clone();
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
                    buffer.lock().unwrap().extend(samples);
                }
            });
        }
        Ok(SystemAudio {
            child,
            buffer,
            rate,
        })
    }

    pub fn take(&self) -> Vec<f32> {
        std::mem::take(&mut *self.buffer.lock().unwrap())
    }

    pub fn ended(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
}

impl Drop for SystemAudio {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
