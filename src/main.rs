//! dictate: local push-to-talk dictation for macOS (and Windows and Linux betas) with Parakeet Ultra.

mod app;
mod audio;
mod bar;
mod correct;
mod feedback;
mod format;
mod glyph;
mod history;
mod llm;
mod local;
mod meeting;
mod overlay;
mod platform;
mod protocol;
mod report;
mod talk;
mod voice;
mod window;

// The platform's own parts, under the names they had before the platform layer.
use platform::{ane, media, pocket};

// The platform-free part (`core/`), under the names the modules had here.
use dictate_core::{
    asr, commands, config, eval, learn, profile, replace, stats, text, trigger, vad,
};
// The Neural Engine helper plugs into it (`platform/macos/ane.rs`).
#[cfg(target_os = "macos")]
use dictate_core::engine;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::trigger::Mode;

#[derive(Parser)]
#[command(
    version,
    about = "Local dictation: hold a key, talk, the text lands at your cursor"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    run: RunArgs,
}

#[derive(clap::Args)]
struct RunArgs {
    /// Key or mouse button that records: fn, right-option, right-command, f13…f20,
    /// ctrl+alt+cmd+d, alt+cmd+fn (modifiers alone), mouse2 (middle), mouse3/mouse4 (side
    /// buttons). Repeatable; a suffix like `mouse3@toggle` gives one trigger its own mode.
    /// Without it the settings (config.toml) apply: by default Fn and ⇧⌥⌘5 in hybrid mode.
    #[arg(short, long = "trigger")]
    triggers: Vec<String>,
    /// hold: record while pressed. toggle: press to start and stop. hybrid: a tap records
    /// hands-free until the next press, holding records until release. Default: hybrid.
    #[arg(short, long)]
    mode: Option<Mode>,
    /// Model name (see `dictate model list`) or a directory with the ONNX files.
    /// Without it the settings (config.toml, `model`) apply: by default parakeet-ultra.
    #[arg(long)]
    model: Option<String>,
    /// Print the transcript instead of pasting it.
    #[arg(long)]
    no_paste: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Transcribe a WAV file and print the text.
    Transcribe {
        file: PathBuf,
        #[arg(long, default_value = asr::DEFAULT_MODEL)]
        model: String,
        /// Spoken language for models that take one ("de", "en", "auto"); default from config.toml.
        #[arg(long)]
        language: Option<String>,
    },
    /// List the microphones, then record 3 seconds from the chosen one (`microphone` in
    /// config.toml, else the Mac's default) and show its level (talk while it runs).
    Mic,
    /// Render the recorder overlay to a PNG (design check, no window on screen; `DICTATE_LANG=en`
    /// for the English texts).
    #[command(hide = true)]
    OverlayPreview {
        out: PathBuf,
        /// Status instead of the level meter, e.g. "…" or "Kein Textfeld".
        #[arg(long)]
        symbol: Option<String>,
        /// With the "Kopieren" / "Schließen" buttons.
        #[arg(long)]
        actions: bool,
        /// As a learned correction with "Rückgängig".
        #[arg(long)]
        learn: bool,
        /// With "Einstellungen" and "Souffleur" beside the pill, as while recording; a time
        /// (e.g. 12:03) as while the Souffleur runs.
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        shortcuts: Option<String>,
        /// Show this part of the text struck through, as "Korrigieren" does while recording
        /// (repeatable).
        #[arg(long)]
        strike: Vec<String>,
        #[arg(
            long,
            default_value = "Hallo, hier ist Dictate. Morgen um 10 Uhr haben wir das Meeting"
        )]
        text: String,
    },
    /// Measure recognition on clips with known text (`scripts/eval.sh` makes the list).
    Eval {
        /// Lines of `wav<TAB>dict<TAB>reference text`.
        list: PathBuf,
        #[arg(long, default_value = asr::DEFAULT_MODEL)]
        model: String,
        /// Write the numbers here as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Compare with such a file; fails when WER or the dictionary words got clearly worse.
        #[arg(long)]
        baseline: Option<PathBuf>,
        /// The profile whose dictionary is used (`scripts/eval.sh` passes eval/profile.toml);
        /// default the installed one.
        #[arg(long)]
        profile: Option<PathBuf>,
    },
    /// The profile: default dictionary words, report target, the Souffleur's tools and context,
    /// web pages allowed to call the app (`core/src/profile.rs`, `profiles/`).
    Profile {
        #[command(subcommand)]
        command: Option<ProfileCommand>,
    },
    /// Read a text aloud with the settings' voices; with `--ask` the agent answers it first and the
    /// answer is read (one turn of "Darüber diskutieren" without microphone and window).
    #[command(hide = true)]
    Say {
        text: String,
        #[arg(long)]
        ask: bool,
    },
    /// Lay a text out as the "Formatieren" shortcuts do: the spoken commands, then Gemma's layout
    /// (Apple's language model without it). Prints the result; the note says why a text stayed.
    Format {
        text: String,
        /// gemma-4-e4b (when downloaded) or apple.
        #[arg(long, default_value = llm::DEFAULT)]
        model: String,
    },
    /// Drop self-corrections and filler words as the "Korrigieren" shortcuts do: the spoken
    /// commands, filler words by rule, then Apple's language model where a correction cue is.
    /// Prints the result; the note on stderr says what was cut or why the text stayed.
    Correct {
        /// The text; "-" reads one text per line from stdin and prints a JSON line for each
        /// (input, output, note, seconds), with the model loaded once.
        #[arg(default_value = "")]
        text: String,
        /// Score the cases of a file like eval/corrections.tsv instead.
        #[arg(long)]
        eval: Option<PathBuf>,
        /// Lay the result out afterwards, as a shortcut with both switches on.
        #[arg(long)]
        format: bool,
        /// The model that proposes: gemma-4-e4b (when downloaded) or apple.
        #[arg(long, default_value = llm::DEFAULT)]
        model: String,
    },
    /// The anonymous usage report (what goes to the profile's report target once a day).
    Report {
        /// Days to cover (default: since the last report, at most 7).
        #[arg(long)]
        days: Option<i64>,
        /// Send it to the profile's target (a private repo's issue with `gh`, or its URL).
        #[arg(long)]
        post: bool,
    },
    /// The Souffleur's saved meetings (`list`, `show`), or without a meeting: a recording plays
    /// as "Andere" in real time through the transcription and the agent (your Claude Code, the
    /// meeting settings in config.toml); prints the lines, what the agent does and its cards.
    /// Make one with `say -o x.aiff` + `afconvert`.
    #[command(args_conflicts_with_subcommands = true)]
    Souffleur {
        #[command(subcommand)]
        command: Option<SouffleurCommand>,
        /// The recording to play as "Andere".
        file: Option<PathBuf>,
        /// Claude model instead of `meeting_model`.
        #[arg(long)]
        model: Option<String>,
    },
    /// Draw the bar at the bottom of the screen into a PNG (transparent; composite on dark).
    #[command(hide = true)]
    BarPreview {
        out: PathBuf,
        /// As while a dictation runs ("Fertig").
        #[arg(long)]
        recording: bool,
        /// As while the Souffleur runs (started 12 minutes ago).
        #[arg(long)]
        meeting: bool,
    },
    /// Whether the paste check sees a text field in the frontmost app (debugging "not pasted"):
    /// waits `--wait` seconds (switch to the app meanwhile), then prints the answer; the reasons go
    /// to stderr as in the log.
    Focus {
        #[arg(long, default_value_t = 3)]
        wait: u64,
    },
    /// Manage the speech models.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
}

