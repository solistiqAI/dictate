//! What the user reads, in German and English: the menu bar, the recorder's card, the bar, notes
//! and problems shown in the window. One place for every string, chosen by the language the app
//! resolved at start (`config.ui_language`, "auto" = the system's preferred language). Log lines
//! (stderr) stay English and are not here. The page (`ui/`) has its own tables
//! (`ui/src/lib/strings.ts`) and gets the resolved language in its state.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    /// "de" or "en", as the page and the prompts get it.
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    /// The language a setting stands for: "de" and "en" as they are; anything else ("auto")
    /// follows the system's preferred languages, German when the first starts with "de", else
    /// English. `DICTATE_LANG=de|en` overrides both (previews, tests).
    pub fn resolve(setting: &str, preferred: &[String]) -> Lang {
        let env = std::env::var("DICTATE_LANG").unwrap_or_default();
        for choice in [env.as_str(), setting] {
            match choice.trim().to_ascii_lowercase().as_str() {
                "de" => return Lang::De,
                "en" => return Lang::En,
                _ => {}
            }
        }
        Lang::from_system(preferred)
    }

    /// German when the first preferred language is German ("de", "de-DE", "de_AT"), else English.
    pub fn from_system(preferred: &[String]) -> Lang {
        match preferred.first() {
            Some(first) if first.to_ascii_lowercase().starts_with("de") => Lang::De,
            _ => Lang::En,
        }
    }
}

/// German until `set` is called: what every build showed before there was English.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::De,
    }
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// The German or the English text, by the current language.
pub fn pick<T>(de: T, en: T) -> T {
    match lang() {
        Lang::De => de,
        Lang::En => en,
    }
}

