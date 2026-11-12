// A pretend app for `npm run dev` in a browser: sample history, dictionary and settings, and rough
// stand-ins for what the Rust side computes (changed words, dictionary test, trigger capture).
// Query flags: ?perm=ok hides the missing-permission banner, ?empty=1 starts without history,
// ?mock=meeting starts with a Souffleur meeting under way (lines and answers, for screenshots),
// ?system=silent|off|problem shows that state of the Mac's sound, ?lang=de|en the page's language
// (else as the setting says, "auto" = the browser's), ?report=none a profile without a report
// target (no report switch), ?platform=windows|linux the app on that OS (key names, its default
// shortcuts, no Neural Engine or Apple model; else macOS), ?mock=notray (with ?platform=linux) a
// desktop without a tray: the banner that the window stands in for the menu.

import type { Candidate, Event, Request, State } from './protocol.ts';
import { createMeeting } from './mock-meeting.ts';
import { translate, type Key, type Lang } from './strings.ts';

const DEFAULT_WORDS = [
	'SOLISTIQ', 'Swarm', 'Dictate', 'Parakeet', 'Claude', 'Codex', 'Opus', 'Sonnet', 'GitHub',
	'Pull Request', 'Commit', 'Deploy', 'Backlog', 'Nightly Build', 'Monorepo', 'Terraform', 'ISMS',
	'CRM', 'Notion', 'Slack', 'Svelte', 'SvelteKit', 'Tauri', 'Rust', 'Python', 'Forecast'
];

function sampleHistory(now: number) {
	const day = 86400;
	const today = new Date(now * 1000);
	today.setHours(0, 0, 0, 0);
	const midnight = today.getTime() / 1000;
	const texts: [number | null, string][] = [
		[null, 'Was denkst du darüber?'],
		[null, 'Kannst du mir die Zahlen von letzter Woche noch einmal schicken?'],
		[midnight - 3 * day + 9 * 3600 + 12 * 60, 'Lass uns das Meeting auf Donnerstag um zehn verschieben.'],
		[midnight - 2 * day + 16 * 3600 + 40 * 60, 'Bitte schau dir den Forecast für die Filiale Nord an, die Werte für November wirken zu hoch.'],
		[midnight - day + 8 * 3600 + 55 * 60, 'Ist das Ticket unabhängig vom Backend, was wir gerade in Rust portieren? Also ist es nur Frontend? Falls ja, dann setz es einfach direkt um und erstelle Matchdruckers und Matschin.'],
		[midnight - day + 17 * 3600 + 2 * 60, 'Wir deployen morgen bei SOLISTIQ, kannst du den Merge Request vorher noch reviewen?'],
		[midnight + 7 * 3600 + 55 * 60, 'Kannst du den Pull Request reviewen? Ich habe die Tests für den Verlauf ergänzt.'],
		[midnight + 9 * 3600 + 31 * 60, 'Okay, dann machen wir das so. Die Session soll im Monorepo sichtbar sein, solange sie läuft oder wartet.'],
		[midnight + 10 * 3600 + 48 * 60, 'Sollten wir Merge Request und mergen noch in unser Wörterbuch aufnehmen? Wenn ja, wie?']
	];
	return texts.map(([at, text], id) => ({ id, at, text }));
}