#[derive(Subcommand)]
enum SouffleurCommand {
    /// The saved meetings, newest first: number, start, lines, answers, first topic.
    List {
        /// How many.
        #[arg(long, default_value_t = 20)]
        last: usize,
    },
    /// Print a saved meeting (Markdown: transcript, the Souffleur's answers, "Das Wichtigste").
    Show {
        /// `latest` (default), its number in `list` (1 = newest) or its name ("2026-11-03 13-28").
        which: Option<String>,
        /// Only the Souffleur's answers and "Das Wichtigste", without the transcript.
        #[arg(long)]
        answers: bool,
        /// Only the file's path.
        #[arg(long)]
        path: bool,
    },
    /// Start the Souffleur in the running app (or show its panel when it runs); Raycast uses it.
    Start,
    /// End the running Souffleur.
    Stop,
}

#[derive(Subcommand)]
enum ModelCommand {
    /// Show the known models and where they live.
    List,
    /// Download a model (0.6 to 5 GB; gemma-4-e4b is the text model for "Korrigieren").
    Download {
        #[arg(default_value = asr::DEFAULT_MODEL)]
        name: String,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// What the profile in use says, and where it comes from (default).
    Show,
    /// Install a profile: a file, an https URL or gh:owner/repo/path (GitHub CLI, private repos
    /// work). The running app picks it up when it restarts.
    Install { source: String },
    /// Fetch the installed profile's `update_from` again now (the app does it once a day).
    Update,
}

fn profile_command(command: Option<ProfileCommand>) -> Result<()> {
    match command.unwrap_or(ProfileCommand::Show) {
        ProfileCommand::Show => print!("{}", profile::describe(&profile::current())),
        ProfileCommand::Install { source } => {
            let installed = profile::install(&source)?;
            print!("{}", profile::describe(&installed));
        }
        ProfileCommand::Update => {
            let installed = profile::load(&profile::Profile::path())?
                .context("no profile installed (dictate profile install <source>)")?;
            anyhow::ensure!(
                !installed.update_from.is_empty(),
                "the installed profile names no update_from"
            );
            profile::install(&installed.update_from)?;
            print!("{}", profile::describe(&profile::current()));
        }
    }
    Ok(())
}

/// What the user reads comes in the language of the settings (`ui_language`; "auto" = the
/// system's); `DICTATE_LANG=de|en` overrides it, e.g. for `overlay-preview`.
fn init_lang() {
    let config = config::Config::load(&config::Config::path()).unwrap_or_default();
    let lang = text::Lang::resolve(&config.ui_language, &platform::preferred_languages());
    text::set(lang);
}

fn main() -> Result<()> {
    // Parakeet on the Neural Engine runs in the Swift helper; dictate-core's engines get it here
    // (a no-op on Windows).
    ane::register();
    profile::init();
    init_lang();
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Profile { command }) => profile_command(command),
        Some(Command::Transcribe {
            file,
            model,
            language,
        }) => {
            println!("{}", asr::transcribe_file(&file, &model, language)?);
            Ok(())
        }
        Some(Command::Mic) => check_microphone(),
        Some(Command::Focus { wait }) => {
            std::thread::sleep(std::time::Duration::from_secs(wait));
            let answer = match crate::platform::focused_accepts_text() {
                Some(true) => "text field: paste",
                Some(false) => "no text field: not pasted, the card keeps the text",
                None => "cannot tell: paste anyway",
            };
            println!("{answer}");
            #[cfg(target_os = "macos")]
            println!(
                "web content in the focused window: {}",
                crate::platform::frontmost_web_area().is_some()
            );
            Ok(())
        }
        Some(Command::Souffleur {
            command,
            file,
            model,
        }) => match (command, file) {
            (Some(command), _) => saved_meetings(command),
            (None, Some(file)) => souffleur(&file, model),
            (None, None) => {
                bail!("dictate souffleur <recording.wav> | list | show [latest|N] (see --help)")
            }
        },
        Some(Command::Report { days, post }) => usage_report(days, post),
        Some(Command::Say { text, ask }) => say(&text, ask),
        Some(Command::Format { text, model }) => {
            eprintln!("format: {}", correct::prepare(&model));
            let started = Instant::now();
            let (text, note) = correct::tidy_text(&commands::apply(&text), false, true);
            eprintln!("format: {note}");
            println!("{text}");
            eprintln!("{:.2} s", started.elapsed().as_secs_f32());
            Ok(())
        }
        Some(Command::Correct {
            eval: Some(file),
            model,
            format,
            ..
        }) => eval_corrections(&file, &model, format),
        Some(Command::Correct {
            text,
            format,
            model,
            eval: None,
        }) if text == "-" => {
            eprintln!("correct: {}", correct::prepare(&model));
            for line in std::io::stdin().lines() {
                let line = line?;
                let started = Instant::now();
                let (out, note) = correct::tidy_text(&commands::apply(&line), true, format);
                println!(
                    "{}",
                    serde_json::json!({
                        "input": line,
                        "output": out,
                        "note": note,
                        "seconds": started.elapsed().as_secs_f64(),
                    })
                );
            }
            Ok(())
        }
        Some(Command::Correct {
            text,
            format,
            model,
            eval: None,
        }) => {
            eprintln!("correct: {}", correct::prepare(&model));
            let started = Instant::now();
            let (text, note) = correct::tidy_text(&commands::apply(&text), true, format);
            eprintln!("correct: {:.2} s, {note}", started.elapsed().as_secs_f32());
            println!("{text}");
            Ok(())
        }
        Some(Command::Eval {
            list,
            model,
            json,
            baseline,
            profile: chosen,
        }) => {
            if let Some(path) = chosen {
                profile::use_file(&path)?;
            }
            eval::run(&list, &model, json.as_deref(), baseline.as_deref())
        }
        Some(Command::OverlayPreview {
            out,
            symbol,
            text,
            actions,
            learn,
            strike,
            shortcuts,
        }) => overlay_preview(
            &out,
            symbol.as_deref(),
            &text,
            actions,
            learn,
            &strike,
            shortcuts.as_deref(),
        ),
        Some(Command::BarPreview {
            out,
            recording,
            meeting,
        }) => bar_preview(&out, recording, meeting),
        Some(Command::Model {
            command: ModelCommand::List,
        }) => {
            for model in llm::MODELS {
                let path = llm::path(model);
                let state = if path.exists() { "installed" } else { "-" };
                println!(
                    "{:22} {state:10} {}\n{:33}Korrigieren · {}",
                    model.name,
                    model.label,
                    "",
                    path.display()
                );
            }
            asr::print_models();
            Ok(())
        }
        Some(Command::Model {
            command: ModelCommand::Download { name },
        }) => {
            let dir = match llm::find(&name) {
                Some(_) => llm::download(&name)?,
                None => asr::download(&name)?,
            };
            println!("{name} ready in {}", dir.display());
            Ok(())
        }
        None => run(cli.run),
    }
}

