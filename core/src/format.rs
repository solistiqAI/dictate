//! "Formatieren", the platform-free half: carrying a language model's layout onto the transcript.
//! The model call (Apple's on-device model via `dictate-ane format`, macOS 26 with Apple
//! Intelligence) is the app's (`format.rs` in the `dictate` crate).
//!
//! The model only proposes. Left alone it answers a dictated question ("Paris"), writes the mail
//! it was asked to write, or swaps names, so its words are never pasted: the proposal is aligned
//! word by word with the transcript, and only its layout (line breaks, blank lines, list markers)
//! is carried over onto the transcript's own words. A proposal that shares too few words with the
//! transcript is dropped, and the transcript goes in as it is.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Break {
    Line,
    Paragraph,
    Bullet,
    Numbered,
}

/// Words that count off list items; they go when the list does it ("erstens" → "1.").
const ENUMERATORS: &[&str] = &[
    "erstens",
    "zweitens",
    "drittens",
    "viertens",
    "fünftens",
    "sechstens",
    "siebtens",
    "achtens",
    "neuntens",
    "zehntens",
    "firstly",
    "secondly",
    "thirdly",
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
];

/// "und drittens", "and third": the conjunction goes with the enumerator.
const CONJUNCTIONS: &[&str] = &["und", "and", "sowie"];

pub fn norm(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_enumerator(word: &str) -> bool {
    ENUMERATORS.contains(&norm(word).as_str())
}

/// The proposal's words, each with the break the proposal puts before it.
fn proposal_words(proposal: &str) -> Vec<(String, Option<Break>)> {
    let mut words = Vec::new();
    let mut blank = false;
    for line in proposal.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank = true;
            continue;
        }
        let (kind, rest) = match list_marker(line) {
            Some((kind, rest)) => (Some(kind), rest),
            None if words.is_empty() => (None, line),
            None if blank => (Some(Break::Paragraph), line),
            None => (Some(Break::Line), line),
        };
        blank = false;
        let mut first = true;
        for word in rest.split_whitespace() {
            let word = norm(word);
            if word.is_empty() {
                continue;
            }
            words.push((word, if first { kind } else { None }));
            first = false;
        }
    }
    words
}

/// "- x", "* x", "• x" → bullet; "1. x", "2) x" → numbered.
pub fn list_marker(line: &str) -> Option<(Break, &str)> {
    for marker in ["- ", "* ", "• ", "– "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some((Break::Bullet, rest.trim_start()));
        }
    }
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let rest = &line[digits..];
    match (digits, rest.strip_prefix(". ").or(rest.strip_prefix(") "))) {
        (1..=2, Some(rest)) => Some((Break::Numbered, rest.trim_start())),
        _ => None,
    }
}

