//! "Korrigieren": a dictation trigger with `correct = true` drops self-corrections ("um drei, äh
//! nein, um fünf Uhr" → "um fünf Uhr") and filler words before pasting.
//!
//! Filler words ("äh", "ähm") go by rule. Self-corrections need a language model, and only when
//! the text holds a correction cue ("nein", "sorry", "ich meine", "Moment", …): most dictations
//! never reach the model. The model is Gemma 4 E4B on the GPU (`llm.rs`) when it is downloaded and
//! chosen (`correct_model`), else Apple's on-device model (`dictate-ane correct`); Gemma failing
//! falls back to Apple too. Both get the same instructions (`PROMPT`).
//!
//! The model only proposes, and only deletions are taken from it. Left alone it answers dictated
//! questions, writes requested mails, reorders and invents, so a proposal is used only when its
//! words are the transcript's words in order with some left out, and every left-out stretch has
//! the shape of a self-correction: the retracted words, a pause (punctuation, or a filler word
//! that was there), then the cue at its end, and the words after the cue stay. Anything else
//! leaves the transcript as it is, and the log says why. Kept words keep their spelling (numbers
//! stay digits or words as Parakeet wrote them); only the punctuation before a cut and the capital
//! letter after it follow the proposal.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::format::norm;
use crate::llm::{self, Llm};

/// The instructions for either model; `dictate-ane` holds the same text for Apple's (a test checks).
pub const PROMPT: &str = include_str!("correct-prompt.txt");
/// What Gemma gets on top: lay the text out too (Formatieren), in the same answer.
pub const LAYOUT: &str = include_str!("layout-prompt.txt");
/// Gemma's instructions (`PROMPT` + `LAYOUT`), or for trying others the file
/// `DICTATE_CORRECT_PROMPT` names.
fn prompt() -> String {
    std::env::var_os("DICTATE_CORRECT_PROMPT")
        .and_then(|f| std::fs::read_to_string(f).ok())
        .unwrap_or_else(|| format!("{}\n\n{}", PROMPT.trim_end(), LAYOUT))
}

