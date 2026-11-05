//! `dictate eval`: how well does recognition work, measured on clips with known text. The list is
//! `wav<TAB>dict<TAB>reference` per line (`scripts/eval.sh` makes it from `eval/sentences.tsv` with
//! macOS voices, `scripts/eval-clips.sh` from the pre-rendered FLAC clips in `eval/clips/`). The numbers: word error rate overall and for sentences with and without
//! dictionary words, how many dictionary words came out spelled right, time per clip. A baseline
//! file turns it into a check. Platform independent.

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::asr::{self, Asr};
use crate::audio::{self, MODEL_RATE};
use crate::config::Config;
use crate::vad;
use crate::vocab::Vocabulary;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub wav: String,
    /// The sentence has words of the default dictionary.
    pub dict: bool,
    pub reference: String,
}

/// Lines without three tab-separated fields and `#` comments are skipped.
pub fn parse_list(text: &str) -> Vec<Item> {
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|line| {
            let mut f = line.splitn(3, '\t');
            let (wav, dict, reference) = (f.next()?, f.next()?, f.next()?);
            Some(Item {
                wav: wav.to_string(),
                dict: dict.trim() == "1",
                reference: reference.trim().to_string(),
            })
        })
        .collect()
}

/// Words as WER counts them: lowercase, letters and digits only ("Pull-Request" is two words).
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Word-level edit distance (substitutions, insertions, deletions).
pub fn edits(reference: &[String], hypothesis: &[String]) -> usize {
    let mut prev: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut cur = vec![i + 1; hypothesis.len() + 1];
        for (j, h) in hypothesis.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(r != h))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[hypothesis.len()]
}

