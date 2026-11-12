// The mock app's Souffleur: a scripted call (lines from Ich and Andere, spoken word by word, then
// final), status ticks with moving levels, answers that are first pending and then written, and
// canned answers for typed questions. ?mock=meeting starts with a meeting under way.

import type { Event, Meeting, MeetingCard, MeetingLine, MeetingStatus } from './protocol.ts';

type Who = MeetingLine['who'];
// `highlights`: "Das Wichtigste" after this answer (the whole list, it replaces the last).
type Answer = { title: string; body: string; activity: string; highlights?: string[] };
type Step = { who: Who; text: string } | { answer: Answer };

const PILOT: Answer = {
	activity: 'Sucht in Notion',
	title: 'Forecast Filiale Nord: neue Daten seit 28.10.',
	body:
		'Läuft mit den neuen Abverkaufsdaten, letzter Lauf heute 06:10 ohne Fehler.\n' +
		'- Fehler (MAPE) Nord: **11,8 %**, vorher 14,2 %\n' +
		'- November noch ohne Aktionsdaten, deshalb eher zu hoch\n' +
		'- [Pilot-Log in Notion](https://www.notion.so/example/pilot-log-kessler)',
	highlights: ['Forecast Nord läuft mit neuen Daten, **MAPE 11,8 %**', 'November ohne Aktionsdaten: eher zu hoch']
};
const CONTACT: Answer = {
	activity: 'Sucht im CRM',
	title: 'Schnittstelle: Jonas Weber',
	body:
		'**Jonas Weber** (Backend) ist im CRM technischer Kontakt für Kessler, Sabine Kraus fachlich.\n' +
		'- Slack: `#pilot-kessler`, zuletzt gestern zum SFTP-Export\n' +
		'- [Kontakt im CRM](https://crm.example.com/contacts/42)',
	highlights: [
		'Forecast Nord läuft mit neuen Daten, **MAPE 11,8 %**',
		'Schnittstelle: **Jonas Weber**, fachlich Sabine Kraus',
		'November ohne Aktionsdaten: eher zu hoch'
	]
};
const ACCEPTANCE: Answer = {
	activity: 'Sucht in Gmail',
	title: 'Abnahme am 14.11., 10:00',
	body:
		'So in Sabines Mail vom 21.10., die Einladung steht noch aus.\n' +
		'- Dabei: Sabine Kraus, Jonas Weber, du\n' +
		'- Offen: Freigabe der Filialen Süd und West\n' +
		'- [Mail in Gmail](https://mail.google.com/mail/u/0/#search/abnahme+kessler)',
	highlights: [
		'Abnahme **14.11., 10:00**, Einladung fehlt noch',
		'Offen: Freigabe Filialen Süd und West',
		'Forecast Nord läuft mit neuen Daten, **MAPE 11,8 %**',
		'Schnittstelle: **Jonas Weber**, fachlich Sabine Kraus'
	]
};

const SCRIPT: Step[] = [
	{ who: 'others', text: 'Hallo zusammen, schön, dass es heute klappt.' },
	{ who: 'me', text: 'Hi, ja gerne. Wollen wir kurz auf den Stand vom Piloten schauen?' },
	{ who: 'others', text: 'Genau. Bei uns ist die Frage, ob die Forecasts für die Filiale Nord schon mit den neuen Daten laufen.' },
	{ answer: PILOT },
	{ who: 'me', text: 'Das müsste seit letzter Woche so sein, ich schau gleich nach.' },
	{ who: 'others', text: 'Super. Und wer ist bei euch eigentlich Ansprechpartner für die Schnittstelle?' },
	{ answer: CONTACT },
	{ who: 'me', text: 'Das läuft über Jonas, der hat auch den Export gebaut.' },
	{ who: 'others', text: 'Okay. Wann war nochmal der Termin für die Abnahme?' },
	{ answer: ACCEPTANCE },
	{ who: 'me', text: 'Der vierzehnte, glaube ich, um zehn.' },
	{ who: 'others', text: 'Passt, dann nehmen wir bis dahin noch Süd und West dazu.' }
];