/// Long enough for loading Gemma when the first dictation comes right after the start.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The loaded local model, by name.
static LOCAL: Mutex<Option<(&'static str, Llm)>> = Mutex::new(None);

/// A short question to the loaded local model under its own instructions (`system`); None when
/// none is loaded (Apple's model is not asked: too slow for a question on the side).
pub fn ask_local(system: &str, text: &str, timeout: Duration) -> Option<anyhow::Result<String>> {
    let local = LOCAL.lock().unwrap().clone();
    local.map(|(_, model)| model.ask_with(system, text, timeout))
}

/// Gets the model `choice` ready (a `llm::MODELS` name, or "apple"): a local model starts loading
/// in the background and stays loaded; Apple's needs nothing. Returns what corrections will use.
pub fn prepare(choice: &str) -> String {
    let mut local = LOCAL.lock().unwrap();
    let wanted = llm::find(choice).filter(|m| llm::path(m).exists());
    match (wanted, local.as_ref()) {
        (Some(model), Some((name, _))) if *name == model.name => {}
        (Some(model), _) => *local = Some((model.name, Llm::start(model, &prompt()))),
        (None, _) => *local = None,
    }
    match (wanted, llm::find(choice)) {
        (Some(model), _) => model.name.to_string(),
        (None, Some(model)) => format!(
            "Apple ({} not downloaded: `dictate model download {}`)",
            model.name, model.name
        ),
        (None, None) => "Apple".to_string(),
    }
}

/// Words that carry nothing and go by rule.
const FILLERS: &[&str] = &[
    "äh", "ähm", "ähh", "äähm", "öh", "öhm", "ehm", "hm", "hmm", "uhm",
];
/// Interjections that may start a cue ("ah ne", "ach nee"); deleting them alone is harmless.
const INTERJECTIONS: &[&str] = &["ah", "ach", "oh", "äh", "ähm"];
/// The colloquial no that marks a correction after an interjection or before a pause ("ah ne,").
const NO: &[&str] = &["ne", "nee", "nö", "nä"];

/// Cues that say "what I just said was wrong". A cut must end in one of them.
const STRONG: &[&[&str]] = &[
    &["nein"],
    &["sorry"],
    &["quatsch"],
    &["moment"],
    &["korrektur"],
    &["pardon"],
    &["falsch"],
    &["entschuldigung"],
    &["ich", "meine"],
    &["ich", "mein"],
    &["ich", "habe", "mich", "vertan"],
    &["ich", "hab", "mich", "vertan"],
    &["das", "ist", "falsch"],
    &["das", "war", "falsch"],
    &["besser", "gesagt"],
    &["oder", "besser"],
    &["oder", "vielmehr"],
    &["streich", "das"],
    &["vergiss", "das"],
    &["no"],
    &["i", "mean"],
    &["scratch", "that"],
];
/// Words that go with a cue ("äh nein", "nein, doch lieber", "also, Moment").
const WEAK: &[&[&str]] = &[
    &["ah"],
    &["ach"],
    &["oh"],
    &["äh"],
    &["ähm"],
    &["also"],
    &["oder"],
    &["besser"],
    &["doch"],
    &["lieber"],
    &["eher"],
    &["vielmehr"],
    &["warte"],
    &["sagen", "wir"],
];

/// At most this many retracted words before one cue.
const MAX_RETRACTED: usize = 12;
/// At most this share of all words may be retracted (a restart retracts a lot: "Also ich sage
/// jetzt zwei Tickets, ah ne, nimm vier Tickets und …" is 6 of 14).
const MAX_SHARE: f32 = 0.5;
/// Too short to hold a correction, or not worth the wait.
const MIN_WORDS: usize = 4;
const MAX_WORDS: usize = 600;

/// `text` without filler words ("äh", "ähm"), by rule, no model: what Souffleur's lines get.
pub fn drop_filler_words(text: &str) -> String {
    text.split('\n')
        .map(|line| drop_fillers(line).0.join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a shortcut does with its dictation: `correct` drops self-corrections and filler words,
/// `format` lays the text out (paragraphs, lists). With Gemma both come from one answer per piece;
/// without it Apple's model corrects (where a cue is) and then lays out, one call each. Returns
/// the text and a note for the log.
pub fn tidy_text(text: &str, correct: bool, format: bool) -> (String, String) {
    if !correct && !format {
        return (text.to_string(), String::new());
    }
    let Some((name, model)) = LOCAL.lock().unwrap().clone() else {
        let (mut text, mut note) = match correct {
            true => self::correct(text),
            false => (text.to_string(), String::new()),
        };
        if format {
            let started = std::time::Instant::now();
            match crate::format::format(&text) {
                Ok(laid_out) => {
                    let what = if laid_out == text {
                        "unchanged"
                    } else {
                        "laid out"
                    };
                    note = join_notes(
                        &note,
                        &format!(
                            "Apple format {:.2} s, {what}",
                            started.elapsed().as_secs_f32()
                        ),
                    );
                    text = laid_out;
                }
                Err(why) => note = join_notes(&note, &format!("Apple format: {why}")),
            }
        }
        return (text, note);
    };
    let mut via = name.to_string();
    let mut failed = false;
    let mode = Mode {
        correct,
        layout: format,
    };
    let (mut text, mut note) = tidy_with(text, true, mode, |queries| {
        let sents: Vec<String> = queries.iter().map(|(sent, _)| sent.clone()).collect();
        match gemma(&model, &sents) {
            Ok((proposals, reused)) => {
                if reused > 0 {
                    via = format!("{name} ({reused} of {} from the live pass)", sents.len());
                }
                proposals.into_iter().map(|p| Ok((p, true))).collect()
            }
            Err(e) => {
                // Apple corrects only where its own check would have asked it.
                failed = true;
                via = format!("Apple ({name}: {e:#})");
                queries
                    .iter()
                    .map(|(sent, cue)| match correct && *cue {
                        true => crate::format::propose("correct", sent).map(|p| (p, false)),
                        false => Err(anyhow::anyhow!("{name} failed")),
                    })
                    .collect()
            }
        }
    });
    if failed
        && format
        && let Ok(laid_out) = crate::format::format(&text)
    {
        text = laid_out;
        note = join_notes(&note, "Apple laid out");
    }
    (text, format!("{via}: {note}"))
}

fn join_notes(a: &str, b: &str) -> String {
    match a.is_empty() {
        true => b.to_string(),
        false => format!("{a}, {b}"),
    }
}

/// What to take from Gemma's answer: its cuts, its line breaks, or both.
#[derive(Clone, Copy)]
struct Mode {
    correct: bool,
    layout: bool,
}

const CORRECT: Mode = Mode {
    correct: true,
    layout: false,
};

/// The text without self-corrections and filler words, and a note for the log (what the model
/// did, or why its proposal was not used).
pub fn correct(text: &str) -> (String, String) {
    let local = LOCAL.lock().unwrap().clone();
    let mut via = None;
    let (text, note) = correct_with(text, local.is_some(), |queries| {
        if let Some((name, model)) = &local {
            let sents: Vec<String> = queries.iter().map(|(sent, _)| sent.clone()).collect();
            match gemma(model, &sents) {
                Ok((proposals, reused)) => {
                    via = Some(match reused {
                        0 => name.to_string(),
                        n => format!("{name} ({n} of {} from the live pass)", sents.len()),
                    });
                    return proposals.into_iter().map(|p| Ok((p, true))).collect();
                }
                Err(e) => {
                    // Apple only where its own check would have asked it.
                    via = Some(format!("Apple where a cue is ({name}: {e:#})"));
                    return queries
                        .iter()
                        .map(|(sent, cue)| match cue {
                            true => crate::format::propose("correct", sent).map(|p| (p, false)),
                            false => Err(anyhow::anyhow!("{name} failed")),
                        })
                        .collect();
                }
            }
        }
        via = Some("Apple".into());
        queries
            .iter()
            .map(|(sent, _)| crate::format::propose("correct", sent).map(|p| (p, false)))
            .collect()
    });
    match via {
        Some(via) => (text, format!("{via}: {note}")),
        None => (text, note),
    }
}

/// Gemma's proposals for `sents` (texts as the model gets them), from the live pass's cache where
/// it has them; also how many came from there.
fn gemma(model: &Llm, sents: &[String]) -> anyhow::Result<(Vec<String>, usize)> {
    let cached: Vec<Option<String>> = {
        let cache = PROPOSALS.lock().unwrap();
        sents.iter().map(|s| cache.get(s).cloned()).collect()
    };
    let missing: Vec<String> = sents
        .iter()
        .zip(&cached)
        .filter(|(_, c)| c.is_none())
        .map(|(s, _)| format!("<text>{s}</text>"))
        .collect();
    let mut asked = match missing.is_empty() {
        true => Vec::new(),
        false => model.ask_all(&missing, TIMEOUT)?,
    }
    .into_iter();
    let mut cache = PROPOSALS.lock().unwrap();
    let reused = cached.iter().filter(|c| c.is_some()).count();
    let proposals = sents
        .iter()
        .zip(cached)
        .map(|(sent, c)| {
            c.unwrap_or_else(|| {
                let p = asked.next().unwrap_or_default();
                cache.insert(sent.clone(), p.clone());
                p
            })
        })
        .collect();
    Ok((proposals, reused))
}

// Tidying up while dictating: once a sentence of the live text is done, Gemma reads it in the
// background (one sentence at a time, never in the way of recording or the live text) and the
// overlay shows its cuts struck through. What gets pasted is still decided after the recording;
// that pass reuses a proposal when it gets the very same text. A sentence with words struck goes
// to it on its own (not in a piece with its neighbours), so it gets the very same text and what
// was struck is what goes: in a longer piece Gemma reads the words in context and may keep them.

/// Gemma's proposals by the text it got, from the live pass (and the final one).
static PROPOSALS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());
/// The words cut from each finished live sentence (indices of its whitespace-separated words).
static MARKS: Mutex<BTreeMap<String, Vec<usize>>> = Mutex::new(BTreeMap::new());
/// Live sentences Gemma cut words from (filler words aside), as the model got them.
static STRUCK: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// Counts recordings; sentences of an earlier one are dropped unread.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CLEANER: Mutex<Option<Sender<(u64, String)>>> = Mutex::new(None);

/// A recording starts: forget the last one's sentences.
pub fn live_begin() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    PROPOSALS.lock().unwrap().clear();
    MARKS.lock().unwrap().clear();
    STRUCK.lock().unwrap().clear();
}

/// The recording stopped: sentences still waiting are not read any more.
pub fn live_end() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Whether live tidying can run (Gemma chosen and started).
pub fn live_ready() -> bool {
    LOCAL.lock().unwrap().is_some()
}

/// Hands a finished live sentence to the background reader (once per sentence).
pub fn live_offer(sentence: &str) {
    let mut cleaner = CLEANER.lock().unwrap();
    let tx = cleaner.get_or_insert_with(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(u64, String)>();
        std::thread::Builder::new()
            .name("live-correct".into())
            .spawn(move || {
                for (generation, sentence) in rx {
                    if generation == GENERATION.load(Ordering::SeqCst) {
                        tidy(generation, &sentence);
                    }
                }
            })
            .expect("spawn live-correct");
        tx
    });
    let _ = tx.send((GENERATION.load(Ordering::SeqCst), sentence.to_string()));
}

fn tidy(generation: u64, sentence: &str) {
    let Some((_, model)) = LOCAL.lock().unwrap().clone() else {
        return;
    };
    let started = std::time::Instant::now();
    let (out, note) = correct_with(sentence, true, |queries| {
        let sents: Vec<String> = queries.iter().map(|(s, _)| s.clone()).collect();
        match gemma(&model, &sents) {
            Ok((proposals, _)) => proposals.into_iter().map(|p| Ok((p, true))).collect(),
            Err(e) => queries
                .iter()
                .map(|_| Err(anyhow::anyhow!("{e:#}")))
                .collect(),
        }
    });
    let cut = struck(sentence, &out);
    // One line per live sentence, so real dictations can be checked in the log.
    let shown = match generation == GENERATION.load(Ordering::SeqCst) {
        true => match cut.len() {
            0 => "nothing struck".to_string(),
            n => format!("{n} words struck"),
        },
        false => "too late, recording over".to_string(),
    };
    eprintln!(
        "live correct: {:.2} s, {shown} ({note}): {sentence}",
        started.elapsed().as_secs_f32()
    );
    if generation == GENERATION.load(Ordering::SeqCst) {
        // Gemma's cuts, not the filler words (those go by rule in any piece).
        let (tokens, _, _) = drop_fillers(sentence);
        if !sentence.contains('\n') && !struck(&tokens.join(" "), &out).is_empty() {
            STRUCK.lock().unwrap().insert(cue_text(&tokens).join(" "));
        }
        MARKS.lock().unwrap().insert(sentence.to_string(), cut);
    }
}

