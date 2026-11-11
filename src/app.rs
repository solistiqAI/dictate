//! The menu bar app: tray icon and menu on the main thread, the event tap on the main run loop,
//! recording and transcription on a worker thread.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::Result;
use dictate_core::linux::Tray;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::asr::{self, Asr};
use crate::audio::{self, Clip, Recorder};
use crate::bar::{self, Bar};
use crate::config::Config;
use crate::correct;
use crate::feedback::{self, Cue, Sounds};
use crate::history::{self, History};
use crate::learn::{self, Candidate, Kind};
use crate::llm;
use crate::media;
use crate::meeting;
use crate::overlay::{self, Left, Overlay};
use crate::platform::{self, MainThreadMarker, RECORDING, TapEvent};
use crate::protocol::{
    self, Event as PageEvent, MeetingView, PageConfig, PageModel, PageState, Request,
};
use crate::replace::{self, Corrector};
use crate::report;
use crate::stats;
use crate::talk::{Rig, Voices};
use crate::text;
use crate::trigger::{Action, Captured, Machine, Mode, Trigger};
use crate::vad;
use crate::window::MainWindow;

/// Below this a recording is room noise; Parakeet turns that into "Thank you." and the like.
const QUIET_DB: f32 = -60.0;
/// How long a transcript nothing could take stays on screen (or until "Schließen").
const RESULT_STAYS: Duration = Duration::from_secs(30);
/// Level meter refresh while recording.
pub(crate) const TICK: Duration = Duration::from_millis(100);
/// Live text: re-transcribe what we have at most this often (more when inference gets slower).
const LIVE_EVERY: Duration = Duration::from_millis(600);
/// How long the text field is watched for corrections after a paste, how long the pasted text
/// must stay unchanged before a correction is learned, and how long "Rückgängig" stays.
const LEARN_WATCH: Duration = Duration::from_secs(30);
const LEARN_SETTLE: Duration = Duration::from_millis(1500);
/// How often the field is read while watching. Short, so an edit sent right away was seen.
const LEARN_POLL: Duration = Duration::from_millis(300);
/// For an edit seen only once before it was sent: Gemma says whether it looks finished.
const LEARN_CHECK: &str = "You check edits to dictated text. Speech recognition may mishear \
words; the user then fixed the text and sent it. The edited text was read once just before \
sending, so it may have been caught while a word was still being typed. Given the dictated text, \
the text as read and one change, answer YES if the change is most likely a finished, deliberate \
correction (a real word or name that fits the sentence), or NO if it is most likely a word typed \
halfway, a typo or an accident. A new word that looks like the start of a longer word which \
would fit and sound more like the dictated one (\"Hendr\" for \"Hendrik\", \"Langs\" for \"LangSmith\") \
is unfinished. Answer with YES or NO only.";
const LEARN_STAYS: Duration = Duration::from_secs(8);
/// How long the question for a correction with another first sound stays.
const LEARN_ASK_STAYS: Duration = Duration::from_secs(12);
/// Live text stops for longer recordings (each pass re-reads everything); the final text still comes.
const LIVE_MAX_SECONDS: f32 = 90.0;

pub struct Settings {
    pub config: Config,
    pub model_dir: PathBuf,
    pub replacements: PathBuf,
    pub paste: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Loading,
    Ready,
    Recording,
    Transcribing,
    Problem,
}

#[derive(Debug)]
enum Ui {
    State(State, String),
    Transcript(String),
    Menu(MenuId),
    Overlay(Show),
    /// A message from a page: the main window's or the Souffleur panel's.
    Page(From, Request),
    /// The running Souffleur: a line, a card, its status, its end.
    Meeting(meeting::Update),
    /// A button on the overlay card (`overlay::COPY`, `overlay::CLOSE`, …).
    OverlayClick(isize),
    /// Corrections the user made in the text field after a paste: `sure` ones are learned at
    /// once, `ask` ones (another first sound) when the user clicks "Merken".
    Learn {
        sure: Vec<Candidate>,
        ask: Vec<Candidate>,
    },
    /// A button on the bar at the bottom (`bar::DICTATE`, …).
    BarClick(isize),
    /// The trigger pressed after "Ändern…" in the window.
    Captured(Captured),
    /// The level of a microphone test (`None`: it failed).
    MicLevel(Option<f32>),
    /// The desktop shows no tray icon (Linux, `platform::tray`): the window stands in for it.
    NoTray,
}

/// Which window a page message came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum From {
    Main,
    Panel,
}

/// What the overlay should show.
#[derive(Debug)]
pub(crate) enum Show {
    /// While recording: level meter and the text so far.
    Live {
        levels: Vec<f32>,
        text: String,
        /// Byte ranges of words "Korrigieren" takes out (shown struck through).
        struck: Vec<std::ops::Range<usize>>,
    },
    /// After recording, while the final transcription runs.
    Busy {
        text: String,
        struck: Vec<std::ops::Range<usize>>,
    },
    /// Nothing took the text: keep it on the card with "Korrigieren…", "Kopieren", "Schließen".
    Result {
        text: String,
    },
    /// A message that disappears by itself.
    Note {
        badge: String,
        text: String,
        seconds: u64,
    },
    Hide,
}

