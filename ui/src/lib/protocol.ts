// The messages between the page and the app (src/protocol.rs mirrors these).
// Page → app: window.ipc.postMessage(JSON.stringify(Request)). App → page: dictate.receive(Event).
// protocol.fixtures.json holds one sample of each; Rust's protocol tests and protocol.test.ts check them.

import type { Page } from './pages.ts';

export type Status = 'loading' | 'ready' | 'recording' | 'transcribing' | 'problem';
export type Permission = 'granted' | 'missing' | 'unknown';
export type Mode = 'hold' | 'toggle' | 'hybrid';
export type Sounds = 'aus' | 'leise' | 'mittel';
export type Kind = 'dictionary' | 'replacement';

export interface Entry {
	/** Position in the history, oldest = 0. */
	id: number;
	/** Unix seconds; null for dictations taken over from the log. */
	at: number | null;
	text: string;
}

export interface Trigger {
	key: string;
	mode: Mode;
	/** Lay the text out with Apple's language model before pasting (paragraphs, lists). */
	format?: boolean;
	/** Drop self-corrections ("um drei, äh nein, um fünf") and filler words before pasting. */
	correct?: boolean;
}

export interface Config {
	triggers: Trigger[];
	model: string;
	/** Spoken language for models that take one: "auto", "de", "en". */
	language: string;
	/** Language of the window, menus and Souffleur: "auto" (the system's), "de", "en". */
	ui_language: 'auto' | 'de' | 'en';
	/** Chosen microphone name, '' = the Mac's default. */
	microphone: string;
	sounds: Sounds;
	pause_media: boolean;
	mute_while_recording: boolean;
	learn_from_corrections: boolean;
	/** Spoken layout commands: "neue Zeile", "neuer Absatz", "Spiegelstrich". */
	voice_commands: boolean;
	/** The model for "Korrigieren": a local text model ("gemma-4-e4b") or "apple". */
	correct_model: string;
	boost_vocabulary: boolean;
	share_reports: boolean;
	start_at_login: boolean;
	/** The bar at the bottom of the screen (Diktieren, Souffleur, Einstellungen). */
	show_bar: boolean;
	/** Souffleur: its own microphone ('' = the Mac's default), separate from the dictation one. */
	meeting_microphone: string;
	/** Also listen to what the Mac plays (the others in a call). */
	meeting_system_audio: boolean;
	/** Claude model of the Souffleur's agent: "sonnet", "haiku", "opus". */
	meeting_model: string;
	/** Extra instructions appended to the agent's prompt. */
	meeting_instructions: string;
}

export interface MeetingLine {
	id: number;
	/** "me" = the microphone, "others" = what the Mac plays. */
	who: 'me' | 'others';
	/** Unix seconds (with fraction) when the speech began. */
	at: number;
	text: string;
	/** Still being spoken: the text may change under the same id. */
	live: boolean;
}

export interface MeetingCard {
	id: number;
	at: number;
	title: string;
	/** Markdown: paragraphs, "- " lists, **bold**, [links](https://…), `code`. */
	body: string;
	/** The question typed in the panel this answers; null when the agent spoke up by itself. */
	question: string | null;
	/** Still being written. */
	pending: boolean;
}

export type AgentPhase = 'off' | 'starting' | 'listening' | 'thinking' | 'problem';

export interface MeetingStatus {
	agent: AgentPhase;
	/** What the agent does right now ("Sucht im CRM"), '' when nothing. */
	activity: string;
	/** dBFS of the last 300 ms; null when that source is off. */
	mic_db: number | null;
	system_db: number | null;
	/** on | off | silent (no permission?) | problem */
	system_audio: 'on' | 'off' | 'silent' | 'problem';
	/** Shown in the panel when not empty. */
	problem: string;
}

export interface Meeting {
	running: boolean;
	/** Unix seconds; null before the first meeting. */
	started: number | null;
	lines: MeetingLine[];
	cards: MeetingCard[];
	status: MeetingStatus;
	/** "Das Wichtigste": the agent's key facts of the meeting (short Markdown lines), most important first. */
	highlights: string[];
}

export interface Word {
	word: string;
	/** "default", "manual" or "learned". */
	source: 'default' | 'manual' | 'learned';
}

export interface Replacement {
	heard: string;
	meant: string;
}

export interface Model {
	name: string;
	label: string;
	size: string;
	downloaded: boolean;
	/** 0–1 while downloading. */
	progress: number | null;
}

export interface Candidate {
	heard: string;
	meant: string;
	kind: Kind;
}

export type Platform = 'macos' | 'windows' | 'linux';

export interface State {
	status: Status;
	status_text: string;
	accessibility: Permission;
	microphone: Permission;
	microphone_name: string;
	/** Microphones connected now (names), the default first. */
	microphones: string[];
	config: Config;
	words: Word[];
	replacements: Replacement[];
	models: Model[];
	/** What "Korrigieren" can use: local text models and "apple". */
	correct_models: Model[];
	/** Oldest first. */
	history: Entry[];
	/** The language to show: config.ui_language resolved by the app. */
	lang: 'de' | 'en';
	/** Where the anonymous reports go ("owner/repo#7"); null = the profile sends none, no switch. */
	report_target: string | null;
	/** The profile's name beside the wordmark ("SOLISTIQ"); "" = none. */
	profile: string;
	/** The OS the app runs on: keys and system settings are named after it. */
	platform: Platform;
	/** Linux without a tray: the window stands in for the menu and closing it quits (banner). */
	no_tray: boolean;
}

export type Request =
	| { type: 'load' }
	| { type: 'copy'; text: string }
	| { type: 'find'; original: string; edited: string }
	| { type: 'save_entry'; id: number; text: string; learn: Candidate[] }
	| { type: 'config'; config: Config }
	| { type: 'words'; words: string[] }
	| { type: 'replacements'; replacements: Replacement[] }
	| { type: 'try'; text: string }
	| { type: 'record_trigger'; row: number }
	| { type: 'download_model'; name: string }
	| { type: 'mic_test' }
	| { type: 'open_privacy'; what: 'accessibility' | 'microphone' }
	| { type: 'open_folder' }
	| { type: 'drag' }
	| { type: 'start_meeting' }
	| { type: 'stop_meeting' }
	| { type: 'ask_meeting'; text: string }
	| { type: 'open_panel' }
	| { type: 'open_page'; page: Page }
	| { type: 'open_url'; url: string }
	| { type: 'open_meetings' };

export type Event =
	| { type: 'state'; state: State }
	| { type: 'candidates'; original: string; edited: string; candidates: Candidate[] }
	| { type: 'tried'; text: string; result: string }
	| { type: 'captured'; row: number; key: string | null; message: string }
	| { type: 'mic_level'; db: number | null; done: boolean }
	| { type: 'note'; text: string }
	| { type: 'show'; page: Page; entry: number | null }
	| { type: 'meeting'; meeting: Meeting }
	| { type: 'meeting_line'; line: MeetingLine }
	| { type: 'meeting_card'; card: MeetingCard }
	| { type: 'meeting_status'; status: MeetingStatus }
	| { type: 'meeting_highlights'; highlights: string[] };