/// Runs the cases of `file` through "Korrigieren" with `model` and scores them: corrections made,
/// texts left alone, the model's median time.
fn eval_corrections(file: &std::path::Path, model: &str, layout: bool) -> Result<()> {
    let cases = correct::parse_cases(&std::fs::read_to_string(file)?);
    anyhow::ensure!(!cases.is_empty(), "no cases in {}", file.display());
    eprintln!("correct: {}", correct::prepare(model));
    // The first answer waits for loading; keep it out of the timing.
    let _ = correct::correct("Wir treffen uns um drei, nein, um vier.");
    let (mut fixed, mut kept, mut times) = (0, 0, Vec::new());
    for case in &cases {
        let started = Instant::now();
        // As the default shortcut does it: corrected and laid out (the cases compare words only).
        let (out, note) = correct::tidy_text(&case.input, true, layout);
        let seconds = started.elapsed().as_secs_f64();
        let pass = case.passes(&out);
        match case.accepted {
            Some(_) => fixed += pass as usize,
            None => kept += pass as usize,
        }
        if !note.contains("no correction cue") && note != "2 filler words" {
            times.push(seconds);
        }
        println!(
            "{} {seconds:5.2} s  {out}  | {note}",
            if pass { "✓" } else { "✗" }
        );
    }
    times.sort_by(f64::total_cmp);
    let corrections = cases.iter().filter(|c| c.accepted.is_some()).count();
    println!(
        "{model}: {fixed}/{corrections} corrected, {kept}/{} left alone, median {:.2} s per model call",
        cases.len() - corrections,
        times.get(times.len() / 2).copied().unwrap_or(0.0)
    );
    Ok(())
}