struct Items {
    status: MenuItem,
    last: MenuItem,
    correct: MenuItem,
    read: MenuItem,
    discuss: MenuItem,
    meeting: MenuItem,
    history: MenuItem,
    dictionary: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

impl Items {
    /// The fixed texts again, after the language changed (the status line follows by itself).
    fn relabel(&self, last_text: &str, meeting: bool) {
        if last_text.is_empty() {
            self.last.set_text(text::nothing_dictated());
        } else {
            self.last.set_text(text::last(&preview(last_text)));
        }
        self.correct.set_text(text::correct_last());
        self.read.set_text(text::read_selection());
        self.discuss.set_text(text::discuss());
        self.meeting.set_text(match meeting {
            true => text::stop_souffleur(),
            false => text::start_souffleur(),
        });
        self.history.set_text(text::history_menu());
        self.dictionary.set_text(text::dictionary_menu());
        self.settings.set_text(text::settings_menu());
        self.quit.set_text(text::quit());
    }
}

pub fn run(settings: Settings) -> Result<()> {
    let mut event_loop = EventLoopBuilder::<Ui>::with_user_event().build();
    platform::menu_bar_app(&mut event_loop);
    let proxy = event_loop.create_proxy();
    {
        let proxy = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = proxy.send_event(Ui::Menu(event.id));
        }));
    }

    stats::trim(&stats::path(), stats::now());
    let (tx, rx) = mpsc::channel::<TapEvent>();
    {
        let tx = tx.clone();
        std::thread::spawn(move || daily(tx));
    }
    let mut config = settings.config.clone();
    audio::choose(&config.microphone);
    let mut triggers = config.tap_triggers()?;
    let replacements_path = settings.replacements.clone();
    {
        // Raycast and `dictate souffleur start|stop`: as if the page asked.
        let proxy = proxy.clone();
        crate::local::serve(tx.clone(), move |start| {
            let request = if start {
                Request::StartMeeting
            } else {
                Request::StopMeeting
            };
            let _ = proxy.send_event(Ui::Page(From::Main, request));
        });
    }
    {
        let proxy = proxy.clone();
        let tx = tx.clone();
        std::thread::spawn(move || worker(settings, rx, tx, proxy));
    }

    let mut tray: Option<(TrayIcon, Items)> = None;
    let mut overlay: Option<Overlay> = None;
    let mut bar: Option<Bar> = None;
    let mut main_window: Option<MainWindow> = None;
    // The Souffleur: its side panel, the running meeting, what it has so far.
    let mut panel: Option<MainWindow> = None;
    let mut meeting: Option<meeting::Handle> = None;
    let mut view = MeetingView::default();
    // What the last learned correction replaced, for "Rückgängig": the settings and, when the
    // replacements file changed, its old text.
    let mut undo: Option<(Config, Option<String>)> = None;
    // Corrections the overlay asks about ("Merken"), until the next one.
    let mut offered: Vec<Candidate> = Vec::new();
    let data = asr::data_dir();
    let mut history = History::load(
        &data.join("history.jsonl"),
        &[data.join("dictate.log.1"), data.join("dictate.log")],
    );
    let mut hide_overlay_at: Option<Instant> = None;
    let mut tap = None;
    let mut last_text = String::new();
    // What the worker last reported; shown unless the keyboard tap is missing.
    let mut worker_state = (State::Loading, text::loading_model().to_string());
    // What the menu bar shows now. Only touch the status item on a change: setting its image
    // resizes it, which wakes this loop again, and doing it every pass spun the main thread at
    // 90 % CPU and starved the overlay.
    let mut shown: Option<(State, String)> = None;
    // The window's trigger row waiting for a key press, and whether the page needs a new state.
    let mut capturing: Option<usize> = None;
    let mut page_dirty = false;
    let mut was_trusted = platform::is_trusted();
    let mut quit_item: Option<MenuId> = None;
    // No tray icon on this desktop: the window is all there is, closing it quits.
    let mut no_tray = false;
    let icons = Icons::new();
    event_loop.run(move |event, target, control_flow| {
        let now = Instant::now();
        if let (Some(at), Some(o)) = (hide_overlay_at, &overlay)
            && now >= at
        {
            o.hide();
            hide_overlay_at = None;
        }
        let wake = hide_overlay_at.unwrap_or(now + Duration::from_secs(1));
        *control_flow = ControlFlow::WaitUntil(wake.min(now + Duration::from_secs(1)));
        let mtm = MainThreadMarker::new().expect("main thread");
        // A page to show in the main window (opened if needed), with a dictation to correct.
        let mut show_page: Option<(&str, Option<usize>)> = None;
        let mut show_panel = false;
        // Start the Souffleur (menu, page, bar); it opens the panel.
        let mut begin_meeting = false;
        // The meeting changed as a whole (started, ended): both pages get it again.
        let mut meeting_dirty = false;
        // The language of the menus changed (settings).
        let mut relabel = false;
        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = Some(build_tray().expect("menu bar icon"));
                quit_item = Some(edit_menu());
                // After a restart the last dictation is still there to copy and correct.
                if let (Some((_, items)), Some(entry)) = (&tray, history.entries().last()) {
                    items.last.set_text(text::last(&preview(&entry.text)));
                    items.last.set_enabled(true);
                    items.correct.set_enabled(true);
                    last_text = entry.text.clone();
                }
                // A desktop without a tray (GNOME without the extension) would show nothing.
                {
                    let proxy = proxy.clone();
                    std::thread::spawn(move || {
                        let tray = Tray::settle(5, platform::tray, || {
                            std::thread::sleep(Duration::from_secs(1))
                        });
                        if tray.open_window_at_start() {
                            let _ = proxy.send_event(Ui::NoTray);
                        }
                    });
                }
                let proxy = proxy.clone();
                overlay = Some(Overlay::new(mtm, {
                    let proxy = proxy.clone();
                    move |tag| {
                        let _ = proxy.send_event(Ui::OverlayClick(tag));
                    }
                }));
                bar = Some(Bar::new(mtm, move |tag| {
                    let _ = proxy.send_event(Ui::BarClick(tag));
                }));
                // Without Accessibility nothing works; show where to fix it right away.
                if !platform::is_trusted() {
                    show_page = Some(("general", None));
                }
            }
            // Opening the app again (Spotlight, Finder, Dock) shows the window.
            Event::Reopen { .. } => show_page = Some(("history", None)),
            Event::WindowEvent {
                window_id, event, ..
            } if main_window.as_ref().is_some_and(|w| w.id() == window_id) => match event {
                // Closing drops the webview; it costs memory only while the window is open.
                WindowEvent::CloseRequested => {
                    if capturing.take().is_some() {
                        platform::stop_capture();
                    }
                    main_window = None;
                    // Back to a menu bar app: no Dock icon, no ⌘-Tab entry.
                    platform::show_in_dock(target, false);
                    // Unless there is no menu bar to go back to (asked again: the user may
                    // have turned the extension on meanwhile).
                    if no_tray && platform::tray().quit_on_close() {
                        eprintln!("window closed without a tray: quitting");
                        quit(control_flow);
                    }
                }
                WindowEvent::Focused(false) => {
                    if let Some(row) = capturing.take()
                        && platform::stop_capture()
                        && let Some(w) = &main_window
                    {
                        // Left while waiting for a key: the next key is the user's again.
                        w.send(&PageEvent::Captured {
                            row,
                            key: None,
                            message: String::new(),
                        });
                    }
                }
                _ => {}
            },
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } if panel.as_ref().is_some_and(|w| w.id() == window_id) => {
                // Closing the panel does not stop the meeting; the menu or the page shows it again.
                panel = None;
            }
            Event::UserEvent(Ui::Meeting(update)) => {
                let event = match update {
                    meeting::Update::Line(line) => {
                        match view.lines.iter().position(|l| l.id == line.id) {
                            Some(i) if line.text.is_empty() => {
                                view.lines.remove(i);
                            }
                            Some(i) => view.lines[i] = line.clone(),
                            None if line.text.is_empty() => {}
                            None => view.lines.push(line.clone()),
                        }
                        Some(PageEvent::MeetingLine { line })
                    }
                    meeting::Update::Card(card) => {
                        match view.cards.iter().position(|c| c.id == card.id) {
                            Some(i) => view.cards[i] = card.clone(),
                            None => view.cards.push(card.clone()),
                        }
                        Some(PageEvent::MeetingCard { card })
                    }
                    meeting::Update::Status(status) => {
                        view.status = status.clone();
                        Some(PageEvent::MeetingStatus { status })
                    }
                    meeting::Update::Highlights(highlights) => {
                        view.highlights = highlights.clone();
                        Some(PageEvent::MeetingHighlights { highlights })
                    }
                    meeting::Update::Ended(reason) => {
                        meeting = None;
                        view.running = false;
                        view.status.agent = "off".into();
                        view.status.mic_db = None;
                        view.status.system_db = None;
                        if let Some(reason) = reason {
                            view.status.problem = reason;
                        }
                        meeting_dirty = true;
                        if let Some((_, items)) = &tray {
                            items.meeting.set_text(text::start_souffleur());
                        }
                        None
                    }
                };
                if let Some(event) = event {
                    for w in [&main_window, &panel].into_iter().flatten() {
                        w.send(&event);
                    }
                }
            }
            Event::UserEvent(Ui::BarClick(tag)) => {
                match tag {
                    // Hands-free, like the trigger's tap: the text goes where the focus is.
                    bar::DICTATE => {
                        let _ = tx.send(TapEvent::Toggle);
                    }
                    bar::SOUFFLEUR => begin_meeting = true,
                    bar::PANEL => show_panel = true,
                    bar::STOP => {
                        if let Some(m) = &meeting {
                            m.stop();
                        }
                    }
                    _ => {
                        let page = if meeting.is_some() {
                            "meeting"
                        } else {
                            "general"
                        };
                        show_page = Some((page, None));
                    }
                }
            }
            Event::UserEvent(Ui::Overlay(show)) => {
                let Some(o) = &overlay else { return };
                hide_overlay_at = None;
                match show {
                    Show::Live {
                        levels,
                        text,
                        struck,
                    } => {
                        // While recording: Einstellungen and Souffleur beside the pill.
                        let time = view
                            .started
                            .filter(|_| meeting.is_some())
                            .map(|started| bar::elapsed(history::now() - started));
                        o.set_shortcuts(true, time);
                        o.show_marked(mtm, Left::Meter(&levels), &text, &struck, &[])
                    }
                    Show::Busy { text, struck } => {
                        o.set_shortcuts(false, None);
                        o.show_marked(mtm, Left::Symbol("…"), &text, &struck, &[])
                    }
                    Show::Result { text } => {
                        o.set_shortcuts(false, None);
                        let buttons = [
                            (text::copy(), overlay::COPY),
                            (text::close(), overlay::CLOSE),
                        ];
                        o.show(mtm, Left::Symbol(text::no_text_field()), &text, &buttons);
                        hide_overlay_at = Some(Instant::now() + RESULT_STAYS);
                        *control_flow = ControlFlow::WaitUntil(hide_overlay_at.unwrap());
                    }
                    Show::Note {
                        badge,
                        text,
                        seconds,
                    } => {
                        o.set_shortcuts(false, None);
                        o.show(mtm, Left::Symbol(&badge), &text, &[]);
                        hide_overlay_at = Some(Instant::now() + Duration::from_secs(seconds));
                        *control_flow = ControlFlow::WaitUntil(hide_overlay_at.unwrap());
                    }
                    Show::Hide => o.hide(),
                }
            }
            Event::UserEvent(Ui::OverlayClick(tag)) => {
                let Some(o) = &overlay else { return };
                match tag {
                    // The shortcuts beside the pill: the dictation stops without pasting (the
                    // window or panel would take the text), then the page or the Souffleur.
                    overlay::SETTINGS | overlay::SOUFFLEUR => {
                        let _ = tx.send(TapEvent::Cancel);
                        o.set_shortcuts(false, None);
                        if tag == overlay::SETTINGS {
                            let page = if meeting.is_some() {
                                "meeting"
                            } else {
                                "general"
                            };
                            show_page = Some((page, None));
                        } else if meeting.is_some() {
                            show_panel = true;
                        } else {
                            begin_meeting = true;
                        }
                    }
                    overlay::COPY => {
                        copy(&last_text);
                        o.show(mtm, Left::Symbol(text::copied_check()), "", &[]);
                        hide_overlay_at = Some(Instant::now() + Duration::from_millis(1200));
                    }
                    overlay::UNDO => {
                        let note = match undo.take() {
                            Some((old, rules)) => {
                                let restored = rules
                                    .map_or(Ok(()), |text| std::fs::write(&replacements_path, text))
                                    .map_err(anyhow::Error::from)
                                    .and_then(|()| old.save(&Config::path()));
                                match restored {
                                    Ok(()) => {
                                        let _ = tx.send(TapEvent::Configure(Box::new(old.clone())));
                                        config = old;
                                        page_dirty = true;
                                        text::undone().to_string()
                                    }
                                    Err(e) => format!("{e:#}"),
                                }
                            }
                            None => String::new(),
                        };
                        o.show(mtm, Left::Symbol(&note), "", &[]);
                        hide_overlay_at = Some(Instant::now() + Duration::from_millis(1200));
                    }
                    overlay::REMEMBER => {
                        let chosen = std::mem::take(&mut offered);
                        match chosen.is_empty() {
                            true => o.hide(),
                            false => {
                                eprintln!(
                                    "learn: {} correction(s) learned on request",
                                    chosen.len()
                                );
                                let _ = proxy.send_event(Ui::Learn {
                                    sure: chosen,
                                    ask: Vec::new(),
                                });
                            }
                        }
                    }
                    _ => {
                        o.hide();
                        hide_overlay_at = None;
                    }
                }
            }
            Event::UserEvent(Ui::Learn {
                sure: candidates,
                ask,
            }) => {
                let Some(o) = &overlay else { return };
                let Some(first) = candidates.first() else {
                    // Only doubtful ones: the overlay asks, nothing is learned unless clicked.
                    let Some(first) = ask.first() else { return };
                    let more = text::more_corrections(ask.len() - 1);
                    o.show(
                        mtm,
                        Left::Symbol(text::remember_question()),
                        &format!("{}{more}", first.describe()),
                        &[(text::remember(), overlay::REMEMBER)],
                    );
                    offered = ask;
                    hide_overlay_at = Some(Instant::now() + LEARN_ASK_STAYS);
                    *control_flow = ControlFlow::WaitUntil(hide_overlay_at.unwrap());
                    return;
                };
                // Learned at once; a doubtful one beside them is only in the log.
                for c in &ask {
                    eprintln!("learn: not asked (learned others): {}", c.describe());
                }
                let more = text::more_corrections(candidates.len() - 1);
                // Learned right away; "Rückgängig" for the rare wrong guess.
                match remember(&candidates, &config, &replacements_path) {
                    Ok((new, rules)) => {
                        let _ = tx.send(TapEvent::Configure(Box::new(new.clone())));
                        page_dirty = true;
                        undo = Some((std::mem::replace(&mut config, new), rules));
                    }
                    Err(e) => {
                        o.show(mtm, Left::Symbol("!"), &format!("{e:#}"), &[]);
                        hide_overlay_at = Some(Instant::now() + Duration::from_secs(4));
                        return;
                    }
                }
                let text = format!("{}{more}", first.describe());
                o.show(
                    mtm,
                    Left::Symbol(text::learned()),
                    &text,
                    &[(text::undo(), overlay::UNDO)],
                );
                hide_overlay_at = Some(Instant::now() + LEARN_STAYS);
                *control_flow = ControlFlow::WaitUntil(hide_overlay_at.unwrap());
            }
            Event::UserEvent(Ui::Captured(captured)) => {
                let Some(row) = capturing.take() else { return };
                let (key, message) = match captured {
                    Captured::Spec(spec) => (Some(spec), String::new()),
                    Captured::Cancelled => (None, String::new()),
                    Captured::Unknown(code) => (None, text::unknown_key(code)),
                };
                if let Some(w) = &main_window {
                    w.send(&PageEvent::Captured { row, key, message });
                }
            }
            Event::UserEvent(Ui::MicLevel(db)) => {
                if let Some(w) = &main_window {
                    w.send(&PageEvent::MicLevel { db, done: true });
                }
            }
            Event::UserEvent(Ui::NoTray) => {
                eprintln!(
                    "tray: no StatusNotifier host or XEmbed tray, the icon is not shown; \
                     opening the window (closing it quits)"
                );
                no_tray = true;
                // The page's banner says what the window stands in for.
                page_dirty = true;
                show_page = Some(("general", None));
            }
            Event::UserEvent(Ui::State(state, text)) => {
                worker_state = (state, text);
                page_dirty = true;
            }
            Event::UserEvent(Ui::Transcript(text)) => {
                if let Some((_, items)) = &tray {
                    items.last.set_text(text::last(&preview(&text)));
                    items.last.set_enabled(true);
                    items.correct.set_enabled(true);
                }
                if let Err(e) = history.push(&text, history::now()) {
                    eprintln!("history: {e:#}");
                }
                last_text = text;
                page_dirty = true;
            }
            Event::UserEvent(Ui::Menu(id)) => {
                let Some((_, items)) = &tray else { return };
                if id == *items.quit.id() || quit_item.as_ref() == Some(&id) {
                    quit(control_flow);
                } else if id == *items.read.id() {
                    let _ = tx.send(TapEvent::Read);
                } else if id == *items.discuss.id() {
                    let _ = tx.send(TapEvent::Discuss);
                } else if id == *items.meeting.id() {
                    match &meeting {
                        Some(m) => m.stop(),
                        None => begin_meeting = true,
                    }
                } else if id == *items.last.id() {
                    copy(&last_text);
                } else if id == *items.correct.id() {
                    show_page = Some(("history", history.entries().len().checked_sub(1)));
                } else if id == *items.history.id() {
                    show_page = Some(("history", None));
                } else if id == *items.dictionary.id() {
                    show_page = Some(("dictionary", None));
                } else if id == *items.settings.id() {
                    show_page = Some(("general", None));
                }
            }
            Event::UserEvent(Ui::Page(from, request)) => {
                let w = match from {
                    From::Main => main_window.as_ref(),
                    From::Panel => panel.as_ref(),
                };
                let Some(w) = w else { return };
                let note = |text: &str| {
                    w.send(&PageEvent::Note { text: text.into() });
                };
                // Saves new settings and hands them to the worker; the page gets the new state.
                let mut apply = |new: Config, config: &mut Config| match save_config(&new, &tx) {
                    Ok(parsed) => {
                        if new.triggers != config.triggers {
                            triggers = parsed;
                            tap = None; // reinstalled below with the new triggers
                        }
                        *config = new;
                    }
                    Err(e) => note(&e),
                };
                match request {
                    Request::Load => {
                        eprintln!("window: page loaded");
                        page_dirty = true;
                        w.loaded();
                        w.send(&PageEvent::Meeting {
                            meeting: Box::new(view.clone()),
                        });
                    }
                    Request::Copy { text } => {
                        copy(&text);
                        note(text::copied());
                    }
                    Request::Find { original, edited } => {
                        let found = learn::candidates(&original, &edited, &config.vocabulary());
                        w.send(&PageEvent::Candidates {
                            original,
                            edited,
                            candidates: found.iter().map(Into::into).collect(),
                        });
                    }
                    Request::SaveEntry { id, text, learn } => {
                        if let Err(e) = history.replace(id, &text) {
                            eprintln!("history: {e:#}");
                        }
                        if id + 1 == history.entries().len() {
                            last_text = text;
                        }
                        let chosen: Vec<Candidate> = learn.iter().map(Into::into).collect();
                        if chosen.is_empty() {
                            note(text::saved());
                        } else {
                            match remember(&chosen, &config, &replacements_path) {
                                Ok((new, _)) => {
                                    let _ = tx.send(TapEvent::Configure(Box::new(new.clone())));
                                    config = new;
                                    note(&text::saved_learned(chosen.len()));
                                }
                                Err(e) => note(&format!("{e:#}")),
                            }
                        }
                        page_dirty = true;
                    }
                    Request::Config { config: page } => {
                        if page.start_at_login != platform::starts_at_login()
                            && let Err(e) = platform::set_start_at_login(page.start_at_login)
                        {
                            note(&format!("{e:#}"));
                        }
                        apply(page.apply(&config), &mut config);
                        let lang = text::Lang::resolve(
                            &config.ui_language,
                            &platform::preferred_languages(),
                        );
                        if lang != text::lang() {
                            text::set(lang);
                            relabel = true;
                        }
                        page_dirty = true;
                    }
                    Request::DownloadModel { name } if llm::find(&name).is_some() => {
                        // A text model for "Korrigieren": chosen now, used once it is here.
                        let mut new = config.clone();
                        new.correct_model = name.clone();
                        apply(new, &mut config);
                        if llm::DOWNLOADING.load(std::sync::atomic::Ordering::SeqCst) {
                            note(text::already_downloading());
                        } else {
                            note(text::downloading_gemma());
                            let tx = tx.clone();
                            std::thread::spawn(move || {
                                let result = llm::download(&name)
                                    .map(|_| ())
                                    .map_err(|e| format!("{e:#}"));
                                let _ = tx.send(TapEvent::Downloaded(name, result));
                            });
                        }
                        page_dirty = true;
                    }
                    Request::DownloadModel { name } => {
                        // Choosing a model that is not there downloads it (worker), then switches.
                        let mut new = config.clone();
                        new.model = name;
                        apply(new, &mut config);
                        page_dirty = true;
                    }
                    Request::Words { words } => {
                        let mut new = config.clone();
                        new.set_vocabulary(&words);
                        apply(new, &mut config);
                        page_dirty = true;
                    }
                    Request::Replacements { replacements } => {
                        let before =
                            std::fs::read_to_string(&replacements_path).unwrap_or_else(|_| {
                                include_str!("../replacements.example.txt").to_string()
                            });
                        let rules: Vec<(String, String)> = replacements
                            .into_iter()
                            .map(|r| (r.heard, r.meant))
                            .collect();
                        match std::fs::write(
                            &replacements_path,
                            replace::with_rules(&before, &rules),
                        ) {
                            Ok(()) => {
                                let _ = tx.send(TapEvent::Configure(Box::new(config.clone())));
                            }
                            Err(e) => note(&format!("{e:#}")),
                        }
                        page_dirty = true;
                    }
                    Request::Try { text } => {
                        let result =
                            Corrector::load(&replacements_path, &config.vocabulary()).apply(&text);
                        w.send(&PageEvent::Tried { text, result });
                    }
                    Request::RecordTrigger { row } => {
                        if tap.is_none() {
                            w.send(&PageEvent::Captured {
                                row,
                                key: None,
                                message: text::accessibility_first().into(),
                            });
                        } else {
                            capturing = Some(row);
                            let proxy = proxy.clone();
                            platform::start_capture(move |captured| {
                                let _ = proxy.send_event(Ui::Captured(captured));
                            });
                        }
                    }
                    Request::MicTest => {
                        let _ = tx.send(TapEvent::MicTest);
                    }
                    Request::OpenPrivacy { what } => {
                        if what == "microphone" {
                            if platform::microphone_permission().is_none() {
                                let _ = tx.send(TapEvent::MicTest); // makes macOS ask
                            } else {
                                platform::open_privacy("Microphone");
                            }
                        } else {
                            platform::ensure_accessibility(); // puts Dictate into the list
                            platform::open_privacy("Accessibility");
                        }
                    }
                    Request::OpenFolder => platform::open(&asr::data_dir(), false),
                    Request::StartMeeting => {
                        begin_meeting = true;
                        show_panel = true;
                    }
                    Request::StopMeeting => {
                        if let Some(m) = &meeting {
                            m.stop();
                        }
                    }
                    Request::AskMeeting { text } => match &meeting {
                        Some(m) if !text.trim().is_empty() => m.ask(text.trim().to_string()),
                        Some(_) => {}
                        None => note(text::souffleur_not_running()),
                    },
                    Request::OpenPanel => show_panel = true,
                    Request::OpenPage { page } => {
                        show_page = Some((page_name(&page), None));
                    }
                    Request::OpenUrl { url } => {
                        if ["https://", "http://", "notion://", "slack://", "mailto:"]
                            .iter()
                            .any(|scheme| url.starts_with(scheme))
                        {
                            platform::open_url(&url);
                        }
                    }
                    Request::OpenMeetings => {
                        let folder = meeting::folder();
                        let _ = std::fs::create_dir_all(&folder);
                        platform::open(&folder, false);
                    }
                    Request::Drag => w.drag(),
                }
            }
            _ => {}
        }
        if let Some((page, entry)) = show_page {
            if main_window.is_none() {
                let proxy = proxy.clone();
                // While the window is open Dictate is a regular app: Dock icon, ⌘-Tab, menu bar.
                platform::show_in_dock(target, true);
                match MainWindow::open(target, move |request| {
                    let _ = proxy.send_event(Ui::Page(From::Main, request));
                }) {
                    Ok(w) => main_window = Some(w),
                    Err(e) => {
                        eprintln!("{e:#}");
                        platform::show_in_dock(target, false);
                    }
                }
            }
            if let Some(w) = &main_window {
                platform::activate(mtm);
                w.focus();
                w.send(&PageEvent::Show {
                    page: page.into(),
                    entry,
                });
            }
        }
        if relabel {
            if let Some((_, items)) = &tray {
                items.relabel(&last_text, meeting.is_some());
            }
            quit_item = Some(edit_menu());
            shown = None;
        }
        if begin_meeting && meeting.is_none() {
            meeting = Some(start_meeting(&config, &replacements_path, &proxy));
            view = MeetingView {
                running: true,
                started: Some(history::now()),
                ..MeetingView::default()
            };
            meeting_dirty = true;
            show_panel = true;
            if let Some((_, items)) = &tray {
                items.meeting.set_text(text::stop_souffleur());
            }
        }
        // The bar: always there unless switched off; it shows a running dictation and Souffleur.
        if let Some(b) = &bar {
            b.set_visible(mtm, config.show_bar && tray.is_some());
            b.set_recording(mtm, worker_state.0 == State::Recording);
            b.set_meeting(mtm, view.started.filter(|_| meeting.is_some()));
            b.tick(mtm, history::now());
        }
        if show_panel {
            if panel.is_none() {
                let proxy = proxy.clone();
                match MainWindow::open_panel(target, move |request| {
                    let _ = proxy.send_event(Ui::Page(From::Panel, request));
                }) {
                    Ok(w) => panel = Some(w),
                    Err(e) => eprintln!("{e:#}"),
                }
            }
            if let Some(w) = &panel {
                platform::activate(mtm);
                w.focus();
            }
        }
        if meeting_dirty {
            let event = PageEvent::Meeting {
                meeting: Box::new(view.clone()),
            };
            for w in [&main_window, &panel].into_iter().flatten() {
                w.send(&event);
            }
        }
        // The keyboard tap needs Accessibility; it appears as soon as that is granted.
        let trusted = platform::is_trusted();
        if trusted != was_trusted {
            was_trusted = trusted;
            page_dirty = true;
        }
        if tap.is_none() && trusted && tray.is_some() {
            match platform::install_tap(triggers.clone(), tx.clone()) {
                Ok(t) => tap = Some(t),
                Err(e) => eprintln!("{e:#}"),
            }
        }
        let (state, text) = if tap.is_none() {
            (
                State::Problem,
                text::accessibility_missing_open().to_string(),
            )
        } else if worker_state.0 == State::Ready {
            (State::Ready, text::ready_with(&config.describe()))
        } else {
            worker_state.clone()
        };
        if page_dirty {
            page_dirty = false;
            let status = if tap.is_none() {
                (State::Problem, text::accessibility_missing().to_string())
            } else if worker_state.0 == State::Ready {
                (State::Ready, text::ready().to_string())
            } else {
                worker_state.clone()
            };
            let event = PageEvent::State {
                state: Box::new(page_state(
                    &status,
                    trusted,
                    &config,
                    &history,
                    &replacements_path,
                    no_tray,
                )),
            };
            for w in [&main_window, &panel].into_iter().flatten() {
                w.send(&event);
            }
        }
        if let Some((icon, items)) = &tray
            && shown.as_ref() != Some(&(state, text.clone()))
        {
            if shown.as_ref().is_none_or(|(s, _)| *s != state) {
                show_state(icon, &icons, state);
            }
            items.status.set_text(&text);
            shown = Some((state, text));
        }
    })
}