const ASKED: { match: RegExp; answer: Answer }[] = [
	{
		match: /nordlicht/i,
		answer: {
			activity: 'Sucht im CRM',
			title: 'Nordlicht: letzter Call am 29.10.',
			body:
				'Mit Herrn Lenz, Thema Datenzugang für den Piloten.\n' +
				'- Nächster Schritt: NDA zurück an uns, liegt bei Nordlicht\n' +
				'- [Account im CRM](https://crm.example.com/accounts/7)'
		}
	}
];

function fallback(question: string): Answer {
	return {
		activity: 'Sucht in Slack',
		title: 'Kurz nachgesehen',
		body:
			`Zu „${question}“ steht in Slack und Notion nichts Eindeutiges.\n` +
			'- Am nächsten dran: Thread in `#pilot-kessler` von gestern\n' +
			'- https://example.slack.com/archives/C042/p1730'
	};
}

export function createMeeting(receive: (event: Event) => void, systemOn: () => boolean) {
	const params = new URLSearchParams(location.search);
	const forced = params.get('system') as MeetingStatus['system_audio'] | null;
	const seconds = () => Date.now() / 1000;
	const timers = new Set<ReturnType<typeof setTimeout>>();
	const later = (ms: number, fn: () => void) => {
		const t = setTimeout(() => {
			timers.delete(t);
			fn();
		}, ms);
		timers.add(t);
	};
	let ticker: ReturnType<typeof setInterval> | undefined;

	const idle = (): MeetingStatus => ({
		agent: 'off',
		activity: '',
		mic_db: null,
		system_db: null,
		system_audio: 'off',
		problem: '',
		ending: false
	});
	let meeting: Meeting = {
		running: false,
		started: null,
		lines: [],
		cards: [],
		status: idle(),
		highlights: [],
		resumable: false
	};
	let speaking: Who | null = null;
	let nextLine = 0;
	let nextCard = 0;
	let busy = 0;

	const snapshot = () => receive({ type: 'meeting', meeting: structuredClone(meeting) });
	const line = (l: MeetingLine) => {
		const i = meeting.lines.findIndex((x) => x.id === l.id);
		if (i >= 0) meeting.lines[i] = l;
		else meeting.lines.push(l);
		receive({ type: 'meeting_line', line: { ...l } });
	};
	const card = (c: MeetingCard) => {
		const i = meeting.cards.findIndex((x) => x.id === c.id);
		if (i >= 0) meeting.cards[i] = c;
		else meeting.cards.push(c);
		receive({ type: 'meeting_card', card: { ...c } });
	};
	const status = (change: Partial<MeetingStatus>) => {
		meeting.status = { ...meeting.status, ...change };
		receive({ type: 'meeting_status', status: { ...meeting.status } });
	};

	function systemAudio(): MeetingStatus['system_audio'] {
		return forced ?? (systemOn() ? 'on' : 'off');
	}

	// Levels: speech around -30…-18 dBFS, room noise around -58.
	function tick() {
		const noise = () => -60 + Math.random() * 5;
		const speech = () => -32 + Math.random() * 14;
		const sys = systemAudio();
		status({
			mic_db: speaking === 'me' ? speech() : noise(),
			system_db: sys === 'on' ? (speaking === 'others' ? speech() : noise()) : sys === 'silent' ? -96 : null,
			system_audio: sys
		});
	}

	function say(who: Who, text: string, done: () => void) {
		const id = nextLine++;
		const at = seconds();
		const words = text.split(' ');
		speaking = who;
		let n = 0;
		const step = () => {
			n = Math.min(words.length, n + 2);
			const final = n >= words.length;
			line({ id, who, at, text: words.slice(0, n).join(' '), live: !final });
			if (final) {
				speaking = null;
				later(900, done);
			} else later(230, step);
		};
		step();
	}

	function answer(a: Answer, question: string | null, wait: number) {
		const id = nextCard++;
		busy++;
		card({ id, at: seconds(), title: '', body: '', question, pending: true });
		status({ agent: 'thinking', activity: a.activity });
		later(wait, () => {
			card({ id, at: seconds(), title: a.title, body: a.body, question, pending: false });
			if (a.highlights) {
				meeting.highlights = a.highlights;
				receive({ type: 'meeting_highlights', highlights: [...a.highlights] });
			}
			if (--busy === 0) status({ agent: 'listening', activity: '' });
		});
	}

	function play(i: number) {
		const s = SCRIPT[i];
		if (!s || !meeting.running) return;
		if ('answer' in s) {
			answer(s.answer, null, 2600);
			play(i + 1);
		} else say(s.who, s.text, () => play(i + 1));
	}

	function begin() {
		clearInterval(ticker);
		ticker = setInterval(tick, 150);
	}

	// A meeting 14 minutes in, three answers given, the agent looking something up.
	function underway() {
		const start = seconds() - 14 * 60 - 12;
		const lines: [Who, number, string][] = [
			['others', 410, 'Hallo zusammen, schön, dass es heute klappt.'],
			['me', 416, 'Hi, ja gerne. Wollen wir kurz auf den Stand vom Piloten schauen?'],
			['others', 422, 'Genau. Bei uns ist die Frage, ob die Forecasts für die Filiale Nord schon mit den neuen Daten laufen.'],
			['me', 431, 'Das müsste seit letzter Woche so sein, ich schau gleich nach.'],
			['others', 540, 'Super. Und wer ist bei euch eigentlich Ansprechpartner für die Schnittstelle?'],
			['me', 546, 'Das läuft über Jonas, der hat auch den Export gebaut.'],
			['others', 790, 'Okay. Wann war nochmal der Termin für die Abnahme?'],
			['me', 797, 'Der vierzehnte, glaube ich, um zehn.'],
			['others', 840, 'Passt, dann nehmen wir bis dahin noch Süd und West dazu. Und bei den'],
		];
		meeting = {
			running: true,
			started: start,
			lines: lines.map(([who, t, text], id) => ({ id, who, at: start + t, text, live: id === lines.length - 1 })),
			cards: [
				{ id: 0, at: start + 438, question: null, pending: false, ...PILOT },
				{ id: 1, at: start + 552, question: 'Wer ist bei Kessler für die Schnittstelle zuständig?', pending: false, ...CONTACT },
				{ id: 2, at: start + 790, question: null, pending: false, ...ACCEPTANCE }
			],
			status: { ...idle(), agent: 'thinking', activity: 'Sucht in Notion', system_audio: systemAudio() },
			highlights: [...(ACCEPTANCE.highlights ?? [])],
			resumable: false
		};
		nextLine = meeting.lines.length;
		nextCard = meeting.cards.length;
		speaking = 'others';
		begin();
	}

	return {
		load() {
			if (params.get('mock') === 'meeting' && !meeting.running && meeting.started === null) underway();
			snapshot();
		},
		start() {
			if (meeting.running) return;
			for (const t of timers) clearTimeout(t);
			timers.clear();
			meeting = {
				running: true,
				started: seconds(),
				lines: [],
				cards: [],
				highlights: [],
				resumable: false,
				status: { ...idle(), agent: 'starting', system_audio: systemAudio() }
			};
			nextLine = 0;
			nextCard = 0;
			busy = 0;
			snapshot();
			begin();
			later(1200, () => status({ agent: 'listening' }));
			later(1500, () => play(0));
		},
		stop() {
			if (!meeting.running || meeting.status.ending) return;
			for (const t of timers) clearTimeout(t);
			timers.clear();
			clearInterval(ticker);
			speaking = null;
			busy = 0;
			// As the app: ending at once, then the final key facts take a moment.
			status({ ending: true, agent: 'thinking', activity: 'Das Wichtigste zum Schluss', mic_db: null, system_db: null });
			later(params.get('ending') === 'hold' ? 3_600_000 : 2500, () => {
				meeting.running = false;
				meeting.resumable = true;
				meeting.lines = meeting.lines.map((l) => ({ ...l, live: false }));
				meeting.cards = meeting.cards.filter((c) => !c.pending);
				meeting.status = idle();
				snapshot();
			});
		},
		resume() {
			if (meeting.running || !meeting.resumable) return;
			meeting.running = true;
			meeting.resumable = false;
			meeting.status = { ...idle(), agent: 'listening', system_audio: systemAudio() };
			snapshot();
			begin();
		},
		ask(question: string) {
			if (!meeting.running) return;
			const a = ASKED.find((q) => q.match.test(question))?.answer ?? fallback(question);
			answer(a, question, 2000);
		}
	};
}