/// `dictate say TEXT [--ask]`: the speech part without microphone and window.
fn say(text: &str, ask: bool) -> Result<()> {
    let config = config::Config::load(&config::Config::path()).unwrap_or_default();
    let text = if ask {
        let agent = voice::Agent::for_conversation(&config)?;
        let started = Instant::now();
        let id = agent.start(text)?;
        let answer = agent.ask(id, None)?;
        eprintln!(
            "session {id}, answer after {:.1} s",
            started.elapsed().as_secs_f32()
        );
        agent.close(id);
        answer
    } else {
        text.to_string()
    };
    println!("{text}");
    let spoken = voice::clean_for_speech(&text);
    let voice = voice::pick_voice(&spoken, &config.voice_de, &config.voice_en).to_string();
    eprintln!("voice {voice}: {spoken}");
    let (tx, rx) = std::sync::mpsc::channel();
    let done = tx.clone();
    if config.voice_engine == "pocket" && voice == config.voice_de && pocket::Pocket::cached() {
        let started = Instant::now();
        pocket::Pocket::new().speak(&spoken, &config.voice_pocket, config.speed(), move || {
            let _ = done.send(());
        })?;
        eprintln!(
            "PocketTTS: first sentence after {:.1} s",
            started.elapsed().as_secs_f32()
        );
    } else {
        voice::Speaker::default().speak(
            &spoken,
            &voice,
            (config.voice_rate as f32 * config.speed()).round() as u32,
            move || {
                let _ = done.send(());
            },
        )?;
    }
    let _ = rx.recv();
    Ok(())
}

