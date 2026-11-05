//! Trims silence before transcription: leading and trailing silence goes, long pauses inside shrink
//! to a short gap. Energy based, relative to the recording's own noise floor, so it needs no model
//! and works at any microphone gain. Silence is where Parakeet hallucinates ("Thank you.") and what
//! makes long hands-free recordings slow. `DICTATE_NO_VAD=1` turns it off.

use crate::audio::MODEL_RATE;

/// Analysis frame: 20 ms at 16 kHz.
const FRAME: usize = MODEL_RATE as usize / 50;
/// A frame is speech when it is this much louder than the noise floor…
const ABOVE_FLOOR_DB: f32 = 10.0;
/// …and at least this loud (keeps a dead-quiet room from turning breath into speech).
const MIN_SPEECH_DB: f32 = -62.0;
/// Without this much distance between the quiet and the loudest frames there is no silence to
/// find (continuous speech, steady noise): keep the clip as it is.
const MIN_RANGE_DB: f32 = 15.0;
/// Fewer speech frames than this (60 ms) are a click, not a word.
const MIN_SPEECH_FRAMES: usize = 3;
/// Kept around speech so word starts and ends are not clipped.
const PAD_BEFORE: usize = 12; // 240 ms
const PAD_AFTER: usize = 15; // 300 ms
/// Pauses longer than this…
const LONG_PAUSE: usize = 50; // 1 s
/// …shrink to this.
const SHORT_PAUSE: usize = 15; // 300 ms

/// What [`trim`] did.
#[derive(Debug, PartialEq)]
pub struct Trimmed {
    pub samples: Vec<f32>,
    /// Seconds before and after.
    pub before: f32,
    pub after: f32,
}

impl Trimmed {
    /// For the log: "trimmed 7.9 → 5.2 s".
    pub fn describe(&self) -> String {
        format!("trimmed {:.1} → {:.1} s", self.before, self.after)
    }
}

/// Off when `DICTATE_NO_VAD` is set.
pub fn enabled() -> bool {
    std::env::var_os("DICTATE_NO_VAD").is_none()
}

/// Trims 16 kHz mono `samples`; no speech at all gives an empty clip.
pub fn trim(samples: Vec<f32>) -> Trimmed {
    let before = samples.len() as f32 / MODEL_RATE as f32;
    let keep = speech_mask(&samples);
    let samples = match keep {
        None => samples,
        Some(keep) => {
            let mut out = Vec::with_capacity(samples.len());
            for (i, frame) in samples.chunks(FRAME).enumerate() {
                if keep[i] {
                    out.extend_from_slice(frame);
                }
            }
            out
        }
    };
    let after = samples.len() as f32 / MODEL_RATE as f32;
    Trimmed {
        samples,
        before,
        after,
    }
}

/// Per frame: keep it? `None` keeps everything (no clear silence in the clip).
fn speech_mask(samples: &[f32]) -> Option<Vec<bool>> {
    let db: Vec<f32> = samples.chunks(FRAME).map(frame_db).collect();
    if db.len() < 10 {
        return None;
    }
    let mut sorted = db.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    // The loudest frame, not a percentile: a short "Ja." in ten seconds of hands-free silence is
    // under 5 % of the frames and must not count as silence.
    let loud = sorted[sorted.len() - 1];
    if loud < MIN_SPEECH_DB {
        return Some(vec![false; db.len()]); // nothing but silence
    }
    if loud - floor.max(-100.0) < MIN_RANGE_DB {
        return None;
    }
    let threshold = (floor + ABOVE_FLOOR_DB).max(MIN_SPEECH_DB);
    let speech: Vec<bool> = db.iter().map(|d| *d > threshold).collect();
    if speech.iter().filter(|s| **s).count() < MIN_SPEECH_FRAMES {
        return Some(vec![false; db.len()]);
    }
    Some(keep_frames(&speech))
}

