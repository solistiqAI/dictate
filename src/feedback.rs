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
        start: None,
    })
}

pub struct Sounds {
    dir: PathBuf,
    /// The user's own start sound (`start_sound` in config.toml), when the file exists.
    start: Option<PathBuf>,
}

impl Sounds {
    pub fn path(&self, cue: Cue) -> PathBuf {
        match (&self.start, cue) {
            (Some(own), Cue::Start) => own.clone(),
            _ => self.dir.join(cue.file_name()),
        }
    }

    /// Plays `file` instead of the start cue; empty or missing = the built-in cue.
    pub fn set_start(&mut self, file: &str) {
        let own = PathBuf::from(file);
        self.start = (!file.is_empty() && own.is_file()).then_some(own);
        if !file.is_empty() && self.start.is_none() {
            eprintln!("sounds: start_sound {file:?} not found, using the built-in cue");
        }
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
    fn own_start_sound_only_when_the_file_exists() {
        let dir = std::env::temp_dir().join(format!("dictate-cues-{}", std::process::id()));
        let mut sounds = prepare(&dir).unwrap();
        let built_in = dir.join("start.wav");
        sounds.set_start("/no/such/file.wav");
        assert_eq!(sounds.path(Cue::Start), built_in);
        let own = dir.join("own.wav");
        std::fs::copy(&built_in, &own).unwrap();
        sounds.set_start(own.to_str().unwrap());
        assert_eq!(sounds.path(Cue::Start), own);
        assert_eq!(sounds.path(Cue::Stop), dir.join("stop.wav"));
        sounds.set_start("");
        assert_eq!(sounds.path(Cue::Start), built_in);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn levels() {
        assert_eq!(volume("aus"), 0.0);
        assert!(volume("leise") < volume("mittel"));
        assert_eq!(volume("unbekannt"), volume("leise"));
    }
}
