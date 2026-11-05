//! Word replacements applied to every transcript, e.g. `Solistik, Solistic => SOLISTIQ`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::{Regex, RegexBuilder};

use crate::vocab::Vocabulary;

/// The personal replacements file; `DICTATE_REPLACEMENTS` overrides it.
pub fn default_path() -> PathBuf {
    std::env::var_os("DICTATE_REPLACEMENTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::asr::data_dir().join("replacements.txt"))
}

pub struct Replacements(Vec<(Regex, String)>);

impl Replacements {
    /// Missing file = no replacements.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).with_context(|| format!("in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Replacements(Vec::new())),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    /// One rule per line: `from[, from…] => to`. `#` starts a comment.
    pub fn parse(text: &str) -> Result<Self> {
        let mut rules = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let Some((from, to)) = line.split_once("=>") else {
                bail!("line {}: expected `from => to`", n + 1);
            };
            for source in from.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                let pattern = format!(r"\b{}\b", regex::escape(source));
                let regex = RegexBuilder::new(&pattern).case_insensitive(true).build()?;
                rules.push((regex, to.trim().to_string()));
            }
        }
        Ok(Replacements(rules))
    }

    pub fn apply(&self, text: &str) -> String {
        self.0.iter().fold(text.to_string(), |text, (regex, to)| {
            regex.replace_all(&text, regex::NoExpand(to)).into_owned()
        })
    }
}

/// Fixes the transcript: dictionary words first (sound-alike → spelling), then the explicit
/// replacements file, so a rule there always has the last word.
pub struct Corrector {
    vocabulary: Vocabulary,
    replacements: Replacements,
    /// Carry out spoken layout commands in [`Corrector::finish`] (`commands.rs`).
    commands: bool,
}

impl Corrector {
    pub fn load(replacements: &std::path::Path, vocabulary: &[String]) -> Self {
        Corrector {
            vocabulary: Vocabulary::new(vocabulary),
            replacements: Replacements::load(replacements).unwrap_or_else(|e| {
                eprintln!("replacements: {e:#}");
                Replacements::parse("").unwrap()
            }),
            commands: true,
        }
    }

    /// With or without the spoken layout commands (`voice_commands` in the settings).
    pub fn with_commands(mut self, on: bool) -> Self {
        self.commands = on;
        self
    }

    /// Dictionary and replacements: the live text, the "Ausprobieren" field.
    pub fn apply(&self, text: &str) -> String {
        self.replacements.apply(&self.vocabulary.correct(text))
    }

    /// The text to paste: [`Corrector::apply`], then the layout commands.
    pub fn finish(&self, text: &str) -> String {
        let text = self.apply(text);
        match self.commands {
            true => crate::commands::apply(&text),
            false => text,
        }
    }
}

/// The rules of a replacements file as the window lists them: one `(from, to)` per rule line,
/// `from` as written ("Solistik, Solistic"). Comments and malformed lines are left out.
pub fn rules(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split('#').next())
        .filter_map(|line| line.split_once("=>"))
        .map(|(from, to)| (from.trim().to_string(), to.trim().to_string()))
        .filter(|(from, to)| !from.is_empty() && !to.is_empty())
        .collect()
}

/// The file with its rules replaced by `rules`: the comment block at the top stays, then one
/// `from => to` line per rule.
pub fn with_rules(text: &str, rules: &[(String, String)]) -> String {
    let mut out: String = text
        .lines()
        .take_while(|line| !line.split('#').next().unwrap_or_default().contains("=>"))
        .map(|line| format!("{line}\n"))
        .collect();
    for (from, to) in rules {
        let (from, to) = (from.trim(), to.trim());
        if !from.is_empty() && !to.is_empty() {
            out.push_str(&format!("{from} => {to}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_rewrites_rules_keeping_the_header() {
        let file = "# Word replacements\n# one per line\n\nSolistik, Solistic => SOLISTIQ # names\nbroken line\nparakeet ultra => Parakeet Ultra\n";
        let listed = rules(file);
        assert_eq!(
            listed,
            [
                ("Solistik, Solistic".to_string(), "SOLISTIQ".to_string()),
                ("parakeet ultra".to_string(), "Parakeet Ultra".to_string())
            ]
        );
        let mut changed = listed.clone();
        changed.remove(1);
        changed.push(("Matchdruckers".into(), "Merge Requests".into()));
        let written = with_rules(file, &changed);
        assert_eq!(
            written,
            "# Word replacements\n# one per line\n\nSolistik, Solistic => SOLISTIQ\nMatchdruckers => Merge Requests\n"
        );
        let applied = Replacements::parse(&written).unwrap();
        assert_eq!(applied.apply("die Matchdruckers"), "die Merge Requests");
    }

    #[test]
    fn replaces_whole_words_case_insensitively() {
        let r = Replacements::parse(
            "# names\nSolistik, solistic => SOLISTIQ\nparakeet ultra => Parakeet Ultra\n",
        )
        .unwrap();
        assert_eq!(
            r.apply("Bei solistik nutzen wir Parakeet ultra, nicht Solistiker."),
            "Bei SOLISTIQ nutzen wir Parakeet Ultra, nicht Solistiker."
        );
    }

    #[test]
    fn rejects_lines_without_arrow() {
        assert!(Replacements::parse("just text").is_err());
    }
}