/// Ends the app: the sound and the paused media as they were.
fn quit(control_flow: &mut ControlFlow) {
    platform::restore_output();
    if let Some(media) = media::Adapter::find() {
        media.resume();
    }
    *control_flow = ControlFlow::Exit;
}

/// Starts the Souffleur with the settings as they are now; its updates come back as `Ui::Meeting`.
fn start_meeting(
    config: &Config,
    replacements: &std::path::Path,
    proxy: &EventLoopProxy<Ui>,
) -> meeting::Handle {
    let proxy = std::sync::Mutex::new(proxy.clone());
    let sink: meeting::Sink = std::sync::Arc::new(move |update| {
        let _ = proxy.lock().unwrap().send_event(Ui::Meeting(update));
    });
    meeting::start(
        meeting::Setup {
            config: config.clone(),
            replacements: replacements.to_path_buf(),
            folder: meeting::folder(),
            replay: None,
        },
        sink,
    )
}

/// A page the window knows, by the name the page sent.
fn page_name(page: &str) -> &'static str {
    match page {
        "history" => "history",
        "dictionary" => "dictionary",
        "shortcuts" => "shortcuts",
        "recognition" => "recognition",
        "meeting" => "meeting",
        _ => "general",
    }
}

/// Checks and saves new settings and hands them to the worker; returns the parsed triggers.
fn save_config(new: &Config, tx: &mpsc::Sender<TapEvent>) -> Result<Vec<Trigger>, String> {
    let parsed = new.parsed().map_err(|e| format!("{e:#}"))?;
    if parsed.is_empty() {
        return Err(text::at_least_one_shortcut().into());
    }
    let parsed = new.tap_triggers().map_err(|e| format!("{e:#}"))?;
    new.save(&Config::path()).map_err(|e| format!("{e:#}"))?;
    audio::choose(&new.microphone);
    let _ = tx.send(TapEvent::Configure(Box::new(new.clone())));
    Ok(parsed)
}