/// Whether the live pass struck words of this sentence (tokens as `drop_fillers` gives them).
fn struck_live(sentence: &[String]) -> bool {
    STRUCK
        .lock()
        .unwrap()
        .contains(&cue_text(sentence).join(" "))
}

/// The cut words of a finished live sentence, once Gemma has read it.
pub fn live_marks(sentence: &str) -> Option<Vec<usize>> {
    MARKS.lock().unwrap().get(sentence).cloned()
}

/// Which words of `input` are not in `output` (a deletion of it), by index.
fn struck(input: &str, output: &str) -> Vec<usize> {
    let words: Vec<String> = input.split_whitespace().map(norm).collect();
    let kept: Vec<String> = output.split_whitespace().map(norm).collect();
    let mut j = kept.len();
    let mut cut = Vec::new();
    for i in (0..words.len()).rev() {
        if words[i].is_empty() {
            continue;
        }
        if j > 0 && words[i] == kept[j - 1] {
            j -= 1;
        } else {
            cut.push(i);
        }
    }
    cut.reverse();
    cut
}

/// The finished sentences of a live text (each ends in . ! or ?), with their byte ranges; the
/// unfinished rest is left out.
pub fn sentences(text: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, word) in words_at(text) {
        let begin = *start.get_or_insert(at);
        if ends_sentence(trailing(word)) {
            out.push((begin..at + word.len(), &text[begin..at + word.len()]));
            start = None;
        }
    }
    out
}

/// The whitespace-separated words of `text` with their byte offsets.
pub fn words_at(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_whitespace()
        .map(move |w| (w.as_ptr() as usize - text.as_ptr() as usize, w))
}

/// A stretch of one line that gets its own proposal.
struct Piece {
    line: usize,
    tokens: Vec<String>,
    /// What the model gets (`cue_text`).
    sent: Vec<String>,
    pause: Vec<bool>,
    cue: bool,
    ask: bool,
}

/// About this many words go to Gemma at once: long dictations are cut at sentence ends into
/// pieces that run side by side (`llm::Llm::ask_all`), so the wait does not grow with the length.
const PIECE_WORDS: usize = 40;

/// The line's tokens split at sentence ends into runs of about `PIECE_WORDS` words. A sentence
/// `alone` says so is a piece of its own (one the live pass struck words of).
fn pieces_of(tokens: &[String], alone: impl Fn(&[String]) -> bool) -> Vec<std::ops::Range<usize>> {
    let mut sentences = Vec::new();
    let mut start = 0;
    for (i, token) in tokens.iter().enumerate() {
        if ends_sentence(trailing(token)) || i + 1 == tokens.len() {
            sentences.push(start..i + 1);
            start = i + 1;
        }
    }
    let words = |r: &std::ops::Range<usize>| {
        tokens[r.clone()]
            .iter()
            .filter(|t| !norm(t).is_empty())
            .count()
    };
    let mut pieces: Vec<std::ops::Range<usize>> = Vec::new();
    // Whether the last piece takes more sentences.
    let mut open = false;
    for sentence in sentences {
        let own = alone(&tokens[sentence.clone()]);
        match pieces.last_mut() {
            Some(last) if open && !own && words(last) + words(&sentence) <= PIECE_WORDS => {
                last.end = sentence.end
            }
            _ => pieces.push(sentence),
        }
        open = !own;
    }
    if pieces.is_empty() {
        pieces.push(0..0);
    }
    pieces
}

/// `text` corrected with proposals from `propose(queries)`: one answer per (text for the model,
/// has a cue), with the proposal and whether it is Gemma's (read loosely: no cue needed). With
/// `local` (Gemma) every piece of every line is asked, else only lines with a cue (Apple).
fn correct_with(
    text: &str,
    local: bool,
    propose: impl FnOnce(&[(String, bool)]) -> Vec<anyhow::Result<(String, bool)>>,
) -> (String, String) {
    tidy_with(text, local, CORRECT, propose)
}

/// `correct_with` taking what `mode` says from the answers: cuts (checked as there) and/or the
/// line breaks of Gemma's layout (carried onto the words that stay, `format::layout`). Text that
/// is already laid out with spoken commands keeps its lines.
fn tidy_with(
    text: &str,
    local: bool,
    mode: Mode,
    propose: impl FnOnce(&[(String, bool)]) -> Vec<anyhow::Result<(String, bool)>>,
) -> (String, String) {
    let spoken_layout = text.contains('\n');
    let (mut laid, mut unlaid) = (0, 0);
    let mut notes = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    for (line, text) in lines.iter().enumerate() {
        let (tokens, pause, fillers) = drop_fillers(text);
        if fillers > 0 {
            notes.push(format!("{fillers} filler words"));
        }
        let ranges = match local {
            true => pieces_of(&tokens, struck_live),
            false => std::iter::once(0..tokens.len()).collect(),
        };
        for range in ranges {
            let tokens = tokens[range.clone()].to_vec();
            let words = tokens.iter().filter(|t| !norm(t).is_empty()).count();
            let sent = cue_text(&tokens);
            let cue = has_cue(&sent);
            let ask = (MIN_WORDS..=MAX_WORDS).contains(&words) && (local || cue);
            pieces.push(Piece {
                line,
                tokens,
                sent,
                pause: pause[range].to_vec(),
                cue,
                ask,
            });
        }
    }
    let queries: Vec<(String, bool)> = pieces
        .iter()
        .filter(|p| p.ask)
        .map(|p| (p.sent.join(" "), p.cue))
        .collect();
    let mut answers = match queries.is_empty() {
        true => Vec::new(),
        false => propose(&queries),
    }
    .into_iter();
    let (mut cut, mut taken, mut refused_cuts, mut refused, mut asked) =
        (0, 0, Vec::new(), Vec::new(), false);
    let mut out: Vec<Vec<String>> = vec![Vec::new(); lines.len()];
    for piece in &pieces {
        let kept = piece.tokens.join(" ");
        let text = match piece.ask.then(|| answers.next()).flatten() {
            None => kept,
            Some(Err(e)) => {
                notes.push(format!("model: {e:#}"));
                kept
            }
            Some(Ok((proposal, loose))) => {
                asked = true;
                let base = match mode.correct {
                    false => kept,
                    true => match apply(&piece.tokens, &piece.sent, &piece.pause, &proposal, loose)
                    {
                        Ok(applied) => {
                            cut += applied.cut;
                            taken += applied.taken;
                            refused_cuts.extend(applied.refused);
                            applied.text
                        }
                        Err(why) => {
                            refused.push(format!("proposal not used: {why}"));
                            kept
                        }
                    },
                };
                match mode.layout && loose && !spoken_layout {
                    false => base,
                    true => match crate::format::layout(&base, &proposal) {
                        Some(text) => {
                            laid += (text != base) as usize;
                            text
                        }
                        None => {
                            unlaid += 1;
                            base
                        }
                    },
                }
            }
        };
        if !text.is_empty() {
            out[piece.line].push(text);
        }
    }
    if cut > 0 {
        notes.push(match taken {
            1 => format!("{cut} words cut"),
            n => format!("{cut} words cut in {n} places"),
        });
    }
    if !refused_cuts.is_empty() {
        let mut why = refused_cuts.clone();
        why.dedup();
        notes.push(format!(
            "{} {} put back: {}",
            refused_cuts.len(),
            if refused_cuts.len() == 1 {
                "cut"
            } else {
                "cuts"
            },
            why.join("; ")
        ));
    }
    notes.extend(refused.iter().cloned());
    if mode.correct && asked && cut == 0 && refused.is_empty() && refused_cuts.is_empty() {
        notes.push("nothing to correct".into());
    }
    if laid > 0 {
        notes.push(format!("laid out ({laid} of {} pieces)", laid + unlaid));
    }
    if unlaid > 0 {
        notes.push(format!("{unlaid} layouts not used (reworded)"));
    }
    if mode.layout && asked && laid == 0 && unlaid == 0 {
        notes.push("no layout".into());
    }
    if notes.is_empty() {
        notes.push("no correction cue".into());
    }
    // Pieces laid out on their own: one with lines or paragraphs is a paragraph of its own.
    let lines: Vec<String> = out
        .iter()
        .map(|l| match l.iter().any(|p| p.contains('\n')) {
            true => l.join("\n\n"),
            false => l.join(" "),
        })
        .collect();
    (lines.join("\n"), notes.join(", "))
}

