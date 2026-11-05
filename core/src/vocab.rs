//! The dictionary: words we use that the model does not know (SOLISTIQ, Parakeet, Pull Request…).
//! After transcription, words and short word groups that sound like a dictionary entry are
//! replaced by its spelling, e.g. "Solistiekw" or "Solis tick" → "SOLISTIQ". Parakeet takes no
//! prompt, so this runs on the text: a phonetic match against the dictionary.

struct Entry {
    word: String,
    key: String,
    tokens: usize,
}

pub struct Vocabulary(Vec<Entry>);

impl Vocabulary {
    pub fn new<S: AsRef<str>>(words: &[S]) -> Self {
        let entries = words
            .iter()
            .map(|w| w.as_ref().trim())
            .filter(|w| !w.is_empty())
            .map(|word| Entry {
                word: word.to_string(),
                key: sound_key(word),
                tokens: word.split_whitespace().count(),
            })
            // Too short to match by sound without hitting ordinary words.
            .filter(|e| e.key.chars().count() >= 4)
            .collect();
        Vocabulary(entries)
    }

    /// Replaces near-misses of dictionary words. Exact words and everything else stay as they are.
    pub fn correct(&self, text: &str) -> String {
        if self.0.is_empty() {
            return text.to_string();
        }
        let words: Vec<&str> = text.split(' ').collect();
        let mut out: Vec<String> = Vec::with_capacity(words.len());
        let mut i = 0;
        while i < words.len() {
            // The closest group of up to three words wins; on a tie the longer one, so "Pull Re
            // Quest" beats "Pull", but "Pull-Request an" does not swallow the "an".
            let mut found: Option<(usize, usize, &Entry, String)> = None;
            for n in 1..=3.min(words.len() - i) {
                let group = &words[i..i + n];
                let (body, trailing) = split_trailing(&group.join(" "));
                let key = sound_key(&body);
                if key.chars().count() < 4 {
                    continue;
                }
                if let Some((distance, entry)) = self.best(&key, n)
                    && found.as_ref().is_none_or(|(d, ..)| distance <= *d)
                {
                    found = Some((distance, n, entry, trailing));
                }
            }
            match found {
                Some((_, n, entry, trailing)) => {
                    out.push(format!("{}{}", entry.word, trailing));
                    i += n;
                }
                None => {
                    out.push(words[i].to_string());
                    i += 1;
                }
            }
        }
        out.join(" ")
    }

    /// The closest entry for a sound key and its distance.
    fn best(&self, key: &str, tokens: usize) -> Option<(usize, &Entry)> {
        self.0
            .iter()
            // A group of n words may only become an entry of about n words (no swallowing).
            .filter(|e| tokens <= e.tokens + 1)
            .filter_map(|e| {
                if key == e.key {
                    return Some((0, e));
                }
                // Near misses only for longer words that start alike: short ones ("Rust",
                // "Sonnet", "Commit") are too close to everyday words ("nutzt", "sonst", "kommt").
                let len = e.key.chars().count();
                if len < 6 || key.chars().next() != e.key.chars().next() {
                    return None;
                }
                let distance = levenshtein(key, &e.key);
                let len_diff = len.abs_diff(key.chars().count());
                // One edit per five letters; lengths must be close.
                (distance <= len / 5 && len_diff <= 2).then_some((distance, e))
            })
            .min_by_key(|(distance, _)| *distance)
    }
}

/// Punctuation at the end of a word group (",", ".", "?") stays where it was.
fn split_trailing(group: &str) -> (String, String) {
    let body = group.trim_end_matches(|c: char| !c.is_alphanumeric());
    (body.to_string(), group[body.len()..].to_string())
}