/// Everything the page shows.
fn page_state(
    status: &(State, String),
    trusted: bool,
    config: &Config,
    history: &History,
    replacements: &std::path::Path,
    no_tray: bool,
) -> PageState {
    let permission = |p: Option<bool>| match p {
        Some(true) => "granted",
        Some(false) => "missing",
        None => "unknown",
    };
    let rules = std::fs::read_to_string(replacements).unwrap_or_default();
    PageState {
        status: match status.0 {
            State::Loading => "loading",
            State::Ready => "ready",
            State::Recording => "recording",
            State::Transcribing => "transcribing",
            State::Problem => "problem",
        }
        .into(),
        status_text: status.1.clone(),
        accessibility: permission(Some(trusted)).into(),
        microphone: permission(platform::microphone_permission()).into(),
        microphone_name: audio::describe_input().unwrap_or_else(|e| format!("{e:#}")),
        microphones: audio::input_names(),
        config: PageConfig::from(config, platform::starts_at_login()),
        words: protocol::words(&config.vocabulary(), &crate::profile::current().dictionary),
        replacements: replace::rules(&rules)
            .into_iter()
            .map(|(heard, meant)| protocol::Replacement { heard, meant })
            .collect(),
        models: asr::available()
            .map(|m| {
                let (label, size) = protocol::split_label(m.label);
                PageModel {
                    name: m.name.into(),
                    label,
                    size: text::model_detail(&size),
                    downloaded: asr::installed(m, &asr::model_dir(m.name)),
                    progress: None,
                }
            })
            .collect(),
        correct_models: llm::MODELS
            .iter()
            .map(|m| {
                let (label, size) = protocol::split_label(m.label);
                PageModel {
                    name: m.name.into(),
                    label,
                    size: text::model_detail(&size),
                    downloaded: llm::path(m).exists(),
                    progress: None,
                }
            })
            .chain(cfg!(target_os = "macos").then(|| PageModel {
                name: "apple".into(),
                label: "Apple Intelligence".into(),
                size: text::apple_detail().into(),
                downloaded: true,
                progress: None,
            }))
            .collect(),
        history: protocol::entries(history.entries()),
        lang: text::lang().code().into(),
        report_target: crate::profile::current()
            .report_target()
            .map(|target| target.to_string()),
        profile: crate::profile::current().name.clone(),
        platform: protocol::platform().into(),
        no_tray,
    }
}

/// The model "Korrigieren" should use: none to load while no shortcut corrects.
fn correct_choice(config: &Config) -> String {
    // Gemma both corrects and lays out.
    match config.triggers.iter().any(|t| t.correct || t.format) {
        true => config.correct_model.clone(),
        false => "apple".into(),
    }
}

/// Gets the choice ready (`correct::prepare`); logs what corrections use when that changes.
fn prepare_correct(choice: &str, before: String) -> String {
    let now = correct::prepare(choice);
    if now != before {
        eprintln!("correct: {now}");
    }
    now
}

fn copy(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text.to_string());
    }
}

