//! The worker's part of reading aloud and talking things over (`voice.rs` has the pieces).
//!
//! - **Vorlesen** (`read_key`, menu): the selected text is spoken with the system voice; the key
//!   again or Esc stops it.
//! - **Darüber diskutieren** (`discuss_key`, menu): selected text becomes the first thing the agent
//!   hears (nothing selected: you start). Then it goes round: you speak, a pause ends your turn,
//!   the agent (Claude Code or an agent plugin, as the profile says) answers and the answer is spoken, then the microphone opens again.
//!   The key during the answer cuts in at once; Esc ends the conversation.
//! - **A web page's Talk button** (`local.rs`, `POST /talk?session=…`; Fleet has one): the same conversation with a
//!   session that already exists; it starts with the microphone and the session stays open after.

use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::app::{Cues, Live, Show, State, TICK, transcribe};
use crate::asr::Asr;
use crate::audio::Recorder;
use crate::config::{Config, VoiceAction};
use crate::feedback::Cue;
use crate::overlay;
use crate::platform::{self, TapEvent};
use crate::pocket::Pocket;
use crate::replace::Corrector;
use crate::text::pick;
use crate::voice::{self, Agent, Endpoint, Heard, Speaker};

#[derive(Debug)]
enum Phase {
    Idle,
    /// The selected text is being read.
    Reading,
    /// The microphone is open for the user's turn.
    Listening(Endpoint),
    /// The agent is working on an answer.
    Thinking,
    /// The answer is being spoken.
    Speaking,
}

struct Talk {
    agent: Agent,
    session: Option<u64>,
    /// Started here (closed when the conversation ends), not a session a web page handed over.
    owned: bool,
    /// A handed-over session got `voice::REMINDER` with the first thing said (it never had the brief).
    reminded: bool,
}

/// The worker pieces a step needs, borrowed for one event.
pub struct Rig<'a> {
    pub asr: &'a mut Asr,
    pub corrector: &'a Corrector,
    pub recorder: &'a mut Option<Recorder>,
    pub live: &'a mut Live,
    pub cues: &'a Cues,
    pub model: String,
    pub show: &'a dyn Fn(Show),
    pub state: &'a dyn Fn(State, &str),
    pub tx: &'a Sender<TapEvent>,
}

pub struct Voices {
    speaker: Speaker,
    pocket: Arc<Pocket>,
    phase: Phase,
    talk: Option<Talk>,
    /// Bumped whenever a conversation or reading ends or starts anew: late answers and "done
    /// speaking" messages of an older one are dropped.
    epoch: u64,
    config: Config,
    actions: Vec<VoiceAction>,
    /// How many tap triggers are dictation triggers (the voice ones come after them).
    dictation: usize,
}

fn note(rig: &Rig, badge: &str, text: &str, seconds: u64) {
    (rig.show)(Show::Note {
        badge: badge.into(),
        text: text.into(),
        seconds,
    });
}

impl Voices {
    pub fn new(config: &Config) -> Self {
        let mut voices = Voices {
            speaker: Speaker::default(),
            pocket: Pocket::new(),
            phase: Phase::Idle,
            talk: None,
            epoch: 0,
            config: config.clone(),
            actions: Vec::new(),
            dictation: 0,
        };
        voices.configure(config);
        voices
    }

    fn configure(&mut self, config: &Config) {
        self.config = config.clone();
        self.actions = config
            .voice_triggers()
            .into_iter()
            .map(|(a, _)| a)
            .collect();
        self.dictation = config.triggers.len();
    }

