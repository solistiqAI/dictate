import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import type { Event, Request } from './protocol.ts';

// The same samples src/protocol.rs parses: here they must at least name every message type the
// page knows, so a new request or event cannot be added on one side only.
const fixtures = JSON.parse(readFileSync(new URL('./protocol.fixtures.json', import.meta.url), 'utf8')) as {
	requests: Request[];
	events: Event[];
};

const REQUESTS: Record<Request['type'], true> = {
	load: true, copy: true, find: true, save_entry: true, config: true, words: true, replacements: true,
	try: true, record_trigger: true, download_model: true, mic_test: true, open_privacy: true,
	open_folder: true, drag: true, start_meeting: true, stop_meeting: true, resume_meeting: true,
	ask_meeting: true,
	open_panel: true, open_page: true, open_url: true, open_meetings: true
};
const EVENTS: Record<Event['type'], true> = {
	state: true, candidates: true, tried: true, captured: true, mic_level: true, note: true, show: true,
	meeting: true, meeting_line: true, meeting_card: true, meeting_status: true,
	meeting_highlights: true
};

test('every message type has a sample', () => {
	assert.deepEqual(new Set(fixtures.requests.map((r) => r.type)), new Set(Object.keys(REQUESTS)));
	assert.deepEqual(new Set(fixtures.events.map((e) => e.type)), new Set(Object.keys(EVENTS)));
});