/// Longest common subsequence of two word lists: the matched index pairs, in order.
fn align(a: &[String], b: &[String]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[at(i, j)] = match a[i] == b[j] {
                true => dp[at(i + 1, j + 1)] + 1,
                false => dp[at(i + 1, j)].max(dp[at(i, j + 1)]),
            };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[at(i + 1, j)] >= dp[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

/// `original` with the layout of `proposal`, `None` when the proposal is not the same text.
pub fn layout(original: &str, proposal: &str) -> Option<String> {
    let words: Vec<&str> = original.split_whitespace().collect();
    let normed: Vec<String> = words.iter().map(|w| norm(w)).collect();
    let proposed = proposal_words(proposal);
    let proposed_words: Vec<String> = proposed.iter().map(|(w, _)| w.clone()).collect();
    let pairs = align(&normed, &proposed_words);

    // The same text? Enumerators the model dropped do not count against it.
    let counted = normed
        .iter()
        .filter(|w| !w.is_empty() && !ENUMERATORS.contains(&w.as_str()))
        .count();
    let matched = pairs
        .iter()
        .filter(|(i, _)| !ENUMERATORS.contains(&normed[*i].as_str()))
        .count();
    let invented = proposed.len() - pairs.len();
    if counted == 0
        || (matched as f32) < 0.85 * counted as f32
        || (invented as f32) > 0.15 * proposed.len().max(1) as f32
    {
        return None;
    }

    let mut matched_at = vec![false; words.len()];
    for (i, _) in &pairs {
        matched_at[*i] = true;
    }
    // Each proposed line break goes before the transcript word its line starts with.
    let mut breaks: Vec<Option<Break>> = vec![None; words.len()];
    let mut line_kind: Option<Break> = None;
    for (j, (_, kind)) in proposed.iter().enumerate() {
        if kind.is_some() {
            line_kind = *kind;
        }
        let Some(kind) = line_kind else { continue };
        if let Some(&(i, _)) = pairs.iter().find(|(_, pj)| *pj == j) {
            // An enumerator the model dropped belongs to the new line, not the end of the last:
            // the line starts at the first conjunction or enumerator of the words it dropped
            // before it ("und drittens noch den Termin" when it also dropped "noch").
            let mut i = i;
            let mut run = i;
            while run > 0 && !matched_at[run - 1] {
                run -= 1;
            }
            let counted = (run..i).any(|k| ENUMERATORS.contains(&normed[k].as_str()));
            match (run..i).find(|&k| droppable(&normed, k)) {
                Some(k) if counted => i = k,
                _ => {
                    while i > 0 && !matched_at[i - 1] && droppable(&normed, i - 1) {
                        i -= 1;
                    }
                }
            }
            if breaks[i].is_none() && i > 0 {
                breaks[i] = Some(kind);
            }
            line_kind = None;
        }
    }
    if breaks.iter().all(Option::is_none) {
        return Some(original.to_string());
    }

    let mut out = String::with_capacity(original.len() + 32);
    let mut number = 0;
    let mut in_list = false;
    let mut i = 0;
    while i < words.len() {
        let mut word = words[i];
        match breaks[i] {
            None => {
                if !out.is_empty() && !out.ends_with(['\n', ' ']) {
                    out.push(' ');
                }
            }
            Some(mut kind) => {
                // "zweitens das …", "und drittens den …", "und die …": the list item says it.
                let list = matches!(kind, Break::Bullet | Break::Numbered);
                while list && i + 1 < words.len() {
                    if is_enumerator(word) {
                        kind = Break::Numbered;
                    } else if !(CONJUNCTIONS.contains(&normed[i].as_str())
                        && (!matched_at[i] || is_enumerator(words[i + 1])))
                    {
                        break;
                    }
                    i += 1;
                    word = words[i];
                }
                end_line(&mut out, in_list, list);
                // After "Hallo Kai," the next line goes on in lower case.
                let capital = !out.ends_with(',');
                if list && !in_list && out.ends_with(char::is_alphanumeric) {
                    out.push(':');
                }
                number = if kind == Break::Numbered {
                    number + 1
                } else {
                    0
                };
                match kind {
                    Break::Line => out.push('\n'),
                    Break::Paragraph => out.push_str("\n\n"),
                    Break::Bullet => out.push_str("\n- "),
                    Break::Numbered => out.push_str(&format!("\n{number}. ")),
                }
                in_list = list;
                let mut chars = word.chars();
                match (capital, chars.next()) {
                    (true, Some(first)) => {
                        out.extend(first.to_uppercase());
                        out.push_str(chars.as_str());
                    }
                    _ => out.push_str(word),
                }
                i += 1;
                continue;
            }
        }
        out.push_str(word);
        i += 1;
    }
    end_line(&mut out, in_list, false);
    Some(out)
}

/// A transcript word the model left out that belongs to the next list item ("zweitens", "und").
fn droppable(normed: &[String], i: usize) -> bool {
    let word = normed[i].as_str();
    ENUMERATORS.contains(&word) || CONJUNCTIONS.contains(&word)
}

/// Before a break: no comma or semicolon before a list item or after one ("Hallo Kai," keeps its
/// comma), no period after a list item.
fn end_line(out: &mut String, in_list: bool, list: bool) {
    out.truncate(out.trim_end().len());
    if in_list || list {
        out.truncate(out.trim_end_matches([',', ';']).len());
    }
    if in_list && out.ends_with('.') && !out.ends_with("..") {
        out.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: &str = "Für morgen brauchen wir drei Sachen erstens das Angebot für Müller zweitens die Folien für das Board und drittens noch den Termin mit Kai festmachen. Danach können wir über das Budget reden.";

    #[test]
    fn takes_the_layout_onto_the_transcript() {
        // Real proposals of Apple's model (macOS 26.5) for these transcripts.
        assert_eq!(
            layout(
                ORDER,
                "Für morgen brauchen wir drei Sachen:\n\n*   Erstens das Angebot für Müller\n*   Zweitens die Folien für das Board\n*   Drittens noch den Termin mit Kai festmachen\n\nDanach können wir über das Budget reden."
            )
            .unwrap(),
            "Für morgen brauchen wir drei Sachen:\n1. Das Angebot für Müller\n2. Die Folien für das Board\n3. Noch den Termin mit Kai festmachen\n\nDanach können wir über das Budget reden."
        );
        let mail = "Hallo Kai, danke für das Treffen gestern. Ich habe mir die Punkte angeschaut: das Budget passt, beim Personal brauchen wir noch eine Person mehr, und die Termine schieben wir um eine Woche. Viele Grüße, Thomas";
        assert_eq!(
            layout(
                mail,
                "Hallo Kai,\n\ndanke für das Treffen gestern.\n\nIch habe mir die Punkte angeschaut:\n\n*   das Budget passt\n*   beim Personal brauchen wir noch eine Person mehr\n*   die Termine schieben wir um eine Woche\n\nViele Grüße,\n\nThomas"
            )
            .unwrap(),
            "Hallo Kai,\n\ndanke für das Treffen gestern.\n\nIch habe mir die Punkte angeschaut:\n- Das Budget passt\n- Beim Personal brauchen wir noch eine Person mehr\n- Die Termine schieben wir um eine Woche\n\nViele Grüße,\n\nThomas"
        );
    }

    #[test]
    fn dropped_enumerators_start_the_item() {
        assert_eq!(
            layout(
                ORDER,
                "Für morgen brauchen wir drei Sachen:\n\n1. das Angebot für Müller\n2. die Folien für das Board\n3. noch den Termin mit Kai festmachen\n\nDanach können wir über das Budget reden."
            )
            .unwrap(),
            "Für morgen brauchen wir drei Sachen:\n1. Das Angebot für Müller\n2. Die Folien für das Board\n3. Noch den Termin mit Kai festmachen\n\nDanach können wir über das Budget reden."
        );
    }

    #[test]
    fn answers_and_rewrites_are_dropped() {
        assert_eq!(
            layout(
                "Was ist eigentlich die Hauptstadt von Frankreich? Das wollte ich Kai noch fragen.",
                "Die Hauptstadt von Frankreich ist Paris."
            ),
            None
        );
        assert_eq!(
            layout(
                "Schreib mir bitte eine Mail an Kai, dass das Treffen morgen ausfällt.",
                "Betreff: Treffen morgen abgesagt\n\nHallo Kai,\n\ndas Treffen morgen muss leider abgesagt werden. Ich hoffe, du verstehst das.\n\nViele Grüße,\n\n[Dein Name]"
            ),
            None
        );
    }

    #[test]
    fn the_same_text_stays_the_same() {
        let text =
            "Kannst du den Bug mit dem Scrollen fixen? Der tritt auf, wenn man viele Sessions hat.";
        assert_eq!(layout(text, text).unwrap(), text);
        // The model's own spelling never gets in: only breaks are taken.
        assert_eq!(
            layout(text, "Kannst du den Bug mit dem Scrollen fixen?\n\nDer tritt auf, wenn man viele Sessions hat!").unwrap(),
            "Kannst du den Bug mit dem Scrollen fixen?\n\nDer tritt auf, wenn man viele Sessions hat."
        );
    }
}