/// Prints the usage report; `--post` also sends it where the profile says.
fn usage_report(days: Option<i64>, post: bool) -> Result<()> {
    let config = config::Config::load(&config::Config::path()).unwrap_or_default();
    let mut state = report::load_state();
    let now = stats::now();
    let from = match days {
        Some(days) => now - days * 86_400,
        None => state.last_posted.max(now - 7 * 86_400),
    };
    let events = stats::load(&stats::path(), from);
    let summary = report::summarize(&events, from, now);
    let meta = report::meta(&config, &state.install);
    println!("{}", report::markdown(&summary, &meta));
    if post {
        let target = report::post(&summary, &meta)?;
        state.last_posted = now;
        report::save_state(&state)?;
        eprintln!("posted to {target}");
    }
    Ok(())
}

/// `dictate souffleur <wav>`: see the subcommand.
fn souffleur(file: &std::path::Path, model: Option<String>) -> Result<()> {
    use std::sync::{Arc, Mutex};
    let mut config = config::Config::load(&config::Config::path()).unwrap_or_default();
    if let Some(model) = model {
        config.meeting_model = model;
    }
    let started = std::time::Instant::now();
    let last = Mutex::new((String::new(), String::new()));
    let sink: meeting::Sink = Arc::new(move |update| {
        let t = started.elapsed().as_secs_f32();
        match update {
            meeting::Update::Line(line) if !line.live && !line.text.is_empty() => {
                println!(
                    "[{t:5.1}s] {}: {}",
                    if line.who == "me" { "Ich" } else { "Andere" },
                    line.text
                )
            }
            meeting::Update::Card(card) if !card.pending => {
                println!("[{t:5.1}s] ## {}\n{}\n", card.title, card.body)
            }
            meeting::Update::Highlights(list) => {
                println!("[{t:5.1}s] ## {}", meeting::highlights_heading());
                for l in list {
                    println!("- {l}");
                }
                println!();
            }
            meeting::Update::Status(s) => {
                let mut last = last.lock().unwrap();
                if (s.agent.clone(), s.activity.clone()) != *last {
                    println!("[{t:5.1}s]    ({} {})", s.agent, s.activity);
                    *last = (s.agent, s.activity);
                }
                if !s.problem.is_empty() {
                    println!("[{t:5.1}s]    problem: {}", s.problem);
                }
            }
            meeting::Update::Ended(reason) => {
                println!("[{t:5.1}s] ended {}", reason.unwrap_or_default())
            }
            _ => {}
        }
    });
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let sink2 = sink.clone();
    let sink: meeting::Sink = Arc::new(move |update| {
        let ended = matches!(update, meeting::Update::Ended(_));
        sink2(update);
        if ended {
            let _ = done_tx.send(());
        }
    });
    let _handle = meeting::start(
        meeting::Setup {
            config,
            replacements: asr::data_dir().join("replacements.txt"),
            folder: std::env::temp_dir().join("dictate-souffleur"),
            replay: Some(file.to_path_buf()),
        },
        sink,
    );
    let _ = done_rx.recv();
    Ok(())
}

