import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import { applyMeeting, isMeetingEvent } from './meeting.ts';
import type { Event, Meeting, MeetingLine } from './protocol.ts';

const fixtures = JSON.parse(readFileSync(new URL('./protocol.fixtures.json', import.meta.url), 'utf8')) as { events: Event[] };

test('fixture events build a meeting', () => {
	let meeting: Meeting | null = null;
	for (const event of fixtures.events) if (isMeetingEvent(event)) meeting = applyMeeting(meeting, event);
	assert.ok(meeting);
	assert.deepEqual(meeting.lines.map((l) => [l.id, l.text, l.live]), [[0, 'Wie weit ist Nordlicht?', false], [3, 'Genau, und', true]]);
	assert.deepEqual(meeting.cards.map((c) => [c.id, c.pending]), [[0, false], [1, true]]);
	assert.equal(meeting.status.agent, 'thinking');
	assert.equal(meeting.status.activity, 'Sucht im CRM');
	assert.deepEqual(meeting.highlights, ['Nächster Termin: **5.11., 16 Uhr**', 'Kontakt: Jana Vogt (BD)']);
});

test('same id replaces, new id appends, nothing before the snapshot', () => {
	const line = (id: number, text: string, live = false): MeetingLine => ({ id, who: 'me', at: id, text, live });
	assert.equal(applyMeeting(null, { type: 'meeting_line', line: line(0, 'x') }), null);
	const meeting: Meeting = {
		running: true,
		started: 0,
		lines: [line(0, 'Hallo'), line(1, 'Wie', true)],
		cards: [],
		highlights: [],
		resumable: false,
		status: {
			agent: 'listening',
			activity: '',
			mic_db: null,
			system_db: null,
			system_audio: 'off',
			problem: '',
			ending: false
		}
	};
	applyMeeting(meeting, { type: 'meeting_line', line: line(1, 'Wie geht es?') });
	applyMeeting(meeting, { type: 'meeting_line', line: line(2, 'Gut', true) });
	applyMeeting(meeting, { type: 'meeting_line', line: line(0, 'Hallo!') });
	assert.deepEqual(meeting.lines.map((l) => [l.id, l.text, l.live]), [[0, 'Hallo!', false], [1, 'Wie geht es?', false], [2, 'Gut', true]]);
	const fresh = applyMeeting(meeting, { type: 'meeting', meeting: { ...meeting, lines: [] } });
	assert.equal(fresh?.lines.length, 0);
});

test('empty line and empty finished card are removed', () => {
	const meeting: Meeting = {
		running: true,
		started: 0,
		lines: [{ id: 0, who: 'me', at: 0, text: 'Echo', live: true }],
		cards: [{ id: 4, at: 0, title: 'Entwurf', body: '…', question: null, pending: true }],
		highlights: [],
		resumable: false,
		status: {
			agent: 'listening',
			activity: '',
			mic_db: null,
			system_db: null,
			system_audio: 'off',
			problem: '',
			ending: false
		}
	};
	applyMeeting(meeting, { type: 'meeting_line', line: { id: 0, who: 'me', at: 0, text: '', live: false } });
	applyMeeting(meeting, { type: 'meeting_card', card: { id: 4, at: 0, title: '', body: '', question: null, pending: false } });
	applyMeeting(meeting, { type: 'meeting_line', line: { id: 7, who: 'me', at: 0, text: '', live: false } });
	assert.equal(meeting.lines.length, 0);
	assert.equal(meeting.cards.length, 0);
});