    pub fn busy(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Takes the events that are about voices; gives back the others.
    pub fn handle(&mut self, event: TapEvent, rig: &mut Rig) -> Option<TapEvent> {
        match event {
            TapEvent::Press(i, _) | TapEvent::Release(i, _) | TapEvent::Interrupt(i, _)
                if i >= self.dictation =>
            {
                // Only the press of a voice shortcut counts.
                if let TapEvent::Press(..) = event
                    && let Some(action) = self.actions.get(i - self.dictation)
                {
                    match action {
                        VoiceAction::Read => self.read(rig),
                        VoiceAction::Discuss => self.discuss(rig),
                    }
                }
                None
            }
            // Dictating while a conversation runs would fight over the microphone.
            TapEvent::Press(..)
            | TapEvent::Release(..)
            | TapEvent::Interrupt(..)
            | TapEvent::Toggle
                if self.busy() =>
            {
                None
            }
            TapEvent::Read => {
                self.read(rig);
                None
            }
            TapEvent::Discuss => {
                self.discuss(rig);
                None
            }
            TapEvent::DiscussSession(id) => {
                self.discuss_session(rig, id);
                None
            }
            TapEvent::EndTalk => {
                if self.busy() {
                    self.end(rig, None);
                }
                None
            }
            TapEvent::Cancel if self.busy() => {
                self.end(rig, None);
                None
            }
            TapEvent::Spoke(epoch) => {
                if epoch == self.epoch {
                    match self.phase {
                        Phase::Reading => self.end(rig, None),
                        Phase::Speaking => self.listen(rig),
                        _ => {}
                    }
                }
                None
            }
            TapEvent::Answer(epoch, answer) => {
                if epoch == self.epoch && matches!(self.phase, Phase::Thinking) {
                    self.answered(rig, answer);
                }
                None
            }
            TapEvent::Configure(config) => {
                self.configure(&config);
                Some(TapEvent::Configure(config))
            }
            other => Some(other),
        }
    }

    /// Every tick while the microphone is open: has the user finished?
    pub fn tick(&mut self, rig: &mut Rig) {
        let Some(r) = rig.recorder.as_ref() else {
            return;
        };
        let level = r.recent_db(TICK.as_millis() as u32);
        let heard = match &mut self.phase {
            Phase::Listening(endpoint) => endpoint.feed(level),
            _ => return,
        };
        match heard {
            Heard::Going => {}
            Heard::Done => self.finish_listening(rig),
            Heard::Nothing => self.end(
                rig,
                Some(pick(
                    "Nichts gehört, Gespräch beendet",
                    "Nothing heard, conversation ended",
                )),
            ),
        }
    }

    /// Tells the tap (Esc) and the local server's `/status` what runs.
    fn set_active(&self) {
        platform::VOICE.store(self.busy(), std::sync::atomic::Ordering::Relaxed);
        let phase = match self.phase {
            Phase::Idle => None,
            Phase::Reading => Some("reading"),
            Phase::Listening(_) => Some("listening"),
            Phase::Thinking => Some("thinking"),
            Phase::Speaking => Some("speaking"),
        };
        let session = self.talk.as_ref().and_then(|t| t.session);
        *crate::local::TALK.lock().unwrap_or_else(|e| e.into_inner()) = phase.map(|p| (session, p));
    }

    /// Stops everything: voice, microphone, the conversation (its session is closed).
    fn end(&mut self, rig: &mut Rig, why: Option<&str>) {
        self.epoch += 1;
        self.stop_speaking();
        if rig.recorder.take().is_some() {
            rig.cues.chime(Cue::Cancel);
        }
        if let Some(talk) = self.talk.take()
            && talk.owned
            && let Some(id) = talk.session
        {
            let agent = talk.agent;
            std::thread::spawn(move || agent.close(id));
        }
        self.phase = Phase::Idle;
        self.set_active();
        match why {
            Some(text) => note(rig, "–", text, 3),
            None => (rig.show)(Show::Hide),
        }
        (rig.state)(State::Ready, crate::text::ready());
    }

    fn stop_speaking(&self) {
        self.speaker.stop();
        self.pocket.stop();
    }

    fn speak(&mut self, rig: &Rig, text: &str) -> bool {
        let spoken = voice::clean_for_speech(text);
        let voice = voice::pick_voice(&spoken, &self.config.voice_de, &self.config.voice_en);
        let (tx, epoch) = (rig.tx.clone(), self.epoch);
        self.stop_speaking();
        // German goes to PocketTTS once its pack is there; until then (and for English text or
        // when it fails) the system voice speaks.
        if self.config.voice_engine == "pocket" && voice == self.config.voice_de {
            if Pocket::cached() {
                let done = tx.clone();
                match self.pocket.speak(
                    &spoken,
                    &self.config.voice_pocket,
                    self.config.speed(),
                    move || {
                        let _ = done.send(TapEvent::Spoke(epoch));
                    },
                ) {
                    Ok(()) => return true,
                    Err(e) => eprintln!("voice: {e:#}, system voice instead"),
                }
            } else {
                self.pocket.fetch();
                note(
                    rig,
                    "♪",
                    pick(
                        "Die bessere Stimme (450 MB) wird geladen, bis dahin spricht die Systemstimme",
                        "Loading the better voice (450 MB), the system voice speaks until then",
                    ),
                    4,
                );
            }
        }
        match self.speaker.speak(
            &spoken,
            voice,
            (self.config.voice_rate as f32 * self.config.speed()).round() as u32,
            move || {
                let _ = tx.send(TapEvent::Spoke(epoch));
            },
        ) {
            Ok(()) => true,
            Err(e) => {
                note(rig, "!", &format!("{e:#}"), 4);
                false
            }
        }
    }

    fn read(&mut self, rig: &mut Rig) {
        if self.busy() {
            // The key again stops (reading, answer, conversation alike).
            return self.end(rig, None);
        }
        // The shortcut's own modifiers may still be down: let them go before ⌘C.
        std::thread::sleep(Duration::from_millis(250));
        let Some(text) = platform::selected_text() else {
            return note(rig, "–", pick("Nichts markiert", "Nothing selected"), 2);
        };
        self.epoch += 1;
        if self.speak(rig, &text) {
            self.phase = Phase::Reading;
            self.set_active();
            (rig.show)(Show::Busy {
                text: pick(
                    "Liest vor… (Taste oder Esc stoppt)",
                    "Reading aloud… (the key or Esc stops)",
                )
                .into(),
                struck: Vec::new(),
            });
            (rig.state)(State::Transcribing, pick("Liest vor…", "Reading aloud…"));
        }
    }

    fn discuss(&mut self, rig: &mut Rig) {
        match self.phase {
            Phase::Listening(_) => self.finish_listening(rig),
            Phase::Speaking => {
                // Cutting in: the answer stops, it is the user's turn.
                self.epoch += 1;
                self.stop_speaking();
                self.listen(rig);
            }
            Phase::Reading | Phase::Thinking => self.end(rig, None),
            Phase::Idle => {
                std::thread::sleep(Duration::from_millis(250));
                let agent = match Agent::for_conversation(&self.config) {
                    Ok(agent) => agent,
                    Err(e) => return note(rig, "!", &format!("{e:#}"), 5),
                };
                self.epoch += 1;
                self.talk = Some(Talk {
                    agent,
                    session: None,
                    owned: true,
                    reminded: true,
                });
                match platform::selected_text() {
                    Some(text) => {
                        let first = pick(
                            format!(
                                "Ich möchte mit dir über diesen Text sprechen, den ich gerade markiert habe:\n\n\
                                 {text}\n\nSag mir kurz deinen ersten Gedanken dazu und frag mich, was mich daran beschäftigt."
                            ),
                            format!(
                                "I'd like to talk about this text I just selected:\n\n{text}\n\n\
                                 Tell me your first thought on it briefly and ask me what is on my mind about it."
                            ),
                        );
                        self.think(rig, first);
                    }
                    None => self.listen(rig),
                }
            }
        }
    }

    /// A web page's Talk button: a conversation with its session `id` (the profile's agent
    /// plugin's), starting with the microphone. One
    /// that runs already (with another session, or a reading) ends first.
    fn discuss_session(&mut self, rig: &mut Rig, id: u64) {
        if self.busy() {
            if matches!(self.phase, Phase::Listening(_))
                && self.talk.as_ref().is_some_and(|t| t.session == Some(id))
            {
                return; // already listening to this one
            }
            self.end(rig, None);
        }
        let agent = match Agent::handed_over(&self.config) {
            Ok(agent) => agent,
            Err(e) => return note(rig, "!", &format!("{e:#}"), 5),
        };
        self.epoch += 1;
        self.talk = Some(Talk {
            agent,
            session: Some(id),
            owned: false,
            reminded: false,
        });
        self.listen(rig);
    }

    /// Opens the microphone for the user's turn.
    fn listen(&mut self, rig: &mut Rig) {
        match Recorder::start() {
            Ok(r) => {
                *rig.recorder = Some(r);
                *rig.live = Live::default();
                rig.cues.chime(Cue::Start);
                self.phase = Phase::Listening(Endpoint::new(TICK));
                self.set_active();
                (rig.show)(Show::Live {
                    levels: Vec::new(),
                    text: overlay::WAITING.into(),
                    struck: Vec::new(),
                });
                (rig.state)(
                    State::Recording,
                    pick("Gespräch: ich höre zu…", "Conversation: listening…"),
                );
            }
            Err(e) => {
                crate::stats::problem(&format!("conversation: recording could not start: {e:#}"));
                self.end(
                    rig,
                    Some(&format!("{}: {e:#}", pick("Mikrofon", "Microphone"))),
                );
            }
        }
    }

    fn finish_listening(&mut self, rig: &mut Rig) {
        let Some(r) = rig.recorder.take() else {
            return;
        };
        rig.cues.chime(Cue::Stop);
        self.phase = Phase::Thinking;
        self.set_active();
        (rig.show)(Show::Busy {
            text: rig.live.text.clone(),
            struck: Vec::new(),
        });
        (rig.state)(State::Transcribing, crate::text::transcribing());
        let clip = r.stop();
        match transcribe(rig.asr, rig.corrector, clip, &rig.model) {
            Ok(Some(text)) => self.think(rig, text),
            // Nothing recognised: the microphone opens again.
            Ok(None) => self.listen(rig),
            Err(problem) => self.end(rig, Some(&problem)),
        }
    }

    /// Hands `said` to the agent in the background; the answer comes back as `TapEvent::Answer`.
    fn think(&mut self, rig: &Rig, said: String) {
        let Some(talk) = &mut self.talk else { return };
        let sent = if talk.reminded {
            said.clone()
        } else {
            format!("{said}\n\n{}", voice::REMINDER)
        };
        talk.reminded = true;
        let (agent, session) = (talk.agent.clone(), talk.session);
        self.phase = Phase::Thinking;
        self.set_active();
        (rig.show)(Show::Busy {
            text: format!(
                "{said}\n\n{}",
                pick("Agent denkt nach…", "Agent is thinking…")
            ),
            struck: Vec::new(),
        });
        (rig.state)(
            State::Transcribing,
            pick("Agent denkt nach…", "Agent is thinking…"),
        );
        let (tx, epoch) = (rig.tx.clone(), self.epoch);
        std::thread::spawn(move || {
            let answer = match session {
                None => agent
                    .start(&said)
                    .and_then(|id| agent.ask(id, None).map(|a| (id, a))),
                Some(id) => agent.ask(id, Some(&sent)).map(|a| (id, a)),
            };
            let _ = tx.send(TapEvent::Answer(
                epoch,
                answer.map_err(|e| format!("{e:#}")),
            ));
        });
    }

    fn answered(&mut self, rig: &mut Rig, answer: Result<(u64, String), String>) {
        match answer {
            Ok((id, text)) => {
                if let Some(talk) = &mut self.talk {
                    talk.session = Some(id);
                }
                if self.speak(rig, &text) {
                    self.phase = Phase::Speaking;
                    self.set_active();
                    (rig.show)(Show::Busy {
                        text,
                        struck: Vec::new(),
                    });
                    (rig.state)(
                        State::Transcribing,
                        pick("Agent spricht…", "Agent is speaking…"),
                    );
                } else {
                    self.end(rig, None);
                }
            }
            Err(e) => {
                crate::stats::problem(&format!("conversation: {e}"));
                self.end(rig, Some(&e));
            }
        }
    }
}