/// `dictate souffleur list` / `show`: the transcripts in the data folder's `meetings/`.
fn saved_meetings(command: SouffleurCommand) -> Result<()> {
    let folder = meeting::folder();
    let saved = meeting::saved(&folder);
    match command {
        SouffleurCommand::Start | SouffleurCommand::Stop => {
            let start = matches!(command, SouffleurCommand::Start);
            local::post(if start {
                "/souffleur"
            } else {
                "/souffleur/stop"
            })?;
            println!(
                "{}",
                if start {
                    text::pick("Souffleur läuft", "Souffleur running")
                } else {
                    text::pick("Souffleur beendet", "Souffleur stopped")
                }
            );
        }
        SouffleurCommand::List { last } => {
            if saved.is_empty() {
                eprintln!("no meetings yet in {}", folder.display());
            }
            for (i, s) in saved.iter().take(last).enumerate() {
                let span = s
                    .span
                    .as_ref()
                    .map_or(String::new(), |(a, b)| format!("{a}–{b}"));
                println!(
                    "{:>3}  {}  {span:<11}  {:>3} Zeilen  {:>2} Antworten  {}",
                    i + 1,
                    s.name,
                    s.lines,
                    s.answers,
                    s.topic
                );
            }
            eprintln!("{}", folder.display());
        }
        SouffleurCommand::Show {
            which,
            answers,
            path,
        } => {
            let which = which.unwrap_or_else(|| "latest".into());
            let found = match (which.as_str(), which.parse::<usize>()) {
                ("latest", _) => saved.first(),
                (_, Ok(n)) => n.checked_sub(1).and_then(|i| saved.get(i)),
                (name, _) => saved
                    .iter()
                    .find(|s| s.name == name.trim_end_matches(".md")),
            };
            let Some(found) = found else {
                bail!(
                    "no meeting {which:?} in {} (see `dictate souffleur list`)",
                    folder.display()
                );
            };
            if path {
                println!("{}", found.path.display());
                return Ok(());
            }
            let text = std::fs::read_to_string(&found.path)
                .with_context(|| format!("read {}", found.path.display()))?;
            if answers {
                print!("{}", meeting::answers_only(&text));
            } else {
                print!("{text}");
            }
        }
    }
    Ok(())
}

