//! Spoken layout commands, applied to the finished transcript: "neue Zeile" breaks the line,
//! "neuer Absatz" starts a paragraph, "Spiegelstrich" starts a list item ("- ").
//!
//! Parakeet writes a command like any other words, usually set off by the commas its pauses
//! bring ("Einkaufsliste, neue Zeile, Milch."), sometimes glued to the next word
//! ("Spiegelstrichangebot"). Those commas and the period go; the next word starts with a capital.
//! Said as part of a sentence ("eine neue Zeile in der Tabelle", "die nächste Zeile") the words
//! stay: a command right after an article or similar is not one.

use std::sync::LazyLock;

use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Line,
    Paragraph,
    Bullet,
}

static COMMANDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:(?P<line>(?:neue|nächste)\s+zeile|zeilenumbruch|new\s+line)|(?P<paragraph>(?:neuer|nächster)\s+absatz|new\s+paragraph)|(?P<bullet>spiegelstrich|aufzählungspunkt|aufzählungszeichen|bullet\s+point))(?P<glue>\p{L}*)",
    )
    .unwrap()
});

/// Words before a command that make it part of the sentence ("eine neue Zeile").
const ARTICLES: &[&str] = &[
    "ein", "eine", "einer", "einen", "einem", "eines", "der", "die", "das", "den", "dem", "des",
    "jede", "jeder", "jedes", "jeden", "jedem", "kein", "keine", "keinen", "keiner", "diese",
    "dieser", "diesen", "dieses", "diesem", "welche", "welcher", "meine", "deine", "seine", "ihre",
    "unsere", "eure", "noch", "weitere", "a", "an", "the", "this", "that", "each", "every",
    "another", "one",
];

/// Lines that keep their comma before a break ("Hallo Kai," + new paragraph).
const GREETINGS: &[&str] = &[
    "hallo",
    "hi",
    "hey",
    "liebe",
    "lieber",
    "liebes",
    "sehr geehrte",
    "sehr geehrter",
    "moin",
    "servus",
    "guten morgen",
    "guten tag",
    "guten abend",
    "grüß dich",
    "dear",
    "hello",
];

/// The transcript with its layout commands carried out; text without commands comes back as it is.
pub fn apply(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = 0; // text[rest..] is not in `out` yet
    let mut capital = false;
    for found in COMMANDS.captures_iter(text) {
        let whole = found.get(0).unwrap();
        if whole.start() < rest {
            continue;
        }
        let command = if found.name("line").is_some() {
            Command::Line
        } else if found.name("paragraph").is_some() {
            Command::Paragraph
        } else {
            Command::Bullet
        };
        let glue = found.name("glue").unwrap();
        // "neue Zeilen", "Spiegelstriche" are words; "Spiegelstrichangebot" is a glued command.
        let glued = !glue.is_empty();
        if glued && (command != Command::Bullet || glue.len() < 4) {
            continue;
        }
        let before = &text[rest..whole.start()];
        if after_article(before) {
            continue;
        }
        push(&mut out, before, capital);
        end_line(&mut out, command);
        match command {
            Command::Line => out.push('\n'),
            Command::Paragraph => out.push_str("\n\n"),
            Command::Bullet => {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("- ");
            }
        }
        // Skip the commas, periods and spaces the pause around the command brought.
        rest = match glued {
            true => glue.start(),
            false => {
                let tail = &text[whole.end()..];
                whole.end() + (tail.len() - tail.trim_start_matches(SPURIOUS).len())
            }
        };
        capital = true;
    }
    if rest == 0 {
        return text.to_string();
    }
    push(&mut out, &text[rest..], capital);
    // The model ends the whole dictation with a period; list items go without.
    let lines: Vec<String> = out
        .split('\n')
        .map(|line| match line.strip_prefix("- ") {
            Some(item) if item.ends_with('.') && !item.ends_with("..") => {
                line[..line.len() - 1].to_string()
            }
            _ => line.to_string(),
        })
        .collect();
    lines.join("\n")
}

const SPURIOUS: &[char] = &[' ', ',', '.', ';', ':', '!', '?'];

/// Whether the text before a command ends in a word that makes the command part of the sentence.
/// Punctuation in between ("Tabelle. Neue Zeile") means it is a command after all.
fn after_article(before: &str) -> bool {
    let last = before
        .trim_end()
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default();
    if last.chars().last().is_some_and(|c| !c.is_alphanumeric()) {
        return false;
    }
    ARTICLES.contains(&last.to_lowercase().as_str())
}

