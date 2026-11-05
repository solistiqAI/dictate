//! Context biasing: the dictionary already steers recognition, not only the text afterwards.
//! Every dictionary word is spelled in Parakeet's SentencePiece tokens (`vocab.txt`), in every way
//! the tokens can spell it, and folded into one token automaton (`parakeet_rs::TokenBoost`). The
//! greedy decoder adds a bonus to the logit of each token that starts a dictionary word or
//! continues one that is under way, and drops the path when the next token does not fit.
//! `vocab.rs` still fixes near misses afterwards (and the spelling, e.g. "Solistiq" → SOLISTIQ).
//!
//! Guard rails, each from a false positive measured on TTS recordings:
//! - the bonus only picks between tokens, never a token over a blank, so it cannot add words;
//! - starting a word only competes with other word starts, so it does not split words ("Baut
//!   Software") or spell out numbers ("80 Prozent", "drei");
//! - starting a word needs a piece of at least [`MIN_START_LETTERS`] letters: "▁S", "▁O" or "▁I"
//!   alone turned "Okay" into "Ok";
//! - entries shorter than [`MIN_WORD_LETTERS`] are left to `vocab.rs`: "Rust" made "Rost" rusty;
//! - no all-lower-case spelling: "backlog" lowered "Backend".

use std::collections::{BTreeMap, HashMap};

use parakeet_rs::{BoostEdge, TokenBoost};

/// SentencePiece marks the start of a word with this character.
const WORD_START: char = '▁';

/// Entries with fewer letters are not boosted (too close to everyday words).
const MIN_WORD_LETTERS: usize = 5;

/// A token that starts a dictionary word gets the bonus only if it spells this many letters.
const MIN_START_LETTERS: usize = 3;

/// One spelling of a dictionary entry, `▁` before every word ("▁Pull▁Request").
struct Pattern(String);

/// Where decoding stands in one pattern: (pattern, byte offset of the next character).
type Position = (usize, usize);

/// The token automaton for `words`, every boosted transition worth `weight` on the logit.
pub fn build<S: AsRef<str>>(words: &[S], vocab: &str, weight: f32) -> TokenBoost {
    let pieces = Pieces::parse(vocab);
    let patterns: Vec<Pattern> = words
        .iter()
        .map(AsRef::as_ref)
        .filter(|w| w.chars().filter(|c| c.is_alphanumeric()).count() >= MIN_WORD_LETTERS)
        .flat_map(variants)
        .collect();
    // Per pattern and offset: the tokens that fit there and the offset after them.
    let fits: Vec<Vec<Vec<(usize, usize, usize)>>> = patterns
        .iter()
        .map(|p| {
            (0..p.0.len())
                .map(|offset| {
                    if !p.0.is_char_boundary(offset) {
                        return Vec::new();
                    }
                    pieces
                        .prefixes_of(&p.0[offset..])
                        .map(|(id, len)| {
                            let letters = p.0[offset..offset + len]
                                .chars()
                                .filter(|&c| c != WORD_START)
                                .count();
                            (id, offset + len, letters)
                        })
                        .collect()
                })
                .collect()
        })
        .collect();

    // Subset construction: a node is the set of positions under way. Every node may also start
    // any pattern, so the start positions are not stored in it.
    let starts: Vec<Position> = (0..patterns.len()).map(|p| (p, 0)).collect();
    let mut nodes: Vec<Vec<Position>> = vec![Vec::new()];
    let mut index: HashMap<Vec<Position>, usize> = HashMap::from([(Vec::new(), 0)]);
    let mut next = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        // Per token: bonus, does it only start a word (no continuation), positions after it.
        let mut by_token: BTreeMap<usize, (f32, bool, Vec<Position>)> = BTreeMap::new();
        let under_way = nodes[i].len();
        for (k, &(p, offset)) in nodes[i].iter().chain(&starts).enumerate() {
            for &(token, end, letters) in &fits[p][offset] {
                let continues = k < under_way;
                let bonus = if continues || letters >= MIN_START_LETTERS {
                    weight
                } else {
                    0.0
                };
                let (best, new_word, positions) =
                    by_token.entry(token).or_insert((0.0, true, Vec::new()));
                *best = best.max(bonus);
                *new_word &= !continues;
                // A finished word leaves nothing under way.
                if end < patterns[p].0.len() {
                    positions.push((p, end));
                }
            }
        }
        let mut edges = Vec::with_capacity(by_token.len());
        for (token, (bonus, new_word, mut positions)) in by_token {
            positions.sort_unstable();
            positions.dedup();
            let to = *index.entry(positions.clone()).or_insert_with(|| {
                nodes.push(positions);
                nodes.len() - 1
            });
            edges.push(BoostEdge {
                token,
                to,
                bonus,
                new_word,
            });
        }
        next.push(edges);
        i += 1;
    }
    TokenBoost {
        next,
        word_start: word_starts(vocab),
    }
}