fn check_microphone() -> Result<()> {
    let config = config::Config::load(&config::Config::path()).unwrap_or_default();
    audio::choose(&config.microphone);
    for (i, name) in audio::input_names().iter().enumerate() {
        let default = if i == 0 { "  (Standard)" } else { "" };
        eprintln!("input: {name}{default}");
    }
    match app::mic_check(std::time::Duration::from_secs(3)) {
        Ok(level) => eprintln!("microphone ok: {level:.0} dBFS (speech is about -35…-20)"),
        Err(problem) => eprintln!("{problem}"),
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn overlay_preview(
    out: &std::path::Path,
    symbol: Option<&str>,
    text: &str,
    actions: bool,
    learn: bool,
    strike: &[String],
    shortcuts: Option<&str>,
) -> Result<()> {
    let mtm = platform::preview_app()?;
    let levels = [-58.0, -45.0, -30.0, -22.0, -26.0, -38.0];
    let left = match symbol {
        Some(symbol) => overlay::Left::Symbol(symbol),
        None => overlay::Left::Meter(&levels),
    };
    let buttons: &[(&str, isize)] = if learn {
        &[(text::undo(), overlay::UNDO)]
    } else if actions {
        &[
            (text::copy(), overlay::COPY),
            (text::close(), overlay::CLOSE),
        ]
    } else {
        &[]
    };
    // Words "Korrigieren" takes out while recording, as the overlay draws them.
    let struck: Vec<std::ops::Range<usize>> = strike
        .iter()
        .filter_map(|w| text.find(w.as_str()).map(|at| at..at + w.len()))
        .collect();
    let overlay = overlay::Overlay::new(mtm, |_| {});
    overlay.set_shortcuts(
        shortcuts.is_some(),
        shortcuts.filter(|t| !t.is_empty()).map(str::to_string),
    );
    let png = overlay
        .render_png(left, text, &struck, buttons)
        .ok_or_else(|| anyhow::anyhow!("rendering failed"))?;
    std::fs::write(out, png)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn bar_preview(out: &std::path::Path, recording: bool, meeting: bool) -> Result<()> {
    let mtm = platform::preview_app()?;
    let started = meeting.then(|| history::now() - 12 * 60 - 34);
    let png = bar::Bar::new(mtm, |_| {})
        .render_png(recording, started)
        .ok_or_else(|| anyhow::anyhow!("rendering failed"))?;
    std::fs::write(out, png)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn bar_preview(_: &std::path::Path, _: bool, _: bool) -> Result<()> {
    bail!("the bar is macOS-only")
}

#[cfg(not(target_os = "macos"))]
fn overlay_preview(
    _: &std::path::Path,
    _: Option<&str>,
    _: &str,
    _: bool,
    _: bool,
    _: &[String],
    _: Option<&str>,
) -> Result<()> {
    bail!("the overlay is macOS-only")
}

fn run(args: RunArgs) -> Result<()> {
    log_to_file_when_launched_as_app();
    let mut config = config::Config::load(&config::Config::path()).unwrap_or_else(|e| {
        eprintln!("settings: {e:#}; using defaults");
        config::Config::default()
    });
    if !args.triggers.is_empty() {
        // Flags win for this run; the settings window still saves to config.toml.
        let mode = args.mode.unwrap_or(Mode::Hybrid);
        config.triggers = args
            .triggers
            .iter()
            .map(|spec| {
                let (key, mode) = spec.split_once('@').unwrap_or((spec, mode.name()));
                config::TriggerConfig {
                    key: key.to_string(),
                    mode: mode.to_string(),
                    format: false,
                    correct: false,
                }
            })
            .collect();
    } else if let Some(mode) = args.mode {
        for trigger in &mut config.triggers {
            trigger.mode = mode.name().to_string();
        }
    }
    config.parsed()?;
    let model_dir = asr::model_path(args.model.as_deref().unwrap_or(&config.model));
    app::run(app::Settings {
        config,
        model_dir,
        replacements: replace::default_path(),
        paste: !args.no_paste,
    })
}

/// The log is rotated at start-up above this size (to `dictate.log.1`).
const LOG_MAX_BYTES: u64 = 2_000_000;

/// Started from Finder (Explorer) or at login there is no terminal; keep the log in the data folder.
fn log_to_file_when_launched_as_app() {
    use std::io::IsTerminal;
    if !platform::launched_as_app() && std::io::stderr().is_terminal() {
        return;
    }
    let _ = std::fs::create_dir_all(asr::data_dir());
    let log = asr::data_dir().join("dictate.log");
    // Every dictation goes into the log; keep one old file instead of growing for ever.
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > LOG_MAX_BYTES) {
        let _ = std::fs::rename(&log, log.with_extension("log.1"));
    }
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    {
        platform::log_to(&file);
    }
}