/// Fixed texts: `name => "Deutsch", "English";` becomes `pub fn name() -> &'static str`.
macro_rules! texts {
    ($($(#[$meta:meta])* $name:ident => $de:literal, $en:literal;)*) => {
        $($(#[$meta])* pub fn $name() -> &'static str { pick($de, $en) })*
    };
}

/// Without it the app sees no trigger: the Accessibility permission on macOS; on Linux reading
/// the keyboard (on Wayland the `input` group, docs/linux.md).
pub fn accessibility_missing() -> &'static str {
    match cfg!(target_os = "linux") {
        true => pick(
            "Kein Zugriff auf die Tastatur (Gruppe input)",
            "No access to the keyboard (input group)",
        ),
        false => pick(
            "Bedienungshilfen fehlen",
            "Accessibility permission missing",
        ),
    }
}

pub fn accessibility_missing_open() -> &'static str {
    match cfg!(target_os = "linux") {
        true => pick(
            "Kein Zugriff auf die Tastatur (Gruppe input) – Einstellungen öffnen",
            "No access to the keyboard (input group) – open Settings",
        ),
        false => pick(
            "Bedienungshilfen fehlen – Einstellungen öffnen",
            "Accessibility permission missing – open Settings",
        ),
    }
}

texts! {
    // Menu bar menu and the app menu (while the window is open).
    loading_model => "Lädt Modell…", "Loading model…";
    nothing_dictated => "Noch nichts diktiert", "Nothing dictated yet";
    correct_last => "Letztes Diktat korrigieren…", "Correct last dictation…";
    read_selection => "Markierten Text vorlesen", "Read selected text aloud";
    discuss => "Darüber diskutieren", "Talk it over";
    start_souffleur => "Souffleur starten", "Start Souffleur";
    stop_souffleur => "Souffleur beenden", "Stop Souffleur";
    history_menu => "Verlauf…", "History…";
    dictionary_menu => "Wörterbuch…", "Dictionary…";
    settings_menu => "Einstellungen…", "Settings…";
    quit => "Dictate beenden", "Quit Dictate";
    edit_menu => "Bearbeiten", "Edit";

    // Status lines (menu, window).
    ready => "Bereit", "Ready";
    recording => "Aufnahme…", "Recording…";
    transcribing => "Transkribiert…", "Transcribing…";
    cancelled => "abgebrochen", "cancelled";
    nothing_heard_status => "nichts gehört", "nothing heard";
    shortcut_discarded => "Tastenkürzel, kein Diktat: verworfen", "Shortcut, not a dictation: discarded";
    gemma_ready => "Gemma 4 ist da", "Gemma 4 is ready";
    no_mic =>
        "Kein Mikrofon-Zugriff: in Systemeinstellungen → Datenschutz & Sicherheit → Mikrofon erlauben, dann neu starten",
        "No microphone access: allow it in System Settings → Privacy & Security → Microphone, then restart";

    // The recorder's card and pill.
    waiting => "Sprich…", "Speak…";
    no_text_field => "Kein Textfeld", "No text field";
    nothing_heard => "Nichts gehört", "Nothing heard";
    copy => "Kopieren", "Copy";
    /// Once per start when a shortcut tidies and the chosen Gemma is not downloaded.
    #[cfg(target_os = "macos")]
    without_gemma =>
        "Ohne Gemma hat Apple Intelligence nachbearbeitet, das dauert bei langen Texten mehrere Sekunden. Gemma laden: Einstellungen → Erkennung",
        "Without Gemma, Apple Intelligence tidied the text, which takes several seconds on long ones. Get Gemma: Settings → Recognition";
    #[cfg(not(target_os = "macos"))]
    without_gemma =>
        "Ohne Gemma bleibt der Text, wie er war. Gemma laden: Einstellungen → Erkennung",
        "Without Gemma, the text stays as it was. Get Gemma: Settings → Recognition";
    close => "Schließen", "Close";
    copied_check => "Kopiert ✓", "Copied ✓";
    undo => "Rückgängig", "Undo";
    undone => "Rückgängig ✓", "Undone ✓";
    learned => "Gelernt", "Learned";
    remember_question => "Merken?", "Remember?";
    remember => "Merken", "Remember";
    dont_remember => "Nicht merken", "Don't remember";
    settings => "Einstellungen", "Settings";

    // The bar at the bottom of the screen.
    dictate => "Diktieren", "Dictate";
    done => "Fertig", "Done";
    side_panel => "Seitenleiste", "Side panel";

    // Notes in the window.
    copied => "Kopiert", "Copied";
    saved => "Gespeichert", "Saved";
    already_downloading => "Lädt schon herunter…", "Already downloading…";
    downloading_gemma =>
        "Lädt Gemma 4 herunter (5 GB), ein paar Minuten…",
        "Downloading Gemma 4 (5 GB), a few minutes…";
    accessibility_first => "Erst Bedienungshilfen freigeben", "Allow Accessibility first";
    souffleur_not_running => "Der Souffleur läuft nicht", "The Souffleur is not running";
    at_least_one_shortcut => "Mindestens ein Kurzbefehl", "At least one shortcut";
    apple_detail =>
        "im System, schwächer bei Selbstkorrekturen",
        "built into the system, weaker at self-corrections";
}

/// "Zuletzt: …" in the menu, a click copies it.
pub fn last(preview: &str) -> String {
    format!("{}: {preview}", pick("Zuletzt", "Last"))
}

pub fn ready_with(triggers: &str) -> String {
    format!("{}: {triggers}", ready())
}

pub fn model_missing(problem: &str) -> String {
    format!("{}: {problem}", pick("Modell fehlt", "Model missing"))
}

pub fn model_problem(problem: &str) -> String {
    format!("{}: {problem}", pick("Modell", "Model"))
}

/// The Linux build found no ONNX Runtime (`onnxruntime.rs`); `looked` lists the places.
pub fn onnx_runtime_missing(looked: &str) -> String {
    pick(
        format!(
            "ONNX Runtime nicht gefunden (gesucht: {looked}); `dictate model download onnxruntime` \
             lädt es, oder ORT_DYLIB_PATH zeigt auf eine libonnxruntime.so"
        ),
        format!(
            "ONNX Runtime not found (looked in {looked}); `dictate model download onnxruntime` \
             fetches it, or point ORT_DYLIB_PATH at a libonnxruntime.so"
        ),
    )
}

pub fn downloading(name: &str) -> String {
    pick(
        format!("Lädt {name} herunter…"),
        format!("Downloading {name}…"),
    )
}

pub fn download_failed(name: &str, problem: &str) -> String {
    pick(
        format!("Download von {name}: {problem}"),
        format!("Download of {name}: {problem}"),
    )
}

pub fn mic_ok(level_db: f32) -> String {
    pick(
        format!("Mikrofon ok ({level_db:.0} dBFS)"),
        format!("Microphone ok ({level_db:.0} dBFS)"),
    )
}

pub fn mic_problem(problem: &str) -> String {
    format!("{}: {problem}", pick("Mikrofon", "Microphone"))
}

pub fn transcription_failed(problem: &str) -> String {
    format!(
        "{}: {problem}",
        pick("Transkription fehlgeschlagen", "Transcription failed")
    )
}

pub fn unknown_key(code: impl std::fmt::Display) -> String {
    pick(
        format!("Diese Taste kennt Dictate nicht (Code {code})"),
        format!("Dictate does not know this key (code {code})"),
    )
}

/// After the first learned correction on the card: "" / " und eine weitere Korrektur" / …
pub fn more_corrections(more: usize) -> String {
    match (lang(), more) {
        (_, 0) => String::new(),
        (Lang::De, 1) => " und eine weitere Korrektur".into(),
        (Lang::De, n) => format!(" und {n} weitere Korrekturen"),
        (Lang::En, 1) => " and one more correction".into(),
        (Lang::En, n) => format!(" and {n} more corrections"),
    }
}

/// "Gespeichert, 2 Korrekturen gelernt".
pub fn saved_learned(n: usize) -> String {
    match (lang(), n) {
        (Lang::De, 1) => "Gespeichert, 1 Korrektur gelernt".into(),
        (Lang::De, n) => format!("Gespeichert, {n} Korrekturen gelernt"),
        (Lang::En, 1) => "Saved, learned 1 correction".into(),
        (Lang::En, n) => format!("Saved, learned {n} corrections"),
    }
}

/// „heard“ → „meant“ (German quotes) or “heard” → “meant”.
pub fn quoted_change(heard: &str, meant: &str) -> String {
    pick(
        format!("\u{201e}{heard}\u{201c} → \u{201e}{meant}\u{201c}"),
        format!("\u{201c}{heard}\u{201d} → \u{201c}{meant}\u{201d}"),
    )
}

/// The detail of a model's label for the settings ("genau, 2,5 GB"): written in German in
/// `asr::MODELS` and `llm.rs`, put into English here.
pub fn model_detail(german: &str) -> String {
    model_detail_in(lang(), german)
}

fn model_detail_in(lang: Lang, german: &str) -> String {
    if lang == Lang::De {
        return german.to_string();
    }
    const WORDS: &[(&str, &str)] = &[
        ("lokal auf der GPU", "local on the GPU"),
        ("im System", "built into the system"),
        ("nur Englisch", "English only"),
        ("empfohlen", "recommended"),
        ("sparsam", "light on power"),
        ("genau", "accurate"),
        ("Sprachen", "languages"),
    ];
    let mut out = german.to_string();
    for (de, en) in WORDS {
        out = out.replace(de, en);
    }
    // "2,5 GB" → "2.5 GB": a comma between digits is a decimal comma.
    let chars: Vec<char> = out.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let digit = |j: Option<usize>| {
                j.and_then(|j| chars.get(j))
                    .is_some_and(char::is_ascii_digit)
            };
            if c == ',' && digit(i.checked_sub(1)) && digit(Some(i + 1)) {
                '.'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn auto_follows_the_first_preferred_language() {
        // SAFETY: no other test reads DICTATE_LANG.
        unsafe { std::env::remove_var("DICTATE_LANG") };
        assert_eq!(Lang::resolve("auto", &langs(&["de-DE", "en-US"])), Lang::De);
        assert_eq!(Lang::resolve("auto", &langs(&["de_AT"])), Lang::De);
        assert_eq!(Lang::resolve("auto", &langs(&["en-GB", "de-DE"])), Lang::En);
        assert_eq!(Lang::resolve("auto", &langs(&["fr-FR"])), Lang::En);
        assert_eq!(Lang::resolve("", &[]), Lang::En);
        assert_eq!(Lang::resolve("de", &langs(&["en-US"])), Lang::De);
        assert_eq!(Lang::resolve("EN", &langs(&["de-DE"])), Lang::En);
        assert_eq!(Lang::De.code(), "de");
    }

    #[test]
    fn model_details_in_english() {
        assert_eq!(
            model_detail_in(Lang::En, "empfohlen, sparsam, 0,6 GB"),
            "recommended, light on power, 0.6 GB"
        );
        assert_eq!(
            model_detail_in(Lang::En, "40 Sprachen, 2,6 GB"),
            "40 languages, 2.6 GB"
        );
        assert_eq!(
            model_detail_in(Lang::En, "nur Englisch, 2,5 GB"),
            "English only, 2.5 GB"
        );
        assert_eq!(model_detail_in(Lang::De, "genau, 2,5 GB"), "genau, 2,5 GB");
        // Every detail the app shows has no German left in English.
        for model in crate::asr::MODELS {
            let detail = model.label.split_once(" (").map_or("", |(_, d)| d);
            let english = model_detail_in(Lang::En, detail);
            assert!(english.chars().all(|c| !"äöüß".contains(c)), "{english}");
        }
    }

    #[test]
    fn german_until_set() {
        // Tests never call `set` (they run in parallel and many check German texts).
        assert_eq!(lang(), Lang::De);
        assert_eq!(ready(), "Bereit");
    }
}