/// How many of the dictionary words in `reference` appear in `hypothesis` with exactly that
/// spelling (case and all), as `(found, wanted)`.
pub fn dictionary_hits(dictionary: &[String], reference: &str, hypothesis: &str) -> (usize, usize) {
    let tokens = |text: &str| -> Vec<String> {
        text.split(|c: char| c.is_whitespace() || ",.;:!?\"„“()".contains(c))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let (reference, hypothesis) = (tokens(reference), tokens(hypothesis));
    let contains = |haystack: &[String], needle: &[String]| {
        !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
    };
    let (mut found, mut wanted) = (0, 0);
    for word in dictionary {
        let needle = tokens(word);
        if contains(&reference, &needle) {
            wanted += 1;
            found += usize::from(contains(&hypothesis, &needle));
        }
    }
    (found, wanted)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub clips: usize,
    pub words: usize,
    pub errors: usize,
}

impl Totals {
    pub fn wer(&self) -> f64 {
        if self.words == 0 {
            0.0
        } else {
            self.errors as f64 / self.words as f64
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub model: String,
    pub clips: usize,
    pub wer: f64,
    pub wer_with_dictionary_words: f64,
    pub wer_without: f64,
    pub dictionary_found: usize,
    pub dictionary_wanted: usize,
    pub ms_median: f64,
    /// Transcription time per second of audio.
    pub took_per_audio_second: f64,
    /// The five clips with the most errors: reference, what came out.
    pub worst: Vec<(String, String)>,
}

#[derive(Default)]
pub struct Run {
    all: Totals,
    with: Totals,
    without: Totals,
    found: usize,
    wanted: usize,
    ms: Vec<f64>,
    took: f64,
    audio: f64,
    misses: Vec<(usize, String, String)>,
}

impl Run {
    pub fn add(
        &mut self,
        item: &Item,
        output: &str,
        dictionary: &[String],
        ms: f64,
        audio_seconds: f64,
    ) {
        let reference = words(&item.reference);
        let errors = edits(&reference, &words(output));
        for totals in [
            &mut self.all,
            if item.dict {
                &mut self.with
            } else {
                &mut self.without
            },
        ] {
            totals.clips += 1;
            totals.words += reference.len();
            totals.errors += errors;
        }
        let (found, wanted) = dictionary_hits(dictionary, &item.reference, output);
        self.found += found;
        self.wanted += wanted;
        self.ms.push(ms);
        self.took += ms / 1000.0;
        self.audio += audio_seconds;
        if errors > 0 {
            self.misses
                .push((errors, item.reference.clone(), output.to_string()));
        }
    }

    pub fn summary(mut self, model: &str) -> Summary {
        self.ms.sort_by(f64::total_cmp);
        self.misses.sort_by_key(|m| std::cmp::Reverse(m.0));
        Summary {
            model: model.into(),
            clips: self.all.clips,
            wer: self.all.wer(),
            wer_with_dictionary_words: self.with.wer(),
            wer_without: self.without.wer(),
            dictionary_found: self.found,
            dictionary_wanted: self.wanted,
            ms_median: self.ms.get(self.ms.len() / 2).copied().unwrap_or(0.0),
            took_per_audio_second: if self.audio > 0.0 {
                self.took / self.audio
            } else {
                0.0
            },
            worst: self
                .misses
                .into_iter()
                .take(5)
                .map(|(_, r, o)| (r, o))
                .collect(),
        }
    }
}

pub fn table(s: &Summary) -> String {
    let pct = |v: f64| format!("{:.1} %", v * 100.0);
    format!(
        "model {}, {} clips\nWER {}  (with dictionary words {}, without {})\ndictionary words {} of {}\n{:.0} ms per clip (median), {:.3} s per second of audio\n",
        s.model,
        s.clips,
        pct(s.wer),
        pct(s.wer_with_dictionary_words),
        pct(s.wer_without),
        s.dictionary_found,
        s.dictionary_wanted,
        s.ms_median,
        s.took_per_audio_second,
    )
}

/// Compares with a baseline; the `Err` lists what got worse by more than the allowed margin
/// (WER up by one point, dictionary words found down by two points).
pub fn compare(now: &Summary, baseline: &Summary) -> (String, Result<(), String>) {
    let rate = |s: &Summary| {
        if s.dictionary_wanted == 0 {
            1.0
        } else {
            s.dictionary_found as f64 / s.dictionary_wanted as f64
        }
    };
    let d_wer = now.wer - baseline.wer;
    let d_dict = rate(now) - rate(baseline);
    let text = format!(
        "against the baseline: WER {:+.1} points, dictionary words {:+.1} points, speed {:+.0} ms per clip\n",
        d_wer * 100.0,
        d_dict * 100.0,
        now.ms_median - baseline.ms_median
    );
    let mut worse = Vec::new();
    if d_wer > 0.01 {
        worse.push(format!("WER up {:.1} points", d_wer * 100.0));
    }
    if d_dict < -0.02 {
        worse.push(format!(
            "dictionary words found down {:.1} points",
            -d_dict * 100.0
        ));
    }
    let verdict = if worse.is_empty() {
        Ok(())
    } else {
        Err(worse.join(", "))
    };
    (text, verdict)
}

/// Transcribes the clips in `list` as the app would (default dictionary, decoder boost, trimming,
/// no personal replacements, so the numbers do not depend on whose Mac it is) and prints the WER.
pub fn run(list: &Path, model: &str, json: Option<&Path>, baseline: Option<&Path>) -> Result<()> {
    let items = parse_list(&std::fs::read_to_string(list)?);
    ensure!(!items.is_empty(), "no clips in {}", list.display());
    let config = Config::default();
    let mut asr = Asr::load(&asr::model_path(model))?;
    asr.set_boost(&config.vocabulary(), config.boost());
    asr.set_language("auto");
    let dictionary = Vocabulary::new(&config.vocabulary());
    // The first inference is slow; keep it out of the timing.
    let _ = asr.transcribe(vec![0.0; MODEL_RATE as usize]);
    let mut run = Run::default();
    for (n, item) in items.iter().enumerate() {
        let clip = audio::read(Path::new(&item.wav))?;
        let seconds = clip.seconds() as f64;
        let started = Instant::now();
        let mut samples = clip.into_model_rate()?;
        if vad::enabled() {
            samples = vad::trim(samples).samples;
        }
        let text = dictionary.correct(&asr.transcribe(samples)?);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        run.add(item, &text, &config.vocabulary(), ms, seconds);
        if (n + 1) % 50 == 0 {
            eprintln!("{} of {} clips", n + 1, items.len());
        }
    }
    let summary = run.summary(model);
    print!("{}", table(&summary));
    for (reference, heard) in &summary.worst {
        println!("  {reference}\n  → {heard}");
    }
    if let Some(path) = json {
        std::fs::write(path, serde_json::to_string_pretty(&summary)? + "\n")?;
        eprintln!("numbers in {}", path.display());
    }
    if let Some(path) = baseline {
        let before: Summary = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let (text, verdict) = compare(&summary, &before);
        print!("{text}");
        if let Err(worse) = verdict {
            bail!("worse than {}: {worse}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str) -> Vec<String> {
        words(text)
    }

    #[test]
    fn list_skips_comments_and_short_lines() {
        let items = parse_list("# c\na.wav\t1\tHallo Welt.\nbroken\nb.wav\t0\tNoch ein Satz\n");
        assert_eq!(items.len(), 2);
        assert!(items[0].dict && !items[1].dict);
        assert_eq!(items[1].reference, "Noch ein Satz");
    }

    #[test]
    fn wer_counts_substitutions_insertions_deletions() {
        assert_eq!(edits(&w("a b c"), &w("a b c")), 0);
        assert_eq!(edits(&w("a b c"), &w("a x c")), 1);
        assert_eq!(edits(&w("a b c"), &w("a c")), 1);
        assert_eq!(edits(&w("a b c"), &w("a b c d")), 1);
        assert_eq!(edits(&w("a b c"), &[]), 3);
        // Case and punctuation do not count.
        assert_eq!(edits(&w("Hallo, Welt!"), &w("hallo welt")), 0);
    }

    #[test]
    fn dictionary_words_must_have_their_spelling() {
        let dictionary: Vec<String> = ["SOLISTIQ", "Pull Request", "Swarm", "Agent Swarm", "Rust"]
            .map(String::from)
            .to_vec();
        let reference = "Ich bin von SOLISTIQ und schreibe einen Pull Request im Agent Swarm.";
        assert_eq!(
            dictionary_hits(&dictionary, reference, reference),
            (4, 4) // SOLISTIQ, Pull Request, Swarm, Agent Swarm
        );
        let heard = "Ich bin von Solistik und schreibe einen Pull Request im Agent Lead.";
        assert_eq!(dictionary_hits(&dictionary, reference, heard), (1, 4));
    }

    #[test]
    fn a_run_sums_up_and_compares() {
        let dictionary = vec!["SOLISTIQ".to_string()];
        let mut run = Run::default();
        let plain = Item {
            wav: "a".into(),
            dict: false,
            reference: "ein zwei drei vier".into(),
        };
        let with = Item {
            wav: "b".into(),
            dict: true,
            reference: "bei SOLISTIQ arbeiten".into(),
        };
        run.add(&plain, "ein zwei drei vier", &dictionary, 50.0, 2.0);
        run.add(&with, "bei Solistik arbeiten", &dictionary, 70.0, 2.0);
        let s = run.summary("m");
        assert_eq!(s.clips, 2);
        assert!((s.wer - 1.0 / 7.0).abs() < 1e-9);
        assert_eq!(s.wer_without, 0.0);
        assert!((s.wer_with_dictionary_words - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!((s.dictionary_found, s.dictionary_wanted), (0, 1));
        assert_eq!(s.worst.len(), 1);
        assert!(table(&s).contains("WER 14.3 %"));
        let (_, same) = compare(&s, &s);
        assert!(same.is_ok());
        let mut better = s.clone();
        better.wer = 0.0;
        better.dictionary_found = 1;
        let (_, verdict) = compare(&s, &better);
        let message = verdict.unwrap_err();
        assert!(
            message.contains("WER") && message.contains("dictionary"),
            "{message}"
        );
    }
}