/// The app's main menu, seen while the window is open (the app is in the Dock then): without it
/// ⌘C/⌘V/⌘Z/⌘A do nothing in the window's text fields and ⌘W/⌘Q do not work. Returns the quit
/// item's id.
fn edit_menu() -> MenuId {
    let edit = Submenu::with_items(
        text::edit_menu(),
        true,
        &[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ],
    );
    // Our own quit item (⌘Q), handled like the one in the menu bar menu.
    let quit = MenuItem::new(text::quit(), true, "CmdOrCtrl+Q".parse().ok());
    let id = quit.id().clone();
    let app = Submenu::with_items(
        "Dictate",
        true,
        &[
            &PredefinedMenuItem::close_window(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ],
    );
    if let (Ok(app), Ok(edit)) = (app, edit) {
        let menu = Menu::new();
        if menu.append_items(&[&app, &edit]).is_ok() {
            // The menu must outlive this function; it lives as long as the app.
            platform::set_app_menu(menu);
        }
    }
    id
}

/// Saves corrections: words into the dictionary (`config.toml`), rules into the replacements
/// file. Returns the new settings for the worker and, when the file changed, its old text.
fn remember(
    chosen: &[Candidate],
    config: &Config,
    replacements: &std::path::Path,
) -> Result<(Config, Option<String>)> {
    let mut new = config.clone();
    let before = match std::fs::read_to_string(replacements) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut rules = before.clone();
    for c in chosen {
        match c.kind {
            Kind::Dictionary => {
                let mut words = new.vocabulary();
                if !words.contains(&c.meant) {
                    words.push(c.meant.clone());
                    new.set_vocabulary(&words);
                }
            }
            Kind::Replacement => rules = learn::add_replacement(&rules, &c.heard, &c.meant),
        }
    }
    let changed = rules != before;
    if changed {
        std::fs::write(replacements, rules)?;
    }
    new.save(&Config::path())?;
    for c in chosen {
        stats::record(stats::Event::Learned {
            at: stats::now(),
            heard: c.heard.clone(),
            meant: c.meant.clone(),
        });
    }
    Ok((new, changed.then_some(before)))
}

/// Once a day: the profile is fetched again from its `update_from` (`profile.rs`; a change, e.g.
/// new default words, applies at once), and, when switched on (read from `config.toml` each time,
/// so the toggle applies at once), the anonymous usage report goes to the profile's standing issue
/// (`report.rs`). Quiet when `gh` is missing or not signed in: the log says so, the next try is in
/// 30 minutes.
fn daily(tx: mpsc::Sender<TapEvent>) {
    loop {
        std::thread::sleep(Duration::from_secs(60));
        let config = Config::load(&Config::path()).unwrap_or_default();
        match crate::profile::update_if_due() {
            Ok(true) => {
                eprintln!("profile: updated");
                let _ = tx.send(TapEvent::Configure(Box::new(config.clone())));
            }
            Ok(false) => {}
            Err(e) => eprintln!("profile: {e:#}"),
        }
        match report::post_if_due(&config) {
            Ok(true) => eprintln!("report: posted"),
            Ok(false) => {}
            Err(e) => eprintln!("report: {e:#}"),
        }
        std::thread::sleep(Duration::from_secs(30 * 60));
    }
}

fn build_tray() -> Result<(TrayIcon, Items)> {
    let items = Items {
        status: MenuItem::new(text::loading_model(), false, None),
        // "Zuletzt: …": a click copies it.
        last: MenuItem::new(text::nothing_dictated(), false, None),
        correct: MenuItem::new(text::correct_last(), false, None),
        read: MenuItem::new(text::read_selection(), true, None),
        discuss: MenuItem::new(text::discuss(), true, None),
        meeting: MenuItem::new(text::start_souffleur(), true, None),
        history: MenuItem::new(text::history_menu(), true, None),
        dictionary: MenuItem::new(text::dictionary_menu(), true, None),
        settings: MenuItem::new(text::settings_menu(), true, None),
        quit: MenuItem::new(text::quit(), true, None),
    };
    let menu = Menu::new();
    menu.append_items(&[
        &items.status,
        &items.last,
        &items.correct,
        &PredefinedMenuItem::separator(),
        &items.read,
        &items.discuss,
        &PredefinedMenuItem::separator(),
        &items.meeting,
        &PredefinedMenuItem::separator(),
        &items.history,
        &items.dictionary,
        &items.settings,
        &PredefinedMenuItem::separator(),
        &items.quit,
    ])?;
    let icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Dictate")
        .with_icon(brand_icon())
        .with_icon_as_template(true)
        .with_title("…")
        .build()?;
    Ok((icon, items))
}

/// The menu bar images, drawn once.
struct Icons {
    brand: Icon,
    dot: Icon,
}

impl Icons {
    fn new() -> Self {
        Icons {
            brand: brand_icon(),
            dot: dot_icon(),
        }
    }
}

fn show_state(icon: &TrayIcon, icons: &Icons, state: State) {
    match state {
        State::Recording => {
            let _ = icon.set_icon_with_as_template(Some(icons.dot.clone()), false);
            icon.set_title(None::<&str>);
        }
        _ => {
            let _ = icon.set_icon_with_as_template(Some(icons.brand.clone()), true);
            let title = match state {
                State::Loading | State::Transcribing => Some("…"),
                State::Problem => Some("!"),
                State::Ready | State::Recording => None,
            };
            icon.set_title(title);
        }
    }
}

fn preview(text: &str) -> String {
    let short: String = text.chars().take(40).collect();
    if short.len() < text.len() {
        format!("{short}…")
    } else {
        short
    }
}

/// `tx` feeds the worker's own queue (for downloads that end later).
fn worker(
    settings: Settings,
    rx: Receiver<TapEvent>,
    tx: mpsc::Sender<TapEvent>,
    ui: EventLoopProxy<Ui>,
) {
    let state = |state: State, text: &str| {
        eprintln!("{} {text}", crate::history::log_time());
        crate::local::READY.store(
            matches!(state, State::Ready | State::Recording | State::Transcribing),
            std::sync::atomic::Ordering::Relaxed,
        );
        let _ = ui.send_event(Ui::State(state, text.to_string()));
    };
    let show = |show: Show| {
        let _ = ui.send_event(Ui::Overlay(show));
    };
    state(State::Loading, text::loading_model());
    let mut model_dir = settings.model_dir.clone();
    let mut asr = match Asr::load(&model_dir) {
        Ok(asr) => asr,
        Err(e) => return state(State::Problem, &text::model_missing(&format!("{e:#}"))),
    };
    // Dictionary words and weight for the decoder; set again after a model switch.
    let mut boost = (settings.config.vocabulary(), settings.config.boost());
    asr.set_boost(&boost.0, boost.1);
    let mut language = settings.config.language.clone();
    asr.set_language(&language);
    // The first inference is slow; pay for it now, not on the first dictation.
    let _ = asr.transcribe(vec![0.0; audio::MODEL_RATE as usize]);
    let mut corrector = Corrector::load(&settings.replacements, &settings.config.vocabulary())
        .with_commands(settings.config.voice_commands);
    // Learning from corrections in the text field (off by default) and the words it checks with.
    let mut learning = settings
        .config
        .learn_from_corrections
        .then(|| settings.config.vocabulary());
    match mic_check(Duration::from_millis(400)) {
        Ok(_) => state(State::Ready, text::ready()),
        Err(problem) => state(State::Problem, &problem),
    }

    let mut modes: Vec<Mode> = settings
        .config
        .parsed()
        .map(|t| t.iter().map(|t| t.mode).collect())
        .unwrap_or_default();
    // Which triggers lay the text out with the language model (`format.rs`), and whether the
    // running recording was started by one.
    let mut formats: Vec<bool> = settings.config.triggers.iter().map(|t| t.format).collect();
    let mut formatting = false;
    // The same for dropping self-corrections (`correct.rs`).
    let mut corrects: Vec<bool> = settings.config.triggers.iter().map(|t| t.correct).collect();
    let mut correcting = false;
    // Its model loads in the background as soon as a shortcut corrects, and stays loaded.
    let mut correct_choice = correct_choice(&settings.config);
    let mut correct_note = prepare_correct(&correct_choice, String::new());
    // Whether the overlay said once since start that Apple's model tidies because Gemma is missing.
    let mut told_without_gemma = false;
    let mut cues = Cues::new(
        feedback::prepare(&asr::data_dir().join("sounds"))
            .map_err(|e| eprintln!("sounds: {e:#}"))
            .ok()
            .map(|mut sounds| {
                sounds.set_start(&settings.config.start_sound);
                sounds
            }),
        feedback::volume(&settings.config.sounds),
        settings.config.mute_while_recording,
        settings.config.pause_media,
    );
    let started = Instant::now();
    // Trigger presses queued while the worker was busy (model load, mic test) are old news: a
    // replayed tap would start a hands-free recording by itself.
    let mut ignore_before = Instant::now();
    // The model the settings ask for (a switch only runs when this changes).
    let mut wanted = settings.config.model.clone();
    let mut triggers = settings.config.triggers.clone();
    // Load `wanted` once no recording runs.
    let mut switch_pending = false;
    let mut machine = Machine::default();
    let mut recorder: Option<Recorder> = None;
    let mut live = Live::default();
    // Reading aloud and talking things over (`talk.rs`).
    let mut voices = Voices::new(&settings.config);
    loop {
        // While recording, wake up regularly for the level meter and the live text.
        let event = if let Some(r) = &recorder {
            match rx.recv_timeout(TICK) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => {
                    live.tick(r, &mut asr, &corrector);
                    show(Show::Live {
                        levels: live.levels.iter().copied().collect(),
                        text: match live.text.is_empty() {
                            true => overlay::WAITING.into(),
                            false => live.text.clone(),
                        },
                        struck: live.struck(),
                    });
                    if voices.busy() {
                        let mut rig = Rig {
                            asr: &mut asr,
                            corrector: &corrector,
                            recorder: &mut recorder,
                            live: &mut live,
                            cues: &cues,
                            model: model_name(&model_dir),
                            show: &show,
                            state: &state,
                            tx: &tx,
                        };
                        voices.tick(&mut rig);
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(event) => event,
                Err(_) => break,
            }
        };
        let event = {
            let mut rig = Rig {
                asr: &mut asr,
                corrector: &corrector,
                recorder: &mut recorder,
                live: &mut live,
                cues: &cues,
                model: model_name(&model_dir),
                show: &show,
                state: &state,
                tx: &tx,
            };
            match voices.handle(event, &mut rig) {
                Some(event) => event,
                None => continue,
            }
        };
        let action = match event {
            TapEvent::Press(_, at) | TapEvent::Release(_, at) | TapEvent::Interrupt(_, at)
                if at < ignore_before =>
            {
                Action::None
            }
            TapEvent::Toggle => {
                let action = machine.toggle();
                if action == Action::Start {
                    formatting = false;
                    correcting = false;
                }
                action
            }
            TapEvent::Press(i, at) => {
                let action = machine.press(i, modes[i], at - started);
                if action == Action::Start {
                    formatting = formats.get(i).copied().unwrap_or(false);
                    correcting = corrects.get(i).copied().unwrap_or(false);
                }
                action
            }
            TapEvent::Release(i, at) => machine.release(i, modes[i], at - started),
            TapEvent::Interrupt(i, at) => machine.interrupt(i, at - started),
            TapEvent::Cancel => {
                machine.cancel();
                if recorder.take().is_some() {
                    cues.end(Some(Cue::Cancel));
                    show(Show::Hide);
                    state(State::Ready, text::cancelled());
                }
                Action::None
            }
            TapEvent::Configure(config) => {
                modes = config
                    .parsed()
                    .map(|t| t.iter().map(|t| t.mode).collect())
                    .unwrap_or_default();
                formats = config.triggers.iter().map(|t| t.format).collect();
                corrects = config.triggers.iter().map(|t| t.correct).collect();
                correct_choice = self::correct_choice(&config);
                correct_note = prepare_correct(&correct_choice, correct_note);
                corrector = Corrector::load(&settings.replacements, &config.vocabulary())
                    .with_commands(config.voice_commands);
                boost = (config.vocabulary(), config.boost());
                asr.set_boost(&boost.0, boost.1);
                if config.language != language {
                    language = config.language.clone();
                    asr.set_language(&language);
                }
                learning = config.learn_from_corrections.then(|| config.vocabulary());
                // New triggers end a running recording (the tap is reinstalled); a learned word
                // or another sound setting does not.
                if config.triggers != triggers {
                    triggers = config.triggers.clone();
                    machine.cancel();
                    if recorder.take().is_some() {
                        cues.end(None);
                    }
                }
                cues.volume = feedback::volume(&config.sounds);
                if let Some(sounds) = cues.sounds.as_mut() {
                    sounds.set_start(&config.start_sound);
                }
                cues.mute = config.mute_while_recording;
                cues.pause = config.pause_media;
                if config.model != wanted {
                    wanted = config.model.clone();
                    if needs_download(&wanted) {
                        state(State::Loading, &text::downloading(&wanted));
                        let (tx, name) = (tx.clone(), wanted.clone());
                        std::thread::spawn(move || {
                            let result = asr::download(&name)
                                .map(|_| ())
                                .map_err(|e| format!("{e:#}"));
                            let _ = tx.send(TapEvent::Downloaded(name, result));
                        });
                    } else {
                        switch_pending = true;
                    }
                }
                Action::None
            }
            TapEvent::Downloaded(name, result) if llm::find(&name).is_some() => {
                match result {
                    Err(e) => state(State::Problem, &text::download_failed(&name, &e)),
                    Ok(()) => {
                        correct_note = prepare_correct(&correct_choice, correct_note);
                        state(State::Ready, text::gemma_ready());
                    }
                }
                Action::None
            }
            TapEvent::Downloaded(name, result) => {
                match result {
                    Err(e) => {
                        state(State::Problem, &text::download_failed(&name, &e));
                        wanted.clear(); // choosing it again retries
                    }
                    Ok(()) => switch_pending |= name == wanted,
                }
                Action::None
            }
            // Voices took these already (`talk.rs`).
            TapEvent::Read
            | TapEvent::Discuss
            | TapEvent::DiscussSession(_)
            | TapEvent::EndTalk
            | TapEvent::Spoke(_)
            | TapEvent::Answer(..) => Action::None,
            TapEvent::MicTest => {
                match mic_check(Duration::from_secs(2)) {
                    Ok(level) => {
                        let _ = ui.send_event(Ui::MicLevel(Some(level)));
                        state(State::Ready, &text::mic_ok(level));
                    }
                    Err(problem) => {
                        let _ = ui.send_event(Ui::MicLevel(None));
                        state(State::Problem, &problem);
                    }
                }
                ignore_before = Instant::now();
                Action::None
            }
        };
        match action {
            Action::Start => match Recorder::start() {
                Ok(r) => {
                    recorder = Some(r);
                    live = Live::default();
                    // Gemma downloaded while the app ran (`dictate model download`, install.sh):
                    // it starts loading now, while the user speaks.
                    if (correcting || formatting) && !correct::live_ready() {
                        correct_note = prepare_correct(&correct_choice, correct_note);
                    }
                    // Tidy up finished sentences while speaking when this shortcut corrects.
                    live.tidy = correcting && correct::live_ready();
                    if live.tidy {
                        correct::live_begin();
                    }
                    cues.start();
                    show(Show::Live {
                        levels: Vec::new(),
                        text: overlay::WAITING.into(),
                        struck: Vec::new(),
                    });
                    state(State::Recording, text::recording());
                }
                Err(e) => {
                    machine.cancel();
                    let problem = text::mic_problem(&format!("{e:#}"));
                    stats::problem(&format!("recording could not start: {e:#}"));
                    show(Show::Note {
                        badge: "!".into(),
                        text: problem.clone(),
                        seconds: 4,
                    });
                    state(State::Problem, &problem);
                }
            },
            Action::Stop => {
                let Some(r) = recorder.take() else { continue };
                RECORDING.store(false, Ordering::Relaxed);
                cues.end(Some(Cue::Stop));
                state(State::Transcribing, text::transcribing());
                show(Show::Busy {
                    text: live.text.clone(),
                    struck: live.struck(),
                });
                correct::live_end();
                let clip = r.stop();
                let model = model_name(&model_dir);
                match transcribe(&mut asr, &corrector, clip, &model) {
                    Ok(Some(text)) => {
                        // "Formatieren" and "Korrigieren": one pass (`correct::tidy_text`).
                        let text = match correcting || formatting {
                            false => text,
                            true => {
                                let started = Instant::now();
                                let (tidied, note) =
                                    correct::tidy_text(&text, correcting, formatting);
                                eprintln!("tidy: {:.2} s, {note}", started.elapsed().as_secs_f32());
                                tidied
                            }
                        };
                        // Gemma chosen but not there: Apple's model tidied (slower) or, off the
                        // Mac, nothing did. Said once per start, after a paste (not over a card).
                        let without_gemma = (correcting || formatting)
                            && llm::find(&correct_choice).is_some()
                            && !correct::live_ready();
                        let learn = learning.as_ref().map(|words| (words.clone(), ui.clone()));
                        let pasted = deliver(&text, settings.paste, &show, learn);
                        if without_gemma && pasted && !told_without_gemma {
                            told_without_gemma = true;
                            show(Show::Note {
                                badge: "i".into(),
                                text: text::without_gemma().into(),
                                seconds: 7,
                            });
                        }
                        let _ = ui.send_event(Ui::Transcript(text));
                        state(State::Ready, text::ready());
                    }
                    Ok(None) => {
                        show(Show::Note {
                            badge: "–".into(),
                            text: text::nothing_heard().into(),
                            seconds: 2,
                        });
                        state(State::Ready, text::nothing_heard_status());
                    }
                    Err(problem) => {
                        show(Show::Note {
                            badge: "!".into(),
                            text: problem.clone(),
                            seconds: 5,
                        });
                        state(State::Problem, &problem);
                    }
                }
            }
            Action::Discard => {
                recorder.take();
                cues.end(None);
                show(Show::Hide);
                state(State::Ready, text::shortcut_discarded());
            }
            Action::None => {}
        }
        RECORDING.store(machine.is_recording(), Ordering::Relaxed);
        if switch_pending && recorder.is_none() {
            switch_pending = false;
            switch_model(&mut asr, &mut model_dir, &wanted, &state);
            asr.set_boost(&boost.0, boost.1);
            asr.set_language(&language);
            ignore_before = Instant::now();
        }
    }
}

/// The model folder's name, which is the model's name.
fn model_name(dir: &std::path::Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A known model that is not downloaded yet.
fn needs_download(model: &str) -> bool {
    asr::find(model).is_some_and(|known| !asr::installed(known, &asr::model_dir(model)))
}

/// Loads the model the settings name (downloaded already). On failure the current model stays.
fn switch_model(asr: &mut Asr, current: &mut PathBuf, model: &str, state: &impl Fn(State, &str)) {
    let (dir, note) = asr::resolve(model);
    if let Some(note) = note {
        eprintln!("{note}");
    }
    if dir == *current {
        return state(State::Ready, text::ready());
    }
    state(State::Loading, text::loading_model());
    match Asr::load(&dir) {
        Ok(new) => {
            *asr = new;
            *current = dir;
            let _ = asr.transcribe(vec![0.0; audio::MODEL_RATE as usize]);
            state(State::Ready, text::ready());
        }
        Err(e) => state(State::Problem, &text::model_problem(&format!("{e:#}"))),
    }
}

/// Start/stop cues, and what happens to other audio while recording (mute the Mac, pause the
/// music). The side effects run on one thread in order, so a quick stop never overtakes the pause.
pub(crate) struct Cues {
    sounds: Option<Sounds>,
    volume: f32,
    mute: bool,
    pause: bool,
    side: std::sync::mpsc::Sender<Side>,
}

enum Side {
    Begin {
        mute: bool,
        pause: bool,
    },
    End {
        cue: Option<(std::path::PathBuf, f32)>,
    },
}

impl Cues {
    fn new(sounds: Option<Sounds>, volume: f32, mute: bool, pause: bool) -> Self {
        let (side, rx) = std::sync::mpsc::channel::<Side>();
        let media = media::Adapter::find();
        if media.is_none() {
            eprintln!("media: adapter not in the app bundle, playback is not paused");
        }
        std::thread::spawn(move || {
            for job in rx {
                match job {
                    Side::Begin { mute, pause } => {
                        // Let the start cue play first; skip if the recording is already over.
                        std::thread::sleep(Duration::from_millis(250));
                        if RECORDING.load(Ordering::Relaxed) {
                            if let (true, Some(media)) = (pause, &media) {
                                media.pause_if_playing();
                            }
                            if mute {
                                platform::mute_output();
                            }
                        }
                    }
                    Side::End { cue } => {
                        platform::restore_output(); // sound back on first, so the cue is heard
                        if let Some((path, volume)) = cue {
                            platform::play(&path, volume);
                        }
                        if let Some(media) = &media {
                            media.resume();
                        }
                    }
                }
            }
        });
        Cues {
            sounds,
            volume,
            mute,
            pause,
            side,
        }
    }

    fn start(&self) {
        if let Some(sounds) = &self.sounds {
            platform::play(&sounds.path(Cue::Start), self.volume);
        }
        if self.mute || self.pause {
            let _ = self.side.send(Side::Begin {
                mute: self.mute,
                pause: self.pause,
            });
        }
    }

    /// Just the cue, no muting or pausing (a conversation has its own sound).
    pub(crate) fn chime(&self, cue: Cue) {
        if let Some(sounds) = &self.sounds {
            platform::play(&sounds.path(cue), self.volume);
        }
    }

    /// Stop or cancel with a cue; `None` ends silently (a discarded shortcut press).
    fn end(&self, cue: Option<Cue>) {
        let cue = cue.and_then(|cue| Some((self.sounds.as_ref()?.path(cue), self.volume)));
        let _ = self.side.send(Side::End { cue });
    }
}

/// Pastes into the focused text field, or, when nothing takes text, leaves the text in the
/// clipboard and says so in the overlay. True when it pasted (the overlay is free again).
/// With `learn` (dictionary words, UI), the field is watched for corrections afterwards.
fn deliver(
    text: &str,
    paste: bool,
    show: &impl Fn(Show),
    learn: Option<(Vec<String>, EventLoopProxy<Ui>)>,
) -> bool {
    if !paste {
        println!("{text}");
        show(Show::Hide);
        return false;
    }
    if platform::focused_accepts_text() == Some(false) {
        show(Show::Result {
            text: text.to_string(),
        });
        return false;
    }
    show(Show::Hide);
    // Read the field before the paste: the text around the insertion point locates it later.
    let field = learn.as_ref().and_then(|_| platform::Field::focused());
    // A space to go on after; not after a line break or a fresh list item.
    let pasted = match text.ends_with(char::is_whitespace) {
        true => text.to_string(),
        false => format!("{text} "),
    };
    if let Err(e) = platform::paste(&pasted) {
        eprintln!("paste: {e:#}");
        stats::problem(&format!("paste failed: {e:#}"));
        // Not lost: the card keeps it to copy (on Wayland without ydotool or wtype, say).
        show(Show::Result {
            text: text.to_string(),
        });
        return false;
    }
    if let (Some(field), Some((words, ui))) = (field, learn) {
        watch_corrections(field, text.to_string(), words, ui);
    }
    true
}

/// Bumped by every paste; an older watcher stops.
static WATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Watches the field after a paste for up to [`LEARN_WATCH`], until the focus moves or the next
/// paste. Once the pasted text has stayed the same for [`LEARN_SETTLE`] (still focused), or right
/// when the field is sent, emptied or gone after an edit that stood still for two reads, and differs
/// from the transcript in a few words that sound alike (a mishearing, not a rewording), those
/// corrections are learned and the overlay says so, with "Rückgängig"; close ones that begin with
/// another sound ("Radman" → "Admin") are offered with "Merken". Only the pasted stretch is
/// compared; the log says which of its changes were not learned and why, nothing else of the field
/// is logged or kept.
fn watch_corrections(
    field: platform::Field,
    pasted: String,
    words: Vec<String>,
    ui: EventLoopProxy<Ui>,
) {
    let generation = WATCH.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut last: Option<String> = None;
        let mut since = Instant::now();
        // The last region looked at, so a settled one is checked (and logged) once.
        let mut checked: Option<String> = None;
        // Why watching ended with an edit not looked at.
        let mut outside = false;
        let mut cleared = false;
        let offer = |region: &str| {
            let review = learn::review(pasted.trim(), region.trim(), &words);
            for why in &review.skipped {
                eprintln!("learn: not learned: {why}");
            }
            if review.sure.is_empty() && review.ask.is_empty() {
                return false;
            }
            if !review.sure.is_empty() {
                eprintln!("learn: {} correction(s) learned", review.sure.len());
            }
            for c in &review.ask {
                eprintln!("learn: asking, another first sound: {}", c.describe());
            }
            let _ = ui.send_event(Ui::Learn {
                sure: review.sure,
                ask: review.ask,
            });
            true
        };
        // An edit seen in one read only, just before the field was sent or went away: it may be
        // half typed, so nothing is learned by itself. The overlay asks ("Merken?") for each one
        // Gemma takes for a finished correction; without Gemma it asks for all of them.
        let offer_unsure = |region: &str| {
            let review = learn::review(pasted.trim(), region.trim(), &words);
            for why in &review.skipped {
                eprintln!("learn: not learned: {why}");
            }
            let ask: Vec<_> = review
                .sure
                .into_iter()
                .chain(review.ask)
                .filter(|c| match unfinished(&pasted, region, &c.describe()) {
                    Some(Ok(true)) => {
                        eprintln!(
                            "learn: not asked, Gemma takes it for unfinished: {}",
                            c.describe()
                        );
                        false
                    }
                    Some(Err(e)) => {
                        eprintln!("learn: Gemma's check failed ({e:#}), asking");
                        true
                    }
                    _ => true,
                })
                .collect();
            for c in &ask {
                eprintln!("learn: asking, seen once before sending: {}", c.describe());
            }
            if !ask.is_empty() {
                let _ = ui.send_event(Ui::Learn {
                    sure: Vec::new(),
                    ask,
                });
            }
        };
        // The last region read, and whether it was read only once (not yet steady).
        let unsure = |last: &Option<String>, checked: &Option<String>| {
            last.clone()
                .filter(|r| r.trim() != pasted.trim() && checked.as_deref() != Some(r.as_str()))
        };
        let mut misses = 0;
        let mut gone = false;
        // The region read twice in a row (unchanged for one poll): not a half-typed word.
        let mut steady: Option<String> = None;
        while started.elapsed() < LEARN_WATCH && WATCH.load(Ordering::Relaxed) == generation {
            std::thread::sleep(LEARN_POLL);
            // Focus moved on (or failed reads for about 2 s): the user is done with this text.
            let Some(now) = field.current_text() else {
                misses += 1;
                if misses >= 7 {
                    // Sending can take the field away too (a chat that rebuilds its composer):
                    // an edit that stood still counts, as when the field is emptied.
                    if let Some(done) = steady.take()
                        && checked.as_deref() != Some(done.as_str())
                    {
                        offer(&done);
                        return;
                    }
                    if let Some(once) = unsure(&last, &checked) {
                        offer_unsure(&once);
                        return;
                    }
                    gone = true;
                    break;
                }
                continue;
            };
            misses = 0;
            let region = learn::pasted_region(&field.text, field.at, &now);
            // Sent or emptied: the edit before it is done, so it counts without waiting.
            if now.trim().is_empty() || region.is_some_and(|r| r.trim().is_empty()) {
                cleared = true;
                if let Some(done) = steady.take()
                    && checked.as_deref() != Some(done.as_str())
                {
                    offer(&done);
                    return;
                }
                if let Some(once) = unsure(&last, &checked) {
                    offer_unsure(&once);
                    return;
                }
                continue;
            }
            cleared = false;
            let Some(region) = region else {
                outside = true;
                steady = None;
                continue; // changed outside the pasted text
            };
            outside = false;
            if last.as_deref() != Some(region) {
                last = Some(region.to_string());
                since = Instant::now();
                steady = None;
            } else {
                if region.trim() != pasted.trim() {
                    steady = Some(region.to_string());
                }
                if since.elapsed() >= LEARN_SETTLE && checked.as_deref() != Some(region) {
                    checked = Some(region.to_string());
                    if offer(region) {
                        return;
                    }
                }
            }
        }
        // An edit never looked at: an unsettled region may be a half-typed word, and after a
        // focus change the user is on to the next thing. The log says why.
        let edited = last
            .as_deref()
            .is_some_and(|r| r.trim() != pasted.trim() && checked.as_deref() != Some(r));
        if edited {
            let why = if cleared {
                "the text was sent or cleared before the edit stood still"
            } else if outside {
                "the field changed outside the pasted text too"
            } else if gone {
                "the focus moved (or the field went away) before the edit stood still"
            } else if WATCH.load(Ordering::Relaxed) != generation {
                "the next dictation came first"
            } else {
                "still being edited 30 s after the paste"
            };
            eprintln!("learn: not learned: {why}");
        }
    });
}

/// Whether Gemma takes `change` in the edited `region` for a word typed halfway or an accident;
/// None without Gemma.
fn unfinished(pasted: &str, region: &str, change: &str) -> Option<anyhow::Result<bool>> {
    let question = format!(
        "Dictated: {}\nSent: {}\nChange: {change}",
        pasted.trim(),
        region.trim()
    );
    correct::ask_local(LEARN_CHECK, &question, Duration::from_secs(5))
        .map(|answer| answer.map(|a| a.trim().to_uppercase().starts_with("NO")))
}

/// Level meter and live text while recording.
#[derive(Default)]
pub(crate) struct Live {
    levels: VecDeque<f32>,
    pub(crate) text: String,
    last_pass: Option<Instant>,
    every: Duration,
    /// Hand finished sentences to Gemma while recording (`correct::live_offer`).
    pub(crate) tidy: bool,
    /// The finished sentences of the last pass; one that is still there a pass later is done.
    finished: Vec<String>,
    offered: std::collections::HashSet<String>,
}

impl Live {
    fn tick(&mut self, recorder: &Recorder, asr: &mut Asr, corrector: &Corrector) {
        self.levels
            .push_back(recorder.recent_db(TICK.as_millis() as u32));
        while self.levels.len() > 8 {
            self.levels.pop_front();
        }
        let due = self
            .last_pass
            .is_none_or(|at| at.elapsed() >= self.every.max(LIVE_EVERY));
        if !due {
            return;
        }
        let clip = recorder.snapshot();
        if clip.seconds() < 0.5 || clip.seconds() > LIVE_MAX_SECONDS || clip.rms_db() < QUIET_DB {
            return;
        }
        let started = Instant::now();
        if let Ok(text) = clip
            .into_model_rate()
            .map(|samples| match vad::enabled() {
                true => vad::trim(samples).samples,
                false => samples,
            })
            .and_then(|samples| asr.transcribe_live(samples))
            && !text.is_empty()
        {
            self.text = corrector.apply(&text);
            if self.tidy {
                self.offer();
            }
        }
        // Back off when a pass gets slow, so the level meter keeps moving.
        self.every = started.elapsed() * 3;
        self.last_pass = Some(Instant::now());
    }
}

impl Live {
    /// Finished sentences that stayed the same over two passes go to Gemma, each once.
    fn offer(&mut self) {
        let now: Vec<String> = correct::sentences(&self.text)
            .into_iter()
            .map(|(_, s)| s.to_string())
            .collect();
        for sentence in &now {
            if self.finished.contains(sentence) && self.offered.insert(sentence.clone()) {
                correct::live_offer(sentence);
            }
        }
        self.finished = now;
    }

    /// Byte ranges of the live text's words Gemma cut so far.
    pub(crate) fn struck(&self) -> Vec<std::ops::Range<usize>> {
        if !self.tidy {
            return Vec::new();
        }
        let mut ranges = Vec::new();
        for (range, sentence) in correct::sentences(&self.text) {
            let Some(cut) = correct::live_marks(sentence) else {
                continue;
            };
            for (i, (at, word)) in correct::words_at(sentence).enumerate() {
                if cut.contains(&i) {
                    ranges.push(range.start + at..range.start + at + word.len());
                }
            }
        }
        ranges
    }
}

/// `Ok(None)` when there was nothing to transcribe; `Err` carries a message for the menu.
pub(crate) fn transcribe(
    asr: &mut Asr,
    corrector: &Corrector,
    clip: Clip,
    model: &str,
) -> Result<Option<String>, String> {
    let seconds = clip.seconds();
    let _ = clip.write_wav(&asr::data_dir().join("last.wav"));
    if clip.peak() == 0.0 {
        stats::problem("recording is digital silence (no microphone access?)");
        return Err(text::no_mic().into());
    }
    let level = clip.rms_db();
    let skipped = |reason: &str| {
        stats::record(stats::Event::Skipped {
            at: stats::now(),
            seconds,
            level_db: level,
            reason: reason.into(),
        })
    };
    if level < QUIET_DB {
        eprintln!("{seconds:.1} s too quiet ({level:.0} dBFS), skipped");
        skipped("quiet");
        return Ok(None);
    }
    let t = Instant::now();
    let mut samples = clip
        .into_model_rate()
        .map_err(|e| text::transcription_failed(&format!("{e:#}")))?;
    let mut trimmed = String::new();
    let mut speech_seconds = seconds;
    if vad::enabled() {
        let t = vad::trim(samples);
        trimmed = format!(", {}", t.describe());
        samples = t.samples;
        speech_seconds = samples.len() as f32 / audio::MODEL_RATE as f32;
    }
    let text = asr.transcribe(samples).map_err(|e| {
        stats::problem(&format!("transcription failed: {e:#}"));
        text::transcription_failed(&format!("{e:#}"))
    })?;
    if text.is_empty() {
        eprintln!("{seconds:.1} s at {level:.0} dBFS{trimmed}: no speech");
        skipped("no_speech");
        return Ok(None);
    }
    let text = corrector.finish(&text);
    eprintln!(
        "{seconds:.1} s at {level:.0} dBFS{trimmed} → {:.2} s: {text}",
        t.elapsed().as_secs_f32()
    );
    stats::record(stats::Event::Dictation {
        at: stats::now(),
        seconds,
        level_db: level,
        speech_seconds,
        took: t.elapsed().as_secs_f32(),
        words: text.split_whitespace().count() as u32,
        model: model.into(),
    });
    Ok(Some(text))
}

/// Records a moment. Also makes macOS ask for microphone access at startup, not mid-dictation.
/// Returns the level, or a problem for the menu (digital silence = no permission).
pub fn mic_check(duration: Duration) -> Result<f32, String> {
    eprintln!(
        "microphone: {}",
        audio::describe_input().map_err(|e| format!("{e:#}"))?
    );
    let recorder = Recorder::start().map_err(|e| text::mic_problem(&format!("{e:#}")))?;
    std::thread::sleep(duration);
    let clip = recorder.stop();
    if clip.samples.is_empty() || clip.peak() == 0.0 {
        return Err(text::no_mic().into());
    }
    Ok(clip.rms_db())
}

/// The voice bars (or the brand's menu bar image), as a template image (src/glyph.rs).
fn brand_icon() -> Icon {
    let (mut rgba, width, height) = crate::glyph::brand().unwrap_or_else(crate::glyph::render);
    // Windows has no template images: white bars for the (usually dark) taskbar.
    if cfg!(windows) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[..3].fill(255);
        }
    }
    Icon::from_rgba(rgba, width, height).expect("icon")
}

