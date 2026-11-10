//! Learning from corrections: compares a transcript with the text the user made of it and finds
//! the words that were heard wrong ("deckläuen" → "deployen"), to offer them for the dictionary
//! or as a replacement rule. Used by "Letztes Diktat korrigieren…" and, when switched on, by
//! watching the text field after a paste. Platform independent.

use crate::vocab::{Vocabulary, levenshtein, sound_key};

/// Corrections longer than this are rewrites, not misheard words.
const MAX_WORDS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Into the dictionary: the next sound-alike gets the right spelling by itself.
    Dictionary,
    /// A rule `heard => meant` in replacements.txt: for mishearings that sound too different.
    Replacement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub heard: String,
    pub meant: String,
    pub kind: Kind,
}

impl Candidate {
    /// „deckläuen“ → „deployen“
    pub fn describe(&self) -> String {
        crate::text::quoted_change(&self.heard, &self.meant)
    }
}

/// The misheard words between a transcript and its corrected version. `vocabulary` decides
/// between dictionary and replacement: a word the dictionary would catch by sound goes there.
pub fn candidates<S: AsRef<str>>(old: &str, new: &str, vocabulary: &[S]) -> Vec<Candidate> {
    sorted(old, new, vocabulary).0
}

/// What watching a field found: corrections to learn, ones to ask about, and why the other
/// changes were left alone (for the log).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    /// Mishearings ("deckläuen" → "deployen"): learned right away.
    pub sure: Vec<Candidate>,
    /// Close, but beginning with another sound ("Radman" → "Admin"): learned when the user says so.
    pub ask: Vec<Candidate>,
    /// „heard“ → „meant“ and why it is not learned.
    pub skipped: Vec<String>,
}

/// A transcript against what the user made of it in the field.
pub fn review<S: AsRef<str>>(old: &str, new: &str, vocabulary: &[S]) -> Review {
    let (found, mut skipped) = sorted(old, new, vocabulary);
    let mut review = Review::default();
    for c in found {
        match doubt(&c.heard, &c.meant) {
            None => review.sure.push(c),
            Some(Doubt::FirstSound) => review.ask.push(c),
            Some(Doubt::TooDifferent) => {
                skipped.push(format!("{}: too different, a rewording", c.describe()))
            }
        }
    }
    review.skipped = skipped;
    review
}

/// The candidates, and the changes that are none, with why.
fn sorted<S: AsRef<str>>(old: &str, new: &str, vocabulary: &[S]) -> (Vec<Candidate>, Vec<String>) {
    let old: Vec<&str> = old.split_whitespace().collect();
    let new: Vec<&str> = new.split_whitespace().collect();
    let mut out: Vec<Candidate> = Vec::new();
    let mut skipped = Vec::new();
    for (heard, meant) in changes(&old, &new) {
        let change = || crate::text::quoted_change(&heard.join(" "), &meant.join(" "));
        if heard.is_empty() || meant.is_empty() {
            skipped.push(format!("{}: words added or removed only", change()));
            continue;
        }
        if heard.len() > MAX_WORDS || meant.len() > MAX_WORDS {
            skipped.push(format!(
                "{}: more than {MAX_WORDS} words, a rewording",
                change()
            ));
            continue;
        }
        let heard = phrase(heard);
        let meant = phrase(meant);
        // Case, punctuation and hyphens only ("Merge-Request" → "Merge Request"): nothing heard wrong.
        if letters(&heard) == letters(&meant) || heard.is_empty() || meant.is_empty() {
            continue;
        }
        // The replacements file reads "," as a list, "#" as a comment and "=>" as the arrow.
        let unsafe_rule = |s: &str| s.contains(',') || s.contains('#') || s.contains("=>");
        if unsafe_rule(&heard) || unsafe_rule(&meant) {
            skipped.push(format!("{}: has , # or =>", change()));
            continue;
        }
        if out.iter().any(|c| c.heard == heard && c.meant == meant) {
            continue;
        }
        let kind = match suggest(&heard, &meant, vocabulary) {
            // "Dry fliegen" → "drive liegen": a rule for these words, not a dictionary entry
            // that the decoder would push into every sentence.
            Kind::Dictionary if !term(&meant) => Kind::Replacement,
            kind => kind,
        };
        out.push(Candidate { heard, meant, kind });
    }
    (out, skipped)
}