function initialState(): State {
	const params = new URLSearchParams(location.search);
	const now = Date.now() / 1000;
	const platform = (['windows', 'linux'] as const).find((p) => p === params.get('platform')) ?? 'macos';
	const mac = platform === 'macos';
	const state: State = {
		status: 'ready',
		status_text: 'Bereit',
		// Windows asks for nothing; the permission app.rs reports there is always granted.
		accessibility: params.get('perm') === 'ok' || platform === 'windows' ? 'granted' : 'missing',
		// Only the Mac says whether the microphone may be used.
		microphone: mac ? 'granted' : 'unknown',
		microphone_name: mac ? 'MacBook Pro-Mikrofon' : 'Eingebautes Mikrofon',
		microphones: mac ? ['MacBook Pro-Mikrofon', 'Jabra Evolve2 65', 'iPhone-Mikrofon'] : ['Eingebautes Mikrofon', 'Jabra Evolve2 65'],
		config: {
			// The defaults of core/src/config.rs: Fn on the Mac, right Alt elsewhere (no Fn key there).
			triggers: [
				{ key: mac ? 'fn' : 'right-alt', mode: 'hybrid' },
				{ key: 'shift+alt+cmd+5', mode: 'hybrid' },
				{ key: 'shift+alt+cmd+8', mode: 'hybrid', format: true, correct: true }
			],
			model: mac ? 'parakeet-ultra-coreml' : 'parakeet-ultra',
			language: 'auto',
			ui_language: (['de', 'en'] as const).find((l) => l === params.get('lang')) ?? 'auto',
			microphone: '',
			sounds: 'leise',
			pause_media: true,
			mute_while_recording: false,
			learn_from_corrections: true,
			voice_commands: true,
			correct_model: 'gemma-4-e4b',
			boost_vocabulary: true,
			share_reports: true,
			show_bar: false,
			start_at_login: true,
			meeting_microphone: '',
			meeting_system_audio: true,
			meeting_model: 'sonnet',
			meeting_instructions: ''
		},
		words: [
			...DEFAULT_WORDS.map((word) => ({ word, source: 'default' as const })),
			{ word: 'Merge Request', source: 'learned' },
			{ word: 'Tourenplanung', source: 'manual' }
		],
		replacements: [
			{ heard: 'Solistik', meant: 'SOLISTIQ' },
			{ heard: 'deckläuen', meant: 'deployen' },
			{ heard: 'Matchdruckers', meant: 'Merge Requests' }
		],
		models: [
			{ name: 'parakeet-ultra-coreml', label: 'Parakeet Ultra · Neural Engine', size: 'empfohlen, sparsam, 0,6 GB', downloaded: true, progress: null },
			{ name: 'parakeet-ultra', label: 'Parakeet Ultra · CPU', size: '2,5 GB', downloaded: true, progress: null },
			{ name: 'parakeet-ultra-int8', label: 'Parakeet Ultra int8 · CPU', size: '0,7 GB', downloaded: false, progress: null },
			{ name: 'parakeet-tdt-0.6b-v3', label: 'NVIDIA Parakeet V3', size: '2,5 GB', downloaded: false, progress: null },
			{ name: 'nemotron-3.5-streaming', label: 'Nemotron 3.5 Streaming', size: '40 Sprachen, 2,6 GB', downloaded: false, progress: null },
			{ name: 'nemotron-unified', label: 'Nemotron Unified', size: '25 Sprachen, 2,6 GB', downloaded: false, progress: null },
			{ name: 'cohere-transcribe-int8', label: 'Cohere Transcribe', size: '14 Sprachen, 3,1 GB', downloaded: false, progress: null },
			{ name: 'parakeet-ctc-0.6b', label: 'Parakeet CTC', size: 'nur Englisch, 2,5 GB', downloaded: false, progress: null }
		],
		correct_models: [
			{ name: 'gemma-4-e4b', label: 'Gemma 4 E4B', size: 'lokal auf der GPU, 5 GB', downloaded: params.get('gemma') !== 'no', progress: null },
			{ name: 'apple', label: 'Apple Intelligence', size: 'im System, schwächer bei Selbstkorrekturen', downloaded: true, progress: null }
		],
		history: params.get('empty') ? [] : sampleHistory(now),
		lang: 'de',
		report_target: params.get('report') === 'none' ? null : 'example-org/dictate#7',
		profile: params.get('report') === 'none' ? '' : 'Example',
		platform,
		// Only the Linux app looks for a tray (linux::Tray); macOS and Windows always have one.
		no_tray: platform === 'linux' && params.get('mock') === 'notray'
	};
	// The Neural Engine models and Apple's are the Mac's only (asr.rs, app.rs).
	if (!mac) {
		state.models = state.models.filter((m) => !m.name.endsWith('-coreml'));
		state.correct_models = state.correct_models.filter((m) => m.name !== 'apple');
	}
	return state;
}

/** Words that differ between two versions of a text, aligned by a word-level LCS. */
export function changedWords(original: string, edited: string, words: string[]): Candidate[] {
	const a = original.split(/\s+/).filter(Boolean);
	const b = edited.split(/\s+/).filter(Boolean);
	const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
	for (let i = a.length - 1; i >= 0; i--)
		for (let j = b.length - 1; j >= 0; j--)
			lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
	const found: Candidate[] = [];
	const strip = (w: string) => w.replace(/[.,!?;:]+$/, '');
	let heard: string[] = [];
	let meant: string[] = [];
	const flush = () => {
		const h = strip(heard.join(' '));
		const m = strip(meant.join(' '));
		if (h && m && h.toLowerCase() !== m.toLowerCase() && heard.length <= 3 && meant.length <= 3) {
			const known = words.some((w) => w.toLowerCase() === m.toLowerCase());
			const kind = known || /^[A-ZÄÖÜ]/.test(m) ? 'dictionary' : 'replacement';
			found.push({ heard: h, meant: m, kind });
		}
		heard = [];
		meant = [];
	};
	let i = 0;
	let j = 0;
	while (i < a.length || j < b.length) {
		if (i < a.length && j < b.length && a[i] === b[j]) {
			flush();
			i++;
			j++;
		} else if (j >= b.length || (i < a.length && lcs[i + 1][j] >= lcs[i][j + 1])) {
			heard.push(a[i++]);
		} else {
			meant.push(b[j++]);
		}
	}
	flush();
	return found;
}