/// One line of `eval/corrections.tsv`: a dictated text and the accepted results (`plain`), or
/// `None` when it must stay as it is.
pub struct Case {
    pub input: String,
    pub accepted: Option<Vec<String>>,
}

pub fn parse_cases(tsv: &str) -> Vec<Case> {
    tsv.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(input, accepted)| Case {
            input: input.to_string(),
            accepted: (accepted.trim() != "=").then(|| accepted.split('|').map(plain).collect()),
        })
        .collect()
}

/// Lower case, punctuation gone: how eval results are compared.
pub fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl Case {
    pub fn passes(&self, output: &str) -> bool {
        match &self.accepted {
            None => plain(output) == plain(&self.input),
            Some(accepted) => accepted.contains(&plain(output)),
        }
    }
}

/// Byte range of a token's word (first to last alphanumeric character).
fn core(token: &str) -> Option<(usize, usize)> {
    let start = token.find(char::is_alphanumeric)?;
    let (last, c) = token.char_indices().rfind(|(_, c)| c.is_alphanumeric())?;
    Some((start, last + c.len_utf8()))
}

fn trailing(token: &str) -> &str {
    core(token).map_or("", |(_, end)| &token[end..])
}

fn with_trailing(token: &str, punct: &str) -> String {
    match core(token) {
        Some((_, end)) => format!("{}{punct}", &token[..end]),
        None => token.to_string(),
    }
}

fn with_case(token: &str, upper: bool) -> String {
    let Some((start, _)) = core(token) else {
        return token.to_string();
    };
    let mut chars = token[start..].chars();
    let first = chars.next().unwrap();
    let first: String = match upper {
        true => first.to_uppercase().collect(),
        false => first.to_lowercase().collect(),
    };
    format!("{}{first}{}", &token[..start], chars.as_str())
}

fn ends_sentence(punct: &str) -> bool {
    punct.contains(['.', '!', '?'])
}

/// The line's tokens without filler words, whether a pause follows each (punctuation, or a filler
/// word that was there), and how many fillers went.
fn drop_fillers(line: &str) -> (Vec<String>, Vec<bool>, usize) {
    let words: Vec<&str> = line.split_whitespace().collect();
    let mut tokens: Vec<String> = Vec::with_capacity(words.len());
    let mut pause: Vec<bool> = Vec::with_capacity(words.len());
    let mut dropped = 0;
    // Set after a dropped filler: the next kept word starts a sentence.
    let mut capital = false;
    for word in words {
        if FILLERS.contains(&norm(word).as_str()) {
            dropped += 1;
            let end = trailing(word);
            match tokens.last_mut() {
                None => capital = true,
                Some(last) => {
                    *pause.last_mut().unwrap() = true;
                    // "Das war's, äh." → "Das war's."
                    if ends_sentence(end) && !ends_sentence(trailing(last)) {
                        *last = with_trailing(last, end);
                    }
                    capital = ends_sentence(trailing(last));
                }
            }
            continue;
        }
        let start = tokens.last().is_none_or(|t| ends_sentence(trailing(t)));
        tokens.push(match capital && start {
            true => with_case(word, true),
            false => word.to_string(),
        });
        pause.push(!trailing(word).is_empty() && core(word).is_some());
        capital = false;
    }
    (tokens, pause, dropped)
}

/// The tokens as the model gets them: a colloquial "ne" at a cue becomes "nein" (the model reads
/// "…, ne?" as a question tag otherwise). One token for one, so positions stay the same.
fn cue_text(tokens: &[String]) -> Vec<String> {
    let mut sent = tokens.to_vec();
    for i in 0..tokens.len() {
        let word = norm(&tokens[i]);
        let after_interjection = i > 0 && INTERJECTIONS.contains(&norm(&tokens[i - 1]).as_str());
        if NO.contains(&word.as_str()) && (after_interjection || !trailing(&tokens[i]).is_empty()) {
            let (start, end) = core(&tokens[i]).unwrap();
            sent[i] = format!("{}nein{}", &tokens[i][..start], &tokens[i][end..]);
        }
    }
    sent
}

fn has_cue(sent: &[String]) -> bool {
    let normed: Vec<String> = sent.iter().map(|t| norm(t)).collect();
    STRONG
        .iter()
        .any(|phrase| normed.windows(phrase.len()).any(|w| w == *phrase))
}

/// What `apply` made of a proposal.
#[derive(Debug, PartialEq)]
struct Applied {
    text: String,
    /// Words cut.
    cut: usize,
    /// Cuts taken.
    taken: usize,
    /// Why each refused cut was put back.
    refused: Vec<&'static str>,
}