/// Whether a phrase is a name or term for the dictionary: one word, or words that each carry a
/// capital or a digit ("Server Prod", "Pull Request", "M5 Pro"), not ordinary words ("drive liegen").
pub fn term(phrase: &str) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    words.len() <= 1
        || words
            .iter()
            .all(|w| w.chars().any(|c| c.is_uppercase() || c.is_ascii_digit()))
}

/// Why a correction is not learned without asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doubt {
    /// Close, but another first sound ("Radman" → "Admin"): misheard or reworded, the user knows.
    FirstSound,
    /// Too many sounds differ ("Montag" → "Dienstag"): a rewording.
    TooDifferent,
}

/// `None` when a correction looks like a mishearing ("deckläuen" → "deployen", "Solis tick" →
/// SOLISTIQ): same first sound and at most two edits in five on the sound keys.
pub fn doubt(heard: &str, meant: &str) -> Option<Doubt> {
    let (a, b) = (sound_key(heard), sound_key(meant));
    if a.is_empty() || b.is_empty() {
        return Some(Doubt::TooDifferent);
    }
    let longest = a.chars().count().max(b.chars().count());
    if levenshtein(&a, &b) > (longest * 2 / 5).max(1) {
        return Some(Doubt::TooDifferent);
    }
    match a.chars().next() == b.chars().next() {
        true => None,
        // Short words with another first sound ("wir" → "ihr") are rewordings.
        false if longest < 5 => Some(Doubt::TooDifferent),
        false => Some(Doubt::FirstSound),
    }
}

/// Whether a correction looks like a mishearing rather than a rewording ("gut" → "super",
/// "Montag" → "Dienstag"); only these are learned without asking.
pub fn sounds_alike(heard: &str, meant: &str) -> bool {
    doubt(heard, meant).is_none()
}

/// Dictionary when the dictionary (with `meant` in it) would turn `heard` into `meant`; else a
/// replacement rule, which matches `heard` exactly.
pub fn suggest<S: AsRef<str>>(heard: &str, meant: &str, vocabulary: &[S]) -> Kind {
    let mut words: Vec<&str> = vocabulary.iter().map(|w| w.as_ref()).collect();
    words.push(meant);
    if Vocabulary::new(&words).correct(heard) == meant {
        Kind::Dictionary
    } else {
        Kind::Replacement
    }
}

/// `replacements.txt` with the rule added (not when it is there already).
pub fn add_replacement(file: &str, heard: &str, meant: &str) -> String {
    let rule = format!("{heard} => {meant}");
    if file.lines().any(|l| l.trim() == rule) {
        return file.to_string();
    }
    let mut out = file.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&rule);
    out.push('\n');
    out
}

/// Where the pasted text is in a field now. `before` is the field's text before the paste and
/// `at` the insertion point in it (UTF-16 units, as Accessibility counts); `now` the text now.
/// The text before and after the insertion point must be unchanged; then what lies between them
/// is the pasted text as the user has corrected it. `None` when the field changed elsewhere.
pub fn pasted_region<'a>(before: &str, at: usize, now: &'a str) -> Option<&'a str> {
    let split = utf16_to_byte(before, at)?;
    let (prefix, suffix) = before.split_at(split);
    let rest = now.strip_prefix(prefix)?;
    rest.strip_suffix(suffix)
}

fn utf16_to_byte(text: &str, units: usize) -> Option<usize> {
    let mut count = 0;
    for (byte, c) in text.char_indices() {
        if count == units {
            return Some(byte);
        }
        count += c.len_utf16();
    }
    (count == units).then_some(text.len())
}