/// Appends `piece`, its first letter upper case when it starts a line.
fn push(out: &mut String, piece: &str, capital: bool) {
    let mut chars = piece.chars();
    match (capital, chars.next()) {
        (true, Some(first)) => {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
        _ => out.push_str(piece),
    }
}

/// Ends the current line before a break: no trailing space, no comma or semicolon from the pause
/// (a greeting keeps its comma), no period on a list item.
fn end_line(out: &mut String, command: Command) {
    out.truncate(out.trim_end().len());
    let line_start = out.rfind('\n').map_or(0, |i| i + 1);
    let line = out[line_start..].to_lowercase();
    let greeting = command != Command::Bullet
        && GREETINGS.iter().any(|g| {
            line.strip_prefix(g)
                .is_some_and(|rest| !rest.starts_with(char::is_alphabetic))
        });
    if !greeting {
        let kept = out.trim_end_matches([',', ';']).len();
        out.truncate(kept);
    }
    if out[line_start..].starts_with("- ") && out.ends_with('.') && !out.ends_with("..") {
        out.pop();
    }
    out.truncate(out.trim_end().len());
}

#[cfg(test)]
mod tests {
    use super::apply;

    #[test]
    fn breaks_lines_and_paragraphs() {
        // As Parakeet wrote them for `say -v Anna` with pauses around the commands.
        assert_eq!(
            apply("Einkaufsliste, neue Zeile, Milch, neue Zeile, Eier."),
            "Einkaufsliste\nMilch\nEier."
        );
        assert_eq!(
            apply("Erstens das Angebot nächste Zeile, zweitens die Folien."),
            "Erstens das Angebot\nZweitens die Folien."
        );
        assert_eq!(
            apply("Danke dir. Neuer Absatz. Viele Grüße, Thomas"),
            "Danke dir.\n\nViele Grüße, Thomas"
        );
    }

    #[test]
    fn a_greeting_keeps_its_comma() {
        assert_eq!(
            apply("Hallo Kai, neuer Absatz, danke für das Treffen gestern."),
            "Hallo Kai,\n\nDanke für das Treffen gestern."
        );
    }

    #[test]
    fn makes_list_items() {
        assert_eq!(
            apply(
                "Die Punkte für morgen, Spiegelstrichangebot für Müller, Spiegelstrich Folien für das Board."
            ),
            "Die Punkte für morgen\n- Angebot für Müller\n- Folien für das Board"
        );
        assert_eq!(
            apply("Spiegelstrich Budget. Spiegelstrich Personal."),
            "- Budget\n- Personal"
        );
        assert_eq!(
            apply("Offene Themen: Spiegelstrich Budget, neuer Absatz, Danach reden wir."),
            "Offene Themen:\n- Budget\n\nDanach reden wir."
        );
    }

    #[test]
    fn leaves_the_words_in_a_sentence_alone() {
        for text in [
            "Wir brauchen eine neue Zeile in der Tabelle für den Kunden.",
            "Das steht in der nächsten Zeile.",
            "Die nächste Zeile ist leer.",
            "Neue Zeilen kommen unten dazu.",
            "Mit Spiegelstrichen ist es lesbarer.",
            "Ein neuer Absatz im Vertrag.",
            "Add a new line to the table.",
            "Das ist ein wichtiger neuer Punkt für die Agenda.",
            "Nichts zu tun hier.",
        ] {
            assert_eq!(apply(text), text);
        }
    }

    #[test]
    fn a_command_at_the_end_leaves_the_break() {
        assert_eq!(apply("Viele Grüße, neue Zeile."), "Viele Grüße\n");
        assert_eq!(apply("Neuer Absatz."), "\n\n");
        // "Hier" is not "hi": the comma goes.
        assert_eq!(
            apply("Hier ist die Liste, neue Zeile"),
            "Hier ist die Liste\n"
        );
        assert_eq!(apply("Themen, Spiegelstrich."), "Themen\n- ");
    }

    #[test]
    fn english_too() {
        assert_eq!(
            apply("Hi Sam, new paragraph, thanks for yesterday. New line, Best, Thomas."),
            "Hi Sam,\n\nThanks for yesterday.\nBest, Thomas."
        );
    }
}