/// Why a cut of `normed[start..end]` is no self-correction (`None`: it is one), and how many of its
/// words are retracted (the rest is the cue). `keep` says which words stay; `loose` reads Gemma's
/// proposals (no cue or pause needed: Parakeet mishears cues as "hinein", "Anni", "eine").
fn check_cut(
    normed: &[String],
    pause: &[bool],
    keep: &[bool],
    (start, end): (usize, usize),
    loose: bool,
) -> Result<usize, &'static str> {
    let cut = &normed[start..end];
    if cut.iter().all(|w| INTERJECTIONS.contains(&w.as_str())) {
        return Ok(0);
    }
    // The cue at the end of the cut, longest phrases first.
    let mut cue = end;
    let mut strong = false;
    'cue: while cue > start {
        for (phrases, is_strong) in [(STRONG, true), (WEAK, false)] {
            let best = phrases
                .iter()
                .filter(|p| p.len() <= cue - start && normed[cue - p.len()..cue] == ***p)
                .max_by_key(|p| p.len());
            if let Some(p) = best {
                cue -= p.len();
                strong |= is_strong;
                continue 'cue;
            }
        }
        break;
    }
    if loose {
        if strong && cue == start {
            return Err("it drops a correction cue but keeps what it corrects");
        }
        if cue - start > MAX_RETRACTED {
            return Err("it cuts too many words at once");
        }
        return Ok(cue - start);
    }
    if !strong {
        return Err("it cuts words that do not end in a correction cue");
    }
    if cue == start {
        return Err("it drops a correction cue but keeps what it corrects");
    }
    if !pause[cue - 1] {
        return Err("no pause before the correction cue");
    }
    if cue - start > MAX_RETRACTED {
        return Err("it cuts too many words before a cue");
    }
    if !keep[end..]
        .iter()
        .zip(&normed[end..])
        .any(|(k, w)| *k && !w.is_empty())
    {
        return Err("nothing follows the correction cue");
    }
    Ok(cue - start)
}

/// Gemma's cuts split at sentence ends: a whole sentence it cuts without a cue in it goes back
/// ("Insane gut geworden." is no slip), unless the next cut sentence starts with a strong cue
/// ("Der Termin ist am Dienstag. Nein, das ist falsch …"); then the two are one cut.
fn sentence_cuts(
    tokens: &[String],
    normed: &[String],
    cuts: Vec<(usize, usize)>,
) -> Vec<((usize, usize), Option<&'static str>)> {
    let ends = |i: usize| ends_sentence(trailing(&tokens[i]));
    let strong_at = |i: usize| {
        STRONG
            .iter()
            .any(|p| normed.get(i..i + p.len()).is_some_and(|w| w == *p))
    };
    let mut out = Vec::new();
    for (start, end) in cuts {
        let mut segments = Vec::new();
        let mut from = start;
        for i in start..end {
            if ends(i) || i + 1 == end {
                segments.push((from, i + 1));
                from = i + 1;
            }
        }
        // Where a run of segments merged into one cut starts.
        let mut open: Option<usize> = None;
        for (k, &(a, b)) in segments.iter().enumerate() {
            let whole = (a == 0 || ends(a - 1)) && ends(b - 1);
            let cue = (a..b).any(strong_at);
            if whole && !cue {
                match segments.get(k + 1) {
                    Some(&(next, _)) if strong_at(next) => {
                        open.get_or_insert(a);
                    }
                    _ => {
                        if let Some(o) = open.take() {
                            out.push(((o, a), None));
                        }
                        out.push(((a, b), Some("it cuts a whole sentence")));
                    }
                }
                continue;
            }
            out.push(((open.take().unwrap_or(a), b), None));
        }
        if let Some(o) = open {
            out.push(((o, end), None));
        }
    }
    out
}