/// Word lists in, changed stretches out: (old words, new words) per change, by longest common
/// subsequence on the words without punctuation and case.
fn changes<'a, 'b>(old: &'a [&'a str], new: &'b [&'b str]) -> Vec<(&'a [&'a str], &'b [&'b str])> {
    let key = |w: &str| letters(w);
    let old_keys: Vec<String> = old.iter().map(|w| key(w)).collect();
    let new_keys: Vec<String> = new.iter().map(|w| key(w)).collect();
    let (n, m) = (old.len(), new.len());
    // lcs[i][j]: common words of old[i..] and new[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old_keys[i] == new_keys[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut from_i, mut from_j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old_keys[i] == new_keys[j] {
            if from_i < i || from_j < j {
                out.push((&old[from_i..i], &new[from_j..j]));
            }
            i += 1;
            j += 1;
            from_i = i;
            from_j = j;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if from_i < n || from_j < m {
        out.push((&old[from_i..], &new[from_j..]));
    }
    out
}

/// Words joined, without the punctuation around the stretch ("deployen," → "deployen").
fn phrase(words: &[&str]) -> String {
    words
        .join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

/// Lowercase letters and digits only.
fn letters(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DICTIONARY: &[&str] = &["SOLISTIQ", "Pull Request"];

    #[test]
    fn finds_misheard_words_and_groups() {
        let found = candidates(
            "Wir deckläuen morgen bei Solis tick, oder?",
            "Wir deployen morgen bei SOLISTIQ, oder?",
            DICTIONARY,
        );
        assert_eq!(
            found,
            vec![
                Candidate {
                    heard: "deckläuen".into(),
                    meant: "deployen".into(),
                    kind: Kind::Replacement,
                },
                Candidate {
                    heard: "Solis tick".into(),
                    meant: "SOLISTIQ".into(),
                    kind: Kind::Dictionary,
                },
            ]
        );
        assert_eq!(found[0].describe(), "„deckläuen“ → „deployen“");
    }

    #[test]
    fn a_sound_alike_term_goes_into_the_dictionary() {
        let found = candidates(
            "Das Merchrequest ist offen.",
            "Das Mergerequest ist offen.",
            &[] as &[&str],
        );
        assert_eq!(found[0].kind, Kind::Dictionary);
        assert_eq!(
            suggest("Kubernetis", "Kubernetes", DICTIONARY),
            Kind::Dictionary
        );
        assert_eq!(suggest("Gitter", "GitHub", DICTIONARY), Kind::Replacement);
    }

    #[test]
    fn skips_case_punctuation_inserts_and_rewrites() {
        let none = |old: &str, new: &str| candidates(old, new, DICTIONARY).is_empty();
        assert!(none(
            "Das ist der Merge-Request.",
            "Das ist der Merge Request"
        ));
        assert!(none("das ist gut", "Das ist gut!"));
        assert!(none("Das ist gut", "Das ist wirklich gut")); // added word, nothing misheard
        assert!(none("Das ist wirklich gut", "Das ist gut"));
        assert!(none(
            "Ich gehe morgen früh in die Stadt",
            "Wir fahren heute Abend nicht mehr in die Stadt"
        ));
        assert!(none("gleich", "gleich"));
        // Would break replacements.txt (list separator, comment).
        assert!(none("bei Solis, tick heute", "bei SOLISTIQ heute"));
        assert!(none("Kanal eins", "Kanal #eins"));
    }

    #[test]
    fn learns_mishearings_but_not_rewordings() {
        assert!(sounds_alike("deckläuen", "deployen"));
        assert!(sounds_alike("Solis tick", "SOLISTIQ"));
        assert!(sounds_alike("Merch", "Merge"));
        assert!(sounds_alike("Kubernetis", "Kubernetes"));
        assert!(!sounds_alike("gut", "super"));
        assert!(!sounds_alike("Montag", "Dienstag"));
        assert!(!sounds_alike("morgen", "Montag"));
        assert!(!sounds_alike("ich", "wir"));
    }

    #[test]
    fn asks_when_only_the_first_sound_differs() {
        assert_eq!(doubt("deckläuen", "deployen"), None);
        assert_eq!(doubt("Radman-Page", "Admin-Page"), Some(Doubt::FirstSound));
        assert_eq!(doubt("Montag", "Dienstag"), Some(Doubt::TooDifferent));
        assert_eq!(doubt("wir", "ihr"), Some(Doubt::TooDifferent)); // short: a rewording
        let review = review(
            "Wir deckläuen die Radman-Page am Montag.",
            "Wir deployen die Admin-Page am Dienstag.",
            DICTIONARY,
        );
        let heard = |c: &[Candidate]| c.iter().map(|c| c.heard.clone()).collect::<Vec<_>>();
        assert_eq!(heard(&review.sure), ["deckläuen"]);
        assert_eq!(heard(&review.ask), ["Radman-Page"]);
        assert_eq!(
            review.skipped,
            ["„Montag“ → „Dienstag“: too different, a rewording"]
        );
    }

    #[test]
    fn says_why_a_change_is_not_learned() {
        let skipped = |old: &str, new: &str| review(old, new, DICTIONARY).skipped;
        assert_eq!(
            skipped("Das ist gut", "Das ist wirklich gut"),
            ["„“ → „wirklich“: words added or removed only"]
        );
        assert_eq!(
            skipped(
                "Ich gehe morgen früh hin",
                "Wir fahren heute Abend nicht hin"
            ),
            [
                "„Ich gehe morgen früh“ → „Wir fahren heute Abend nicht“: more than 3 words, a rewording"
            ]
        );
        assert_eq!(
            skipped("bei Solis, tick heute", "bei SOLISTIQ heute"),
            ["„Solis, tick“ → „SOLISTIQ“: has , # or =>"]
        );
        assert!(skipped("das ist gut", "Das ist gut!").is_empty());
    }

    #[test]
    fn ordinary_words_become_a_rule_names_go_into_the_dictionary() {
        assert!(term("Drive"));
        assert!(term("Server Prod"));
        assert!(term("M5 Pro"));
        assert!(!term("drive liegen"));
        assert!(!term("Drive liegen"));
        let kind = |old: &str, new: &str| candidates(old, new, &[] as &[&str])[0].kind;
        assert_eq!(
            kind("Das soll in Dry fliegen.", "Das soll in drive liegen."),
            Kind::Replacement
        );
        assert_eq!(
            kind("Das läuft auf ServerProad.", "Das läuft auf Server Prod."),
            Kind::Dictionary
        );
    }

    #[test]
    fn adds_a_rule_once() {
        let file = add_replacement("# Regeln\nSolistik => SOLISTIQ", "deckläuen", "deployen");
        assert_eq!(
            file,
            "# Regeln\nSolistik => SOLISTIQ\ndeckläuen => deployen\n"
        );
        assert_eq!(add_replacement(&file, "deckläuen", "deployen"), file);
        assert_eq!(add_replacement("", "a", "b"), "a => b\n");
    }

    #[test]
    fn finds_the_pasted_text_in_the_field() {
        let before = "Hallo 👋 Welt";
        // Pasted after "Hallo 👋 " (the emoji counts two UTF-16 units).
        let at = "Hallo 👋 ".encode_utf16().count();
        let now = "Hallo 👋 wir deployen heute Welt";
        assert_eq!(pasted_region(before, at, now), Some("wir deployen heute "));
        assert_eq!(pasted_region(before, at, "Hallo Welt"), None);
        assert_eq!(pasted_region(before, at, "Servus 👋 x Welt"), None);
        assert_eq!(pasted_region("", 0, "neu"), Some("neu"));
        assert_eq!(pasted_region("abc", 9, "abc"), None);
    }
}