/// Per token id of `vocab.txt`: does it begin a word (`▁` and at least one more character)?
fn word_starts(vocab: &str) -> Vec<bool> {
    let mut starts = Vec::new();
    for line in vocab.lines() {
        let Some((token, id)) = line.rsplit_once(' ') else {
            continue;
        };
        let Ok(id) = id.parse::<usize>() else {
            continue;
        };
        if starts.len() <= id {
            starts.resize(id + 1, false);
        }
        starts[id] = token.starts_with(WORD_START) && token.chars().count() > 1;
    }
    starts
}

/// The spellings Parakeet may produce for an entry: as written and capitalised ("SOLISTIQ" →
/// "Solistiq", which `vocab.rs` turns back; "SvelteKit" → "Sveltekit").
fn variants(entry: &str) -> Vec<Pattern> {
    let words: Vec<&str> = entry.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let capitalised = |w: &str| {
        let mut chars = w.chars();
        let first = chars.next().map(|c| c.to_uppercase().to_string());
        first.unwrap_or_default() + &chars.as_str().to_lowercase()
    };
    let mut spellings: Vec<String> = vec![
        words.join(" "),
        words
            .iter()
            .map(|w| capitalised(w))
            .collect::<Vec<_>>()
            .join(" "),
    ];
    spellings.sort();
    spellings.dedup();
    spellings
        .into_iter()
        .map(|s| {
            Pattern(
                s.split(' ')
                    .map(|w| format!("{WORD_START}{w}"))
                    .collect::<String>(),
            )
        })
        .collect()
}

/// The tokens of `vocab.txt` (`<token> <id>` per line) that can spell text, by first character.
struct Pieces(HashMap<char, Vec<(String, usize)>>);

impl Pieces {
    fn parse(vocab: &str) -> Self {
        let mut by_first: HashMap<char, Vec<(String, usize)>> = HashMap::new();
        for line in vocab.lines() {
            let Some((token, id)) = line.rsplit_once(' ') else {
                continue;
            };
            let Ok(id) = id.parse::<usize>() else {
                continue;
            };
            // Control tokens (<blk>, <|pnc|>, …) and the bare word start spell nothing.
            let control = token.starts_with('<') && token.ends_with('>');
            if control || token.is_empty() || token == WORD_START.to_string() {
                continue;
            }
            let first = token.chars().next().unwrap_or(' ');
            by_first
                .entry(first)
                .or_default()
                .push((token.to_string(), id));
        }
        Pieces(by_first)
    }