/// Pads speech and shrinks long pauses: from "is this frame speech" to "keep this frame".
fn keep_frames(speech: &[bool]) -> Vec<bool> {
    let n = speech.len();
    let mut padded = vec![false; n];
    for (i, _) in speech.iter().enumerate().filter(|(_, s)| **s) {
        let from = i.saturating_sub(PAD_BEFORE);
        let to = (i + PAD_AFTER + 1).min(n);
        padded[from..to].iter_mut().for_each(|k| *k = true);
    }
    // Inside the speech (not at the edges, those are gone already): cut each long gap down to
    // SHORT_PAUSE, keeping its middle.
    let mut keep = padded.clone();
    let mut i = 0;
    while i < n {
        if padded[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && !padded[i] {
            i += 1;
        }
        let inner = start > 0 && i < n;
        if inner && i - start > LONG_PAUSE {
            let middle = start + (i - start - SHORT_PAUSE) / 2;
            keep[middle..middle + SHORT_PAUSE]
                .iter_mut()
                .for_each(|k| *k = true);
        }
    }
    keep
}

fn frame_db(frame: &[f32]) -> f32 {
    let mean = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    10.0 * mean.max(1e-12).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: usize = MODEL_RATE as usize;

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| amplitude * (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin())
            .collect()
    }

    /// Deterministic noise in [-amplitude, amplitude].
    fn noise(seconds: f32, amplitude: f32) -> Vec<f32> {
        let mut state = 0x2545_f491_u32;
        (0..(seconds * RATE as f32) as usize)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                amplitude * (state as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    fn mix(a: Vec<f32>, b: &[f32]) -> Vec<f32> {
        a.iter().zip(b).map(|(x, y)| x + y).collect()
    }

    #[test]
    fn cuts_silence_before_and_after_speech_with_some_padding() {
        let clip = [vec![0.0; 2 * RATE], tone(1.0, 0.1), vec![0.0; 3 * RATE]].concat();
        let t = trim(clip);
        assert_eq!(t.before, 6.0);
        assert!((t.after - 1.54).abs() < 0.05, "{}", t.after);
        assert!(t.describe().starts_with("trimmed 6.0 → 1.5"));
    }

    #[test]
    fn works_over_a_noise_floor_and_shrinks_long_pauses() {
        let speech = [
            vec![0.0; RATE],
            tone(1.0, 0.05),
            vec![0.0; 4 * RATE],
            tone(1.0, 0.05),
            vec![0.0; RATE],
        ]
        .concat();
        let noisy = mix(speech, &noise(8.0, 0.002));
        let t = trim(noisy);
        // 2 s of tone, padding around both, the 4 s pause down to 0.3 s + padding.
        assert!(t.after > 2.8 && t.after < 3.6, "{}", t.after);
    }

    #[test]
    fn short_pauses_between_words_stay() {
        let clip = [
            vec![0.0; RATE],
            tone(0.5, 0.1),
            vec![0.0; RATE / 2],
            tone(0.5, 0.1),
            vec![0.0; RATE],
        ]
        .concat();
        let t = trim(clip);
        // 0.5 + 0.5 + 0.5 pause + 0.24 + 0.3 padding.
        assert!((t.after - 2.04).abs() < 0.05, "{}", t.after);
    }

    #[test]
    fn a_short_word_in_a_long_quiet_recording_stays_and_a_click_goes() {
        let clip = [vec![0.0; 5 * RATE], tone(0.3, 0.05), vec![0.0; 5 * RATE]].concat();
        let t = trim(clip);
        assert!((t.after - 0.84).abs() < 0.05, "{}", t.after);
        let click = [vec![0.0; 5 * RATE], tone(0.02, 0.5), vec![0.0; 5 * RATE]].concat();
        assert!(trim(click).samples.is_empty());
    }

    #[test]
    fn all_silence_is_empty_and_continuous_speech_is_kept() {
        assert!(trim(vec![0.0; 3 * RATE]).samples.is_empty());
        assert!(trim(noise(3.0, 0.0002)).samples.is_empty());
        let speech = tone(3.0, 0.1);
        assert_eq!(trim(speech.clone()).samples, speech);
        assert_eq!(trim(vec![0.1; 100]).samples, vec![0.1; 100]);
    }
}