/// A rough German/English sound key: lowercase letters and digits only, with spellings that
/// sound alike folded together (c/k/q, ph/f, w/v, ie/i, double letters…).
pub fn sound_key(word: &str) -> String {
    let lower: String = word
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'ß' => 's',
            'j' => 'i', // German j sounds like i/y ("Solistjek")
            c => c,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let mut s = lower;
    for (from, to) in [
        ("sch", "s"),
        ("ph", "f"),
        ("th", "t"),
        ("ck", "k"),
        ("qu", "kv"),
        ("ie", "i"),
        ("ei", "ai"),
        ("ey", "ai"),
        ("ee", "i"),
        ("oo", "u"),
        ("ou", "u"),
        ("tz", "z"),
    ] {
        s = s.replace(from, to);
    }
    let folded: String = s
        .chars()
        .map(|c| match c {
            'c' | 'q' => 'k',
            'w' => 'v',
            'y' => 'i',
            'z' => 's',
            'h' => '\0',
            c => c,
        })
        .filter(|&c| c != '\0')
        .collect();
    let mut key = String::with_capacity(folded.len());
    for c in folded.chars() {
        if !key.ends_with(c) {
            key.push(c);
        }
    }
    key
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A team dictionary like eval/profile.toml: words the model gets wrong.
    pub(crate) const WORDS: &[&str] = &[
        "SOLISTIQ",
        "Swarm",
        "Agent Swarm",
        "Dictate",
        "Parakeet",
        "Claude",
        "Codex",
        "Opus",
        "Sonnet",
        "GitHub",
        "Pull Request",
        "Commit",
        "Deploy",
        "Backlog",
        "Nightly Build",
        "Monorepo",
        "Terraform",
        "ISMS",
        "CRM",
        "Notion",
        "Slack",
        "Raycast",
        "Grafana",
        "Proxmox",
        "Ansible",
        "Kubernetes",
        "Svelte",
        "SvelteKit",
        "Tauri",
        "Rust",
        "Python",
        "Tourenplanung",
        "Forecast",
    ];

    fn vocab() -> Vocabulary {
        Vocabulary::new(WORDS)
    }

    #[test]
    fn fixes_what_parakeet_made_of_our_words() {
        let v = vocab();
        assert_eq!(
            v.correct("Hallo, ich bin Thomas von Solistiekw."),
            "Hallo, ich bin Thomas von SOLISTIQ."
        );
        assert_eq!(v.correct("von Solistjekv. Morgen"), "von SOLISTIQ. Morgen");
        assert_eq!(
            v.correct("bei Solis tick arbeiten"),
            "bei SOLISTIQ arbeiten"
        );
        assert_eq!(v.correct("das Para kiet Modell"), "das Parakeet Modell");
        assert_eq!(
            v.correct("mach einen Pull Requests"),
            "mach einen Pull Request"
        );
        assert_eq!(v.correct("nach Diktate suche"), "nach Dictate suche");
        assert_eq!(
            v.correct("im Agent Swom haben wir das"),
            "im Agent Swarm haben wir das"
        );
    }

    #[test]
    fn leaves_ordinary_words_alone() {
        let v = vocab();
        for text in [
            "Ich hole das Paket morgen ab.",
            "Kannst du den Pull Request reviewen?",
            "Das ist ein Test, test.",
            "Die Schwärme sind schnell, nicht so wie Swarm.",
            "Wir rasten kurz und essen etwas.",
            "Bitte schick mir vorher noch das Deck.",
            // Real transcripts that an earlier, looser matcher got wrong.
            "was brauchen wir sonst noch so",
            "dann kommt nichts.",
            "Nutzt der Agent, der gerade den Python-Port macht, das?",
            "Der Agent läuft über Nacht.",
        ] {
            assert_eq!(v.correct(text), text, "{text}");
        }
        // A near miss must not swallow the next word.
        assert_eq!(
            v.correct("schauen uns den Pull-Request an."),
            "schauen uns den Pull Request an."
        );
    }

    #[test]
    fn keeps_trailing_punctuation_and_empty_dictionary_is_a_no_op() {
        assert_eq!(vocab().correct("Solistik?"), "SOLISTIQ?");
        assert_eq!(Vocabulary::new::<&str>(&[]).correct("Solistik"), "Solistik");
    }

    #[test]
    fn sound_keys_fold_spellings() {
        assert_eq!(sound_key("SOLISTIQ"), sound_key("Solistik"));
        assert_eq!(sound_key("Solistic"), sound_key("solistik"));
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }
}

#[cfg(test)]
mod corpus {
    /// `DICTATE_CORPUS=file cargo test -- --ignored corpus --nocapture` prints what the default
    /// dictionary would change in real transcripts (one per line), to catch false positives.
    #[test]
    #[ignore]
    fn corpus() {
        let v = super::Vocabulary::new(super::tests::WORDS);
        let text = std::fs::read_to_string(std::env::var("DICTATE_CORPUS").unwrap()).unwrap();
        for line in text.lines() {
            let fixed = v.correct(line);
            if fixed != line {
                println!("- {line}\n+ {fixed}");
            }
        }
    }
}
