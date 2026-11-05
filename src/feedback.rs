//! Start/stop/cancel cues: short, soft two-note chimes we synthesise ourselves (no system sounds,
//! no borrowed audio files), and how loud to play them.

use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
    Cancel,
}

impl Cue {
    fn notes(self) -> [f32; 2] {
        match self {
            Cue::Start => [660.0, 990.0],  // rising fifth
            Cue::Stop => [990.0, 660.0],   // falling fifth
            Cue::Cancel => [440.0, 330.0], // lower, falling fourth
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Cue::Start => "start.wav",
            Cue::Stop => "stop.wav",
            Cue::Cancel => "cancel.wav",
        }
    }
}

/// Settings value → playback volume (0 = silent). Measured peaks of the cues: "leise" -29 dBFS,
/// "mittel" -21 dBFS; the macOS Tink/Pop we used before peaked at -17/-19 dBFS (at afplay -v 0.4).
pub fn volume(level: &str) -> f32 {
    match level {
        "aus" => 0.0,
        "mittel" => 0.2,
        _ => 0.08, // "leise", the default
    }
}

const RATE: u32 = 44_100;

/// Two 55 ms sine notes with a soft attack and exponential decay, peak 0.5.
pub fn synth(cue: Cue) -> Vec<f32> {
    let note_len = (RATE as f32 * 0.055) as usize;
    let mut out = Vec::with_capacity(note_len * 2 + RATE as usize / 10);
    for (n, freq) in cue.notes().iter().enumerate() {
        // The second note rings out a little longer.
        let len = if n == 0 { note_len } else { note_len * 2 };
        for i in 0..len {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let decay = (-t / 0.035).exp();
            let tone = (std::f32::consts::TAU * freq * t).sin()
                + 0.12 * (std::f32::consts::TAU * 2.0 * freq * t).sin();
            out.push(0.45 * attack * decay * tone);
        }
    }
    out
}

/// Writes the cue files into `dir` (once per start; cheap) and returns where they are.
pub fn prepare(dir: &Path) -> Result<Sounds> {
    std::fs::create_dir_all(dir)?;
    for cue in [Cue::Start, Cue::Stop, Cue::Cancel] {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(dir.join(cue.file_name()), spec)?;
        for s in synth(cue) {
            writer.write_sample((s * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }
    Ok(Sounds {
        dir: dir.to_path_buf(),
    })
}

pub struct Sounds {
    dir: PathBuf,
}

impl Sounds {
    pub fn path(&self, cue: Cue) -> PathBuf {
        self.dir.join(cue.file_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_short_soft_and_differ() {
        for cue in [Cue::Start, Cue::Stop, Cue::Cancel] {
            let s = synth(cue);
            let seconds = s.len() as f32 / RATE as f32;
            assert!((0.1..0.25).contains(&seconds), "{cue:?} {seconds}");
            let peak = s.iter().fold(0.0f32, |p, v| p.max(v.abs()));
            assert!(peak <= 0.5 && peak > 0.2, "{cue:?} peak {peak}");
        }
        assert_ne!(synth(Cue::Start), synth(Cue::Stop));
    }

    #[test]
    fn levels() {
        assert_eq!(volume("aus"), 0.0);
        assert!(volume("leise") < volume("mittel"));
        assert_eq!(volume("unbekannt"), volume("leise"));
    }
}