export function createMock(receive: (event: Event) => void): (request: Request) => void {
	const state = initialState();
	// What the app does with config.ui_language: "auto" follows the system (here the browser).
	const resolve = (): Lang => {
		const setting = state.config.ui_language;
		if (setting === 'de' || setting === 'en') return setting;
		return navigator.language.toLowerCase().startsWith('de') ? 'de' : 'en';
	};
	// Like the app, the mock sends its status and notes in the resolved language.
	const say = (key: Key, params?: Record<string, string | number>) => translate(resolve(), key, params);
	const emit = () =>
		receive({
			type: 'state',
			state: structuredClone({ ...state, lang: resolve(), status_text: say('mock.ready') })
		});
	const note = (key: Key, params?: Record<string, string | number>) => receive({ type: 'note', text: say(key, params) });
	const later = (ms: number, fn: () => void) => setTimeout(fn, ms);
	const meeting = createMeeting(receive, () => state.config.meeting_system_audio);

	return (request) => {
		switch (request.type) {
			case 'load':
				return later(30, () => {
					emit();
					meeting.load();
				});
			case 'start_meeting':
				return meeting.start();
			case 'stop_meeting':
				return meeting.stop();
			case 'resume_meeting':
				return meeting.resume();
			case 'ask_meeting':
				return meeting.ask(request.text);
			case 'open_panel': {
				const theme = new URLSearchParams(location.search).get('theme');
				// The panel's own mock starts from the URL: hand it the language set here.
				const lang = state.config.ui_language;
				const query = `${theme ? `&theme=${theme}` : ''}${lang !== 'auto' ? `&lang=${lang}` : ''}`;
				open(`${location.pathname}?view=panel${query}`, 'souffleur', 'width=380,height=760');
				return note('mock.panel_opened');
			}
			case 'open_page':
				return note('mock.settings');
			case 'open_url':
				open(request.url, '_blank', 'noopener');
				return note('mock.browser');
			case 'open_meetings':
				return note('mock.meetings');
			case 'copy':
				void navigator.clipboard?.writeText(request.text).catch(() => {});
				return note('mock.copied');
			case 'find': {
				const words = state.words.map((w) => w.word);
				const candidates = changedWords(request.original, request.edited, words);
				return later(20, () => receive({ type: 'candidates', original: request.original, edited: request.edited, candidates }));
			}
			case 'save_entry': {
				const entry = state.history.find((e) => e.id === request.id);
				if (entry) entry.text = request.text;
				for (const c of request.learn) {
					if (c.kind === 'dictionary' && !state.words.some((w) => w.word === c.meant))
						state.words.push({ word: c.meant, source: 'learned' });
					if (c.kind === 'replacement') state.replacements.push({ heard: c.heard, meant: c.meant });
				}
				emit();
				return note(request.learn.length ? 'mock.saved_learned' : 'mock.saved', { n: request.learn.length });
			}
			case 'config':
				state.config = request.config;
				return emit();
			case 'words': {
				const before = new Map(state.words.map((w) => [w.word, w.source]));
				state.words = request.words.map((word) => ({ word, source: before.get(word) ?? 'manual' }));
				return emit();
			}
			case 'replacements':
				state.replacements = request.replacements;
				return emit();
			case 'try': {
				let result = request.text;
				for (const r of state.replacements)
					result = result.replace(new RegExp(`\\b${r.heard}\\b`, 'gi'), r.meant);
				for (const w of state.words)
					result = result.replace(new RegExp(`\\b${w.word}\\b`, 'gi'), w.word);
				return receive({ type: 'tried', text: request.text, result });
			}
			case 'record_trigger':
				return later(1400, () => receive({ type: 'captured', row: request.row, key: 'f18', message: '' }));
			case 'download_model': {
				const text = state.correct_models.find((m) => m.name === request.name);
				if (text) {
					state.config.correct_model = text.name;
					text.downloaded = true;
					emit();
					return;
				}
				const model = state.models.find((m) => m.name === request.name);
				if (!model) return;
				model.progress = 0;
				const tick = setInterval(() => {
					model.progress = Math.min(1, (model.progress ?? 0) + 0.08);
					if (model.progress >= 1) {
						clearInterval(tick);
						model.progress = null;
						model.downloaded = true;
						state.config.model = model.name;
					}
					emit();
				}, 250);
				return;
			}
			case 'mic_test': {
				let n = 0;
				const tick = setInterval(() => {
					n++;
					const done = n >= 20;
					receive({ type: 'mic_level', db: -45 + Math.random() * 25, done });
					if (done) clearInterval(tick);
				}, 100);
				return;
			}
			case 'open_privacy':
				state.accessibility = 'granted';
				emit();
				return note('mock.privacy');
			case 'open_folder':
				return note('mock.folder');
		}
	};
}