    /// Tokens that `text` starts with: (id, length in bytes).
    fn prefixes_of<'a>(&'a self, text: &'a str) -> impl Iterator<Item = (usize, usize)> + 'a {
        let first = text.chars().next().unwrap_or(' ');
        self.0
            .get(&first)
            .into_iter()
            .flatten()
            .filter(move |(token, _)| text.starts_with(token.as_str()))
            .map(|(token, id)| (*id, token.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOCAB: &str = "<unk> 0\n▁ 1\n▁Sol 2\nist 3\nik 4\ni 5\nq 6\n▁S 7\nol 8\n▁sol 9\n\
                         ▁Pull 10\n▁Re 11\nquest 12\n▁und 13\n▁pull 14\n▁re 15\n<blk> 16\n";

    /// Follows `tokens` from the start and returns the bonus each one got.
    fn walk(boost: &TokenBoost, tokens: &[usize]) -> Vec<f32> {
        let mut node = 0;
        tokens
            .iter()
            .map(|&t| {
                let bonus = boost.next[node]
                    .iter()
                    .find(|e| e.token == t)
                    .map_or(0.0, |e| e.bonus);
                node = boost.step(node, t);
                bonus
            })
            .collect()
    }

    #[test]
    fn spellings_cover_capitalisation() {
        let spell = |w: &str| variants(w).into_iter().map(|p| p.0).collect::<Vec<_>>();
        assert_eq!(spell("SOLISTIQ"), ["▁SOLISTIQ", "▁Solistiq"]);
        assert_eq!(spell("GitHub"), ["▁GitHub", "▁Github"]);
        assert_eq!(spell("Pull Request"), ["▁Pull▁Request"]);
        assert_eq!(spell("swarm"), ["▁Swarm", "▁swarm"]);
        assert!(spell("  ").is_empty());
    }

    #[test]
    fn every_token_spelling_of_a_word_is_boosted() {
        let boost = build(&["SOLISTIQ"], VOCAB, 1.5);
        // ▁Sol ist i q and ▁S ol ist i q both spell "Solistiq"; "▁S" alone is too short to
        // earn a bonus but still opens the path.
        assert_eq!(walk(&boost, &[2, 3, 5, 6]), [1.5; 4]);
        assert_eq!(walk(&boost, &[7, 8, 3, 5, 6]), [0.0, 1.5, 1.5, 1.5, 1.5]);
        // No all-lower-case spelling.
        assert_eq!(walk(&boost, &[9, 3, 5, 6]), [0.0; 4]);
        // "Solistik": the path breaks at "ik", nothing after it is boosted until a word starts.
        assert_eq!(walk(&boost, &[2, 3, 4, 13]), [1.5, 1.5, 0.0, 0.0]);
        assert_eq!(walk(&boost, &[2, 3, 4, 2]), [1.5, 1.5, 0.0, 1.5]);
        // Continuations are only boosted after the word started: "ist" alone gets nothing.
        assert_eq!(walk(&boost, &[13, 3]), [0.0, 0.0]);
    }

    #[test]
    fn starting_a_word_is_marked_continuing_is_not() {
        let boost = build(&["SOLISTIQ"], VOCAB, 1.0);
        let edge =
            |node: usize, token| *boost.next[node].iter().find(|e| e.token == token).unwrap();
        let start = edge(0, 2);
        assert!(start.new_word);
        assert!(!edge(start.to, 3).new_word);
        assert_eq!(boost.word_start[..4], [false, false, true, false]);
    }

    #[test]
    fn word_groups_continue_across_the_word_start() {
        let boost = build(&["Pull Request"], VOCAB, 2.0);
        assert_eq!(walk(&boost, &[10, 11, 12]), [2.0; 3]);
        assert_eq!(walk(&boost, &[14, 15, 12]), [0.0; 3]);
        // "▁Re" on its own does not start anything.
        assert_eq!(walk(&boost, &[13, 11, 12]), [0.0; 3]);
        // After the word everything starts over.
        assert_eq!(walk(&boost, &[10, 11, 12, 12]), [2.0, 2.0, 2.0, 0.0]);
    }

    #[test]
    fn short_entries_are_left_to_the_text_dictionary() {
        let vocab = format!("{VOCAB}▁Rust 17\n▁CRM 18\n");
        assert_eq!(build(&["Rust", "CRM"], &vocab, 1.0).next, vec![vec![]]);
    }

    #[test]
    fn control_tokens_and_the_bare_word_start_are_never_boosted() {
        let boost = build(&["SOLISTIQ", "Pull Request"], VOCAB, 1.0);
        for edges in &boost.next {
            assert!(
                edges.windows(2).all(|w| w[0].token < w[1].token),
                "sorted by token"
            );
            assert!(edges.iter().all(|e| ![0, 1, 16].contains(&e.token)));
        }
        assert_eq!(build::<&str>(&[], VOCAB, 1.0).next, vec![vec![]]);
    }

    #[test]
    fn the_real_dictionary_fits_parakeet_vocabulary() {
        // Without the model at hand, every letter is a token: the automaton stays small.
        let mut vocab = String::new();
        let mut id = 0;
        for c in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            vocab += &format!("{c} {id}\n{WORD_START}{c} {}\n", id + 1);
            id += 2;
        }
        let boost = build(crate::vocab::tests::WORDS, &vocab, 1.0);
        assert!(boost.next.len() < 2000, "{} nodes", boost.next.len());
    }
}

#[cfg(test)]
mod corpus {
    use std::path::Path;

    use crate::asr::{self, Asr};

    /// Measures the bonus on recordings: `DICTATE_BOOST_CORPUS=list.tsv` (a WAV path first on
    /// each line, tab-separated) and `DICTATE_BOOST_WEIGHTS=0,1,2`; prints one JSON line per
    /// recording and weight, raw and after the text dictionary and how long it took, for a
    /// script to score against what was said. `DICTATE_BOOST_MODEL` picks another CPU model
    /// (default parakeet-ultra; the Neural Engine is the app's and not registered here).
    /// Needs the model.
    /// `cargo test --release -p dictate-core -- --ignored boost::corpus --nocapture`
    #[test]
    #[ignore]
    fn measure() {
        let list = std::fs::read_to_string(std::env::var("DICTATE_BOOST_CORPUS").unwrap()).unwrap();
        let weights: Vec<f32> = std::env::var("DICTATE_BOOST_WEIGHTS")
            .unwrap_or_else(|_| "0,1,2,3".into())
            .split(',')
            .map(|w| w.trim().parse().unwrap())
            .collect();
        let words = crate::config::Config::default().vocabulary();
        let dictionary = crate::vocab::Vocabulary::new(&words);
        let model =
            std::env::var("DICTATE_BOOST_MODEL").unwrap_or_else(|_| "parakeet-ultra".into());
        let dir = asr::model_dir(&model);
        let mut asr = Asr::load(&dir).unwrap();
        if let Ok(vocab) = std::fs::read_to_string(dir.join("vocab.txt")) {
            let started = std::time::Instant::now();
            let nodes = super::build(&words, &vocab, 1.0).next.len();
            eprintln!("{nodes} nodes, built in {:?}", started.elapsed());
        }
        let clips: Vec<(&str, Vec<f32>)> = list
            .lines()
            .filter_map(|line| line.split('\t').next().filter(|w| !w.is_empty()))
            .map(|wav| {
                let clip = crate::audio::read_wav(Path::new(wav)).unwrap();
                (wav, clip.into_model_rate().unwrap())
            })
            .collect();
        for &weight in &weights {
            asr.set_boost(&words, Some(weight));
            for (wav, samples) in &clips {
                let started = std::time::Instant::now();
                let raw = asr.transcribe(samples.clone()).unwrap();
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                let seconds = samples.len() as f64 / crate::audio::MODEL_RATE as f64;
                let fixed = dictionary.correct(&raw);
                let line = serde_json::json!({"weight": weight, "wav": wav, "raw": raw,
                    "fixed": fixed, "ms": ms, "seconds": seconds});
                println!("{line}");
            }
        }
    }
}