/// Red recording dot.
fn dot_icon() -> Icon {
    draw(|x, y| {
        let d = (x - 18.0).hypot(y - 18.0) - 8.0;
        [230, 57, 70, coverage(d)]
    })
}

fn draw(pixel: impl Fn(f32, f32) -> [u8; 4]) -> Icon {
    const SIZE: u32 = 36;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            rgba.extend_from_slice(&pixel(x as f32 + 0.5, y as f32 + 0.5));
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("icon")
}

/// Anti-aliased alpha from a signed distance (negative inside).
fn coverage(distance: f32) -> u8 {
    ((0.5 - distance).clamp(0.0, 1.0) * 255.0) as u8
}

#[cfg(test)]
mod tests {
    /// Gemma's check of edits seen once before sending; needs the model downloaded:
    /// `cargo test --release unfinished_edits -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn unfinished_edits_by_gemma() {
        crate::correct::prepare("gemma-4-e4b");
        let cases = [
            (
                "Ich habe den Screenshot von Henrik bekommen.",
                "Ich habe den Screenshot von Hendrik bekommen.",
                "Henrik",
                "Hendrik",
                false,
            ),
            (
                "Der Merch-Request sollte passen.",
                "Der Merge-Request sollte passen.",
                "Merch-Request",
                "Merge-Request",
                false,
            ),
            (
                "Haben wir langsmoth traces aktiviert?",
                "Haben wir LangSmith traces aktiviert?",
                "langsmoth",
                "LangSmith",
                false,
            ),
            (
                "Ich habe den Screenshot von Henrik bekommen.",
                "Ich habe den Screenshot von Hendr bekommen.",
                "Henrik",
                "Hendr",
                true,
            ),
            (
                "Haben wir langsmoth traces aktiviert?",
                "Haben wir Langs traces aktiviert?",
                "langsmoth",
                "Langs",
                true,
            ),
            (
                "Kannst du das deployen?",
                "Kannst du das deplo?",
                "deployen",
                "deplo",
                true,
            ),
            (
                "Wie kommt das Backend an Keychlock?",
                "Wie kommt das Backend an Keycloak?",
                "Keychlock",
                "Keycloak",
                false,
            ),
            (
                "Wie kommt das Backend an Keychlock?",
                "Wie kommt das Backend an Keycl?",
                "Keychlock",
                "Keycl",
                true,
            ),
            (
                "Ist das schon auf Prot gesetzt?",
                "Ist das schon auf Prod gesetzt?",
                "Prot",
                "Prod",
                false,
            ),
            (
                "Läuft das im Kubanetes Cluster?",
                "Läuft das im Kuber Cluster?",
                "Kubanetes",
                "Kuber",
                true,
            ),
            (
                "Läuft das im Kubanetes Cluster?",
                "Läuft das im Kubernetes Cluster?",
                "Kubanetes",
                "Kubernetes",
                false,
            ),
        ];
        let mut wrong = 0;
        for (pasted, region, heard, meant, expected) in cases {
            let change = crate::text::quoted_change(heard, meant);
            let got = super::unfinished(pasted, region, &change)
                .expect("gemma-4-e4b loaded")
                .unwrap();
            println!("{change}: unfinished {got} (expected {expected})");
            wrong += usize::from(got != expected);
        }
        assert_eq!(wrong, 0);
    }

    use super::*;

    #[test]
    fn preview_shortens_long_text() {
        assert_eq!(preview("kurz"), "kurz");
        assert!(preview(&"ä".repeat(60)).ends_with('…'));
    }

    #[test]
    fn coverage_is_antialiased() {
        assert_eq!(coverage(-2.0), 255);
        assert_eq!(coverage(2.0), 0);
        assert_eq!(coverage(0.0), 127);
    }
}