/// `tokens` with the cuts of `proposal` that are self-corrections, each cut checked on its own (a
/// refused one is put back, the others stay taken). `Err` when the proposal is no deletion of the
/// text at all (added, changed or reordered words) or the cuts taken retract too much of it.
/// `sent` is what the model got (`cue_text`).
fn apply(
    tokens: &[String],
    sent: &[String],
    pause: &[bool],
    proposal: &str,
    loose: bool,
) -> Result<Applied, &'static str> {
    let normed: Vec<String> = sent.iter().map(|t| norm(t)).collect();
    // Gemma's answer may be laid out: list markers ("1.", "- ") are no words of the text.
    let proposed: Vec<&str> = proposal
        .lines()
        .flat_map(|line| {
            let line = line.trim();
            crate::format::list_marker(line)
                .map_or(line, |(_, rest)| rest)
                .split_whitespace()
        })
        .filter(|t| !norm(t).is_empty())
        .collect();

    // Match the proposal from the end, each word as late as it goes: a retraction comes before
    // its repair ("um drei, nein, um fünf" keeps the second "um").
    let mut keep = vec![false; tokens.len()];
    let mut source: Vec<Option<&str>> = vec![None; tokens.len()];
    let mut j = proposed.len();
    for i in (0..tokens.len()).rev() {
        if normed[i].is_empty() {
            keep[i] = true;
        } else if j > 0 && normed[i] == norm(proposed[j - 1]) {
            keep[i] = true;
            source[i] = Some(proposed[j - 1]);
            j -= 1;
        }
    }
    if j > 0 {
        return Err("it adds, changes or reorders words");
    }

    let mut cuts = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if keep[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < tokens.len() && !keep[i] {
            i += 1;
        }
        cuts.push((start, i));
    }
    let words = normed.iter().filter(|w| !w.is_empty()).count();
    let cuts = match loose {
        true => sentence_cuts(tokens, &normed, cuts),
        false => cuts.into_iter().map(|c| (c, None)).collect(),
    };
    let (mut retracted, mut taken, mut refused) = (0, 0, Vec::new());
    for (cut, verdict) in cuts {
        let verdict = match verdict {
            Some(why) => Err(why),
            None => check_cut(&normed, pause, &keep, cut, loose),
        };
        match verdict {
            Ok(n) => {
                retracted += n;
                taken += 1;
            }
            Err(why) => {
                refused.push(why);
                keep[cut.0..cut.1].iter_mut().for_each(|k| *k = true);
                // Its words come back as they were, the neighbours' punctuation too.
                let around = cut.0.saturating_sub(1)..(cut.1 + 1).min(tokens.len());
                source[around].iter_mut().for_each(|s| *s = None);
            }
        }
    }
    if retracted as f32 > MAX_SHARE * words as f32 {
        return Err("it cuts too large a share of the text");
    }

    // The kept words; around a cut the punctuation before it and the capital after it follow the
    // proposal.
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut cut = 0;
    let mut after_cut = false;
    let mut last_kept: Option<usize> = None;
    for i in 0..tokens.len() {
        if !keep[i] {
            if !normed[i].is_empty() {
                cut += 1;
            }
            if let (Some(k), false) = (last_kept, after_cut)
                && let Some(proposed) = source[k]
            {
                let n = out.len() - 1;
                out[n] = with_trailing(&out[n], trailing(proposed));
            }
            after_cut = true;
            continue;
        }
        let mut token = tokens[i].clone();
        if after_cut && let Some(proposed) = source[i] {
            let upper = core(proposed)
                .and_then(|(s, _)| proposed[s..].chars().next())
                .is_some_and(char::is_uppercase);
            token = with_case(&token, upper);
        }
        after_cut = false;
        last_kept = Some(i);
        out.push(token);
    }
    Ok(Applied {
        text: out.join(" "),
        cut,
        taken,
        refused,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` corrected with a fixed model proposal (`None`: the model must not be asked).
    fn run(text: &str, proposal: Option<&str>) -> (String, String) {
        correct_with(text, false, |queries| {
            queries
                .iter()
                .map(|(sent, _)| match proposal {
                    Some(p) => Ok((p.to_string(), false)),
                    None => panic!("model asked for {sent:?}"),
                })
                .collect()
        })
    }

    /// As Gemma: every piece asked (one proposal each, in order), cuts read loosely.
    fn run_local(text: &str, proposals: &[&str]) -> (String, String) {
        correct_with(text, true, |queries| {
            assert_eq!(queries.len(), proposals.len(), "{queries:?}");
            proposals
                .iter()
                .map(|p| Ok((p.to_string(), true)))
                .collect()
        })
    }

    // Real proposals of Apple's model (macOS 26.5, `dictate-ane correct`) for these transcripts.

    #[test]
    fn takes_self_corrections() {
        assert_eq!(
            run(
                "Wir treffen uns um drei, äh nein, um fünf Uhr am Bahnhof und bringen die Unterlagen mit.",
                Some("Wir treffen uns um fünf Uhr am Bahnhof und bringen die Unterlagen mit.")
            )
            .0,
            "Wir treffen uns um fünf Uhr am Bahnhof und bringen die Unterlagen mit."
        );
        assert_eq!(
            run(
                "Schick das bitte an Kai, sorry, ich meine an Jonas, der kümmert sich darum.",
                Some("Schick das bitte an Jonas, der kümmert sich darum.")
            )
            .0,
            "Schick das bitte an Jonas, der kümmert sich darum."
        );
        assert_eq!(
            run(
                "Wir brauchen zwei, Moment, drei Leute für das Projekt im Januar.",
                Some("Wir brauchen drei Leute für das Projekt im Januar.")
            )
            .0,
            "Wir brauchen drei Leute für das Projekt im Januar."
        );
        assert_eq!(
            run(
                "Ich nehme den Zug, ah nee, doch lieber das Auto, weil es schneller ist.",
                Some("Ich nehme das Auto, weil es schneller ist.")
            )
            .0,
            "Ich nehme das Auto, weil es schneller ist."
        );
        // A whole retracted sentence, and the sentence after it starts with its capital.
        assert_eq!(
            run(
                "Der Termin ist am Dienstag. Nein, das ist falsch, ich habe mich vertan. Der Termin ist am Mittwoch um zehn.",
                Some("Der Termin ist am Mittwoch um zehn.")
            ),
            (
                "Der Termin ist am Mittwoch um zehn.".into(),
                "13 words cut".into()
            )
        );
        // A restart: the new sentence keeps the capital the model gave it.
        assert_eq!(
            run(
                "Also ich sage jetzt zwei Tickets, ah ne, nimm vier Tickets und schließe sie danach.",
                Some("Nimm vier Tickets und schließe sie danach.")
            )
            .0,
            "Nimm vier Tickets und schließe sie danach."
        );
    }

    #[test]
    fn numbers_stay_as_parakeet_wrote_them() {
        assert_eq!(
            run(
                "Das Budget liegt bei 5000 Euro, nein, bei 6000 Euro für das ganze Jahr.",
                Some("Das Budget liegt bei 6000 Euro für das ganze Jahr.")
            )
            .0,
            "Das Budget liegt bei 6000 Euro für das ganze Jahr."
        );
        // The model writing a number differently is not a deletion.
        assert!(
            run(
                "Wir brauchen zwei, Moment, drei Leute für das Projekt.",
                Some("Wir brauchen 3 Leute für das Projekt.")
            )
            .1
            .contains("not used")
        );
    }

    #[test]
    fn answers_and_rewrites_are_not_used() {
        let question = "Was ist eigentlich die Hauptstadt von Frankreich? Nein, das wollte ich Kai noch fragen.";
        assert_eq!(
            run(question, Some("Die Hauptstadt von Frankreich ist Paris.")),
            (
                question.into(),
                "proposal not used: it adds, changes or reorders words".into()
            )
        );
        let mail = "Schreib mir bitte eine Mail an Kai, sorry, an Jonas, dass das Treffen morgen ausfällt.";
        assert_eq!(
            run(
                mail,
                Some(
                    "Hallo Jonas,\n\ndas Treffen morgen fällt aus. Ich hoffe, es geht dir gut.\n\nLiebe Grüße,\n\n\\[Dein Name]"
                )
            )
            .0,
            mail
        );
        // Right in meaning, but reordered.
        let ticket = "Schreib bitte in das Ticket, dass der Fehler beim Login auftritt, äh, nein, beim Logout.";
        assert_eq!(
            run(
                ticket,
                Some("Schreib bitte in das Ticket, dass der Fehler beim Logout auftritt.")
            )
            .0,
            "Schreib bitte in das Ticket, dass der Fehler beim Login auftritt, nein, beim Logout."
        );
    }

    #[test]
    fn wrong_cuts_are_not_used() {
        // The model kept the wrong half ("…, ne?" read as a question tag).
        let week = "Wir machen das nächste Woche, ne, übernächste Woche, weil Kai Urlaub hat.";
        assert_eq!(
            run(
                week,
                Some("Wir machen das nächste Woche, weil Kai Urlaub hat.")
            ),
            (
                week.into(),
                "1 cut put back: it cuts words that do not end in a correction cue".into()
            )
        );
        // Words after the cue are the correction: they stay.
        let steps =
            "Also ich sage jetzt drei Schritte, ah ne, mach fünf Schritte und bearbeite diese.";
        assert_eq!(
            run(
                steps,
                Some("Also ich sage jetzt fünf Schritte und bearbeite diese.")
            )
            .0,
            steps
        );
        // The cue alone goes, what it corrects stays: "zwei, drei Leute" says something else.
        assert_eq!(
            run(
                "Wir brauchen zwei, Moment, drei Leute für das Projekt im Januar.",
                Some("Wir brauchen zwei, drei Leute für das Projekt im Januar.")
            )
            .1,
            "1 cut put back: it drops a correction cue but keeps what it corrects"
        );
        // A real sentence that happens to end in "nein" is no correction.
        let answer = "Ich habe Kai gefragt, er sagte nein. Dann haben wir es gelassen.";
        assert_eq!(
            run(answer, Some("Dann haben wir es gelassen.")).1,
            "1 cut put back: no pause before the correction cue"
        );
        // Nor is a whole sentence before a cue that leaves nothing after it.
        let end = "Das Angebot geht heute raus. Wir warten auf die Zahlen von Kai. Nein.";
        assert_eq!(run(end, Some("Das Angebot geht heute raus.")).0, end);
        // Too much at once.
        let long = "Wir machen das so, dass wir erst die Daten laden, dann die Modelle trainieren und am Ende alles prüfen, nein, wir prüfen zuerst.";
        assert_eq!(
            run(long, Some("Wir prüfen zuerst.")).1,
            "1 cut put back: it cuts too many words before a cue"
        );
    }

    #[test]
    fn text_without_a_cue_skips_the_model() {
        let text =
            "Kannst du den Bug mit dem Scrollen fixen? Der tritt auf, wenn man viele Sessions hat.";
        assert_eq!(run(text, None), (text.into(), "no correction cue".into()));
        // "ne" without a pause is "eine", not a cue.
        let ne = "Hast du ne Idee, wie wir das Angebot für Müller machen?";
        assert_eq!(run(ne, None).0, ne);
    }

    #[test]
    fn filler_words_go_by_rule() {
        assert_eq!(
            run(
                "Ähm, ich glaube, wir sollten das Angebot für Müller noch mal anschauen, äh, bevor wir es rausschicken.",
                None
            ),
            (
                "Ich glaube, wir sollten das Angebot für Müller noch mal anschauen, bevor wir es rausschicken.".into(),
                "2 filler words".into()
            )
        );
        assert_eq!(run("Das war es dann, äh.", None).0, "Das war es dann.");
        assert_eq!(
            run("Gut so. Äh, wir machen weiter.", None).0,
            "Gut so. Wir machen weiter."
        );
        // A dropped filler counts as a pause before the cue.
        assert_eq!(
            run(
                "Wir treffen uns um drei äh nein um fünf Uhr am Bahnhof.",
                Some("Wir treffen uns um fünf Uhr am Bahnhof.")
            )
            .0,
            "Wir treffen uns um fünf Uhr am Bahnhof."
        );
    }

    // Real proposals of Gemma 4 E4B (Q4_K_M, llama.cpp) for live dictations with misheard cues.

    #[test]
    fn gemma_cuts_without_a_cue() {
        assert_eq!(
            run_local(
                "Wir treffen uns um drei, hinein um fünf Uhr.",
                &["Wir treffen uns um fünf Uhr."]
            ),
            ("Wir treffen uns um fünf Uhr.".into(), "3 words cut".into())
        );
        assert_eq!(
            run_local(
                "Okay, mache drei, Anni fünf Bewertungen.",
                &["Okay, mache fünf Bewertungen."]
            )
            .0,
            "Okay, mache fünf Bewertungen."
        );
        assert_eq!(
            run_local(
                "Okay, kannst du drei oder nee sogar fünf Bewertungen machen?",
                &["Kannst du sogar fünf Bewertungen machen?"]
            )
            .0,
            "Kannst du sogar fünf Bewertungen machen?"
        );
        // Gemma is asked without a cue; Apple is not.
        assert_eq!(
            run_local(
                "Ich gehe hinein und hole den Schlüssel.",
                &["Ich gehe hinein und hole den Schlüssel."]
            )
            .1,
            "nothing to correct"
        );
        assert_eq!(
            run("Wir treffen uns um drei, hinein um fünf Uhr.", None).1,
            "no correction cue"
        );
    }

    #[test]
    fn gemma_cuts_stay_bounded() {
        // A whole sentence of 25 words without a cue is no self-correction (as Gemma proposed it).
        let bike = "Also nicht das Foto auf dem Fahrrad und auch nicht das am Fluss, das vom Markt gefällt mir am besten. Die anderen beiden nehmen wir vielleicht noch für die Website.";
        assert_eq!(
            run_local(
                bike,
                &["Die anderen beiden nehmen wir vielleicht noch für die Website."]
            ),
            (
                bike.into(),
                "1 cut put back: it cuts a whole sentence".into()
            )
        );
        // Only the cue goes, what it corrects stays: "zwei, drei Leute" says something else.
        assert_eq!(
            run_local(
                "Wir brauchen zwei, Moment, drei Leute für das Projekt.",
                &["Wir brauchen zwei, drei Leute für das Projekt."]
            )
            .1,
            "1 cut put back: it drops a correction cue but keeps what it corrects"
        );
        // Words of its own are never taken.
        assert_eq!(
            run_local(
                "Was ist eigentlich die Hauptstadt von Frankreich, weißt du das?",
                &["Die Hauptstadt von Frankreich ist Paris."]
            )
            .1,
            "proposal not used: it adds, changes or reorders words"
        );
        // At most half of a piece.
        assert_eq!(
            run_local("Ja, gut, das machen wir dann so.", &["So."]).1,
            "proposal not used: it cuts too large a share of the text"
        );
    }

    /// As Gemma with Formatieren (+ Korrigieren): one answer per piece, cuts and/or line breaks.
    fn run_tidy(text: &str, correct: bool, proposals: &[&str]) -> (String, String) {
        let mode = Mode {
            correct,
            layout: true,
        };
        tidy_with(text, true, mode, |queries| {
            assert_eq!(queries.len(), proposals.len(), "{queries:?}");
            proposals
                .iter()
                .map(|p| Ok((p.to_string(), true)))
                .collect()
        })
    }

    #[test]
    fn one_gemma_answer_corrects_and_lays_out() {
        // A real answer of Gemma 4 E4B (lmstudio Q4_K_M) with the layout instructions.
        let order = "Für morgen brauchen wir drei Sachen erstens das Angebot für Müller zweitens die Folien für das Board und drittens noch den Termin mit Kai festmachen. Danach können wir über das Budget reden.";
        let answer = "Für morgen brauchen wir drei Sachen:\n1. das Angebot für Müller\n2. die Folien für das Board\n3. den Termin mit Kai festmachen.\n\nDanach können wir über das Budget reden.";
        let laid_out = "Für morgen brauchen wir drei Sachen:\n1. Das Angebot für Müller\n2. Die Folien für das Board\n3. Den Termin mit Kai festmachen\n\nDanach können wir über das Budget reden.";
        assert_eq!(
            run_tidy(order, true, &[answer]),
            (
                laid_out.into(),
                "5 words cut in 3 places, laid out (1 of 1 pieces)".into()
            )
        );
        // Formatieren alone takes the breaks, no cut: "noch" stays.
        let (text, note) = run_tidy(order, false, &[answer]);
        assert!(text.contains("3. Noch den Termin"), "{text}");
        assert_eq!(note, "laid out (1 of 1 pieces)");
        // A correction and a mail layout in one answer.
        assert_eq!(
            run_tidy(
                "Hallo Kai, wir treffen uns um drei, hinein um fünf am Bahnhof. Viele Grüße, Thomas",
                true,
                &["Hallo Kai,\n\nwir treffen uns um fünf am Bahnhof.\n\nViele Grüße,\nThomas"]
            )
            .0,
            "Hallo Kai,\n\nwir treffen uns um fünf am Bahnhof.\n\nViele Grüße,\nThomas"
        );
        // Text laid out with spoken commands keeps its lines.
        assert_eq!(
            run_tidy(
                "Einkauf:\n- Milch, sorry, Hafermilch und Eier",
                true,
                &["Hafermilch und Eier\n\nnoch mehr"]
            )
            .0,
            "Einkauf:\n- Milch, sorry, Hafermilch und Eier"
        );
    }

    #[test]
    fn gemma_does_not_cut_whole_sentences_without_a_cue() {
        // Proposals as Gemma 4 E4B (lmstudio Q4_K_M) made them.
        let praise =
            "Richtig gut geworden. Einziges Problem, die Szene am Anfang, die ist noch zu lang.";
        assert_eq!(
            run_local(
                praise,
                &["Einziges Problem, die Szene am Anfang, die ist noch zu lang."]
            ),
            (
                praise.into(),
                "1 cut put back: it cuts a whole sentence".into()
            )
        );
        assert_eq!(
            run_local(
                "Bei mir kommt keine Nachbearbeitung. Also ich sage jetzt drei Schritte, ah ne, mach fünf Schritte und bearbeite diese.",
                &["Mach fünf Schritte und bearbeite diese."]
            ),
            (
                "Bei mir kommt keine Nachbearbeitung. Mach fünf Schritte und bearbeite diese."
                    .into(),
                "8 words cut, 1 cut put back: it cuts a whole sentence".into()
            )
        );
        // A sentence the next one takes back is one cut.
        assert_eq!(
            run_local(
                "Der Termin ist am Dienstag. Nein, das ist falsch, ich habe mich vertan. Der Termin ist am Mittwoch um zehn.",
                &["Der Termin ist am Mittwoch um zehn."]
            )
            .0,
            "Der Termin ist am Mittwoch um zehn."
        );
    }

    #[test]
    fn each_cut_is_checked_on_its_own() {
        // Gemma cuts a real correction and, in the same piece, a whole clause: the correction is
        // taken, the clause put back.
        let text = "Wir treffen uns um drei, hinein um fünf Uhr am Bahnhof, und dann gehen wir alle zusammen noch in das kleine Café an der Ecke beim Markt.";
        assert_eq!(
            run_local(text, &["Wir treffen uns um fünf Uhr am Bahnhof."]),
            (
                "Wir treffen uns um fünf Uhr am Bahnhof, und dann gehen wir alle zusammen noch in das kleine Café an der Ecke beim Markt.".into(),
                "3 words cut, 1 cut put back: it cuts too many words at once".into()
            )
        );
        // Apple's strict reading too: the cue-less cut goes back, the real one stays.
        assert_eq!(
            run(
                "Wir treffen uns um drei, äh nein, um fünf Uhr, also am Bahnhof.",
                Some("Wir treffen uns um fünf Uhr am Bahnhof.")
            ),
            (
                "Wir treffen uns um fünf Uhr, also am Bahnhof.".into(),
                "1 filler words, 3 words cut, 1 cut put back: it cuts words that do not end in a correction cue".into()
            )
        );
    }

    #[test]
    fn live_text_gives_finished_sentences_and_struck_words() {
        let text = "Wir treffen uns um drei, hinein um fünf Uhr. Kommst du? Und dann";
        let found: Vec<_> = sentences(text)
            .into_iter()
            .map(|(r, s)| (r, s.to_string()))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    0..45,
                    "Wir treffen uns um drei, hinein um fünf Uhr.".to_string()
                ),
                (46..56, "Kommst du?".to_string())
            ]
        );
        assert_eq!(
            struck(
                "Wir treffen uns um drei, hinein um fünf Uhr.",
                "Wir treffen uns um fünf Uhr."
            ),
            vec![3, 4, 5]
        );
        assert_eq!(struck("Ähm, ich glaube, ja.", "Ich glaube, ja."), vec![0]);
    }

    #[test]
    fn long_dictations_go_in_pieces_side_by_side() {
        let first = "Wir haben heute lange über das Angebot gesprochen, und am Ende waren wir uns einig, dass wir die Preise anpassen.";
        let second = "Den Kunden schicken wir bis Freitag, äh nein, bis Donnerstag eine Antwort, damit sie planen können, und dann reden wir über den Rest.";
        let text = format!("{first} {second}");
        assert!(text.split_whitespace().count() > PIECE_WORDS);
        assert_eq!(
            run_local(
                &text,
                &[
                    first,
                    "Den Kunden schicken wir bis Donnerstag eine Antwort, damit sie planen können, und dann reden wir über den Rest."
                ]
            ),
            (
                format!(
                    "{first} Den Kunden schicken wir bis Donnerstag eine Antwort, damit sie planen können, und dann reden wir über den Rest."
                ),
                "1 filler words, 3 words cut".into()
            )
        );
        // Short sentences stay together, so a correction across them is seen whole.
        let tokens: Vec<String> = "Der Termin ist am Dienstag. Nein, das ist falsch. Am Mittwoch."
            .split_whitespace()
            .map(String::from)
            .collect();
        assert_eq!(pieces_of(&tokens, |_| false), vec![0..tokens.len()]);
        // One the live pass struck words of goes on its own, so it gets the same answer again.
        let alone = |s: &[String]| s[0] == "Nein,";
        assert_eq!(pieces_of(&tokens, alone), vec![0..5, 5..9, 9..11]);
    }

    #[test]
    fn a_sentence_struck_live_is_asked_on_its_own() {
        let struck = "Also das gehen wir mit dem Vermieter durch.";
        let (tokens, _, _) = drop_fillers(struck);
        STRUCK.lock().unwrap().insert(cue_text(&tokens).join(" "));
        let text = format!(
            "Wie schwierig ist es, so einen Antrag zu stellen? {struck} Oder wie geht das?"
        );
        // Without it the three would be one piece; Gemma's answer for it alone is the live one.
        let out = run_local(
            &text,
            &[
                "Wie schwierig ist es, so einen Antrag zu stellen?",
                "Das gehen wir mit dem Vermieter durch.",
                "Oder wie geht das?",
            ],
        );
        STRUCK.lock().unwrap().remove(&cue_text(&tokens).join(" "));
        assert_eq!(
            out.0,
            "Wie schwierig ist es, so einen Antrag zu stellen? Das gehen wir mit dem Vermieter durch. Oder wie geht das?"
        );
    }

    #[test]
    fn the_eval_set_parses() {
        let cases = parse_cases(include_str!("../eval/corrections.tsv"));
        assert_eq!(cases.len(), 43);
        assert_eq!(cases.iter().filter(|c| c.accepted.is_none()).count(), 17);
        let bus = cases
            .iter()
            .find(|c| c.input.starts_with("Der Bus"))
            .unwrap();
        assert!(bus.passes("Der Bus kommt um neun Uhr an der Haltestelle."));
        assert!(!bus.passes(&bus.input));
        let question = cases
            .iter()
            .find(|c| c.input.starts_with("Was ist"))
            .unwrap();
        assert!(question.passes(&question.input));
    }

    #[test]
    fn apple_and_gemma_get_the_same_instructions() {
        let swift = include_str!("../ane/Sources/dictate-ane/main.swift");
        let start = swift.find("let correctInstructions = \"\"\"\n").unwrap();
        let body = &swift[start..];
        let body = &body[body.find('\n').unwrap() + 1..];
        let body = &body[..body.find("\n\"\"\"").unwrap()];
        assert_eq!(body, PROMPT.trim_end());
    }

    #[test]
    fn the_model_gets_nein_for_a_colloquial_no() {
        let (tokens, _, _) = drop_fillers("drei Schritte, ah ne, mach fünf Schritte");
        assert_eq!(
            cue_text(&tokens).join(" "),
            "drei Schritte, ah nein, mach fünf Schritte"
        );
    }

    #[test]
    fn spoken_layout_lines_are_corrected_one_by_one() {
        assert_eq!(
            run(
                "Einkauf:\n- Milch, sorry, Hafermilch und Eier\n- Brot",
                Some("- Hafermilch und Eier")
            )
            .0,
            "Einkauf:\n- Hafermilch und Eier\n- Brot"
        );
    }
}
