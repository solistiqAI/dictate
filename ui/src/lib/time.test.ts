import { test } from 'node:test';
import assert from 'node:assert/strict';

import { ago, clock, dayLabel, elapsed, groupByDay } from './time.ts';
import { changedWords } from './mock.ts';

const now = new Date(2026, 10, 1, 10, 48);
const at = (d: Date) => d.getTime() / 1000;

test('day labels', () => {
	assert.equal(dayLabel(at(new Date(2026, 10, 1, 7, 0)), now), 'Heute');
	assert.equal(dayLabel(at(new Date(2026, 9, 31, 23, 59)), now), 'Gestern');
	assert.equal(dayLabel(at(new Date(2026, 9, 29, 9, 0)), now), 'Donnerstag, 29. Oktober');
	assert.equal(dayLabel(at(new Date(2025, 11, 24, 9, 0)), now), 'Mittwoch, 24. Dezember 2025');
	assert.equal(dayLabel(null, now), 'Früher');
	assert.equal(clock(at(new Date(2026, 10, 1, 9, 5))), '09:05');
});

test('day labels in English', () => {
	assert.equal(dayLabel(at(new Date(2026, 10, 1, 7, 0)), now, 'en'), 'Today');
	assert.equal(dayLabel(at(new Date(2026, 9, 31, 23, 59)), now, 'en'), 'Yesterday');
	assert.equal(dayLabel(at(new Date(2026, 9, 30, 9, 0)), now, 'en'), 'Friday, October 30');
	assert.equal(dayLabel(at(new Date(2026, 9, 29, 9, 0)), now, 'en'), 'Thursday, October 29');
	assert.equal(dayLabel(at(new Date(2025, 11, 24, 9, 0)), now, 'en'), 'Wednesday, December 24, 2025');
	assert.equal(dayLabel(null, now, 'en'), 'Earlier');
});

test('groups newest first, untimed last', () => {
	const entries = [
		{ id: 0, at: null, text: 'alt' },
		{ id: 1, at: at(new Date(2026, 9, 31, 17, 0)), text: 'gestern' },
		{ id: 2, at: at(new Date(2026, 10, 1, 8, 0)), text: 'früh' },
		{ id: 3, at: at(new Date(2026, 10, 1, 10, 0)), text: 'spät' }
	];
	const groups = groupByDay(entries, now);
	assert.deepEqual(
		groups.map((g) => [g.label, g.entries.map((e) => e.text)]),
		[
			['Heute', ['spät', 'früh']],
			['Gestern', ['gestern']],
			['Früher', ['alt']]
		]
	);
	assert.deepEqual(
		groupByDay(entries, now, 'en').map((g) => g.label),
		['Today', 'Yesterday', 'Earlier']
	);
});

test('mock finds changed words', () => {
	const found = changedWords(
		'erstelle Matchdruckers und Matschin.',
		'erstelle Merge Requests und merge sie.',
		['Merge Request']
	);
	assert.deepEqual(found, [
		{ heard: 'Matchdruckers', meant: 'Merge Requests', kind: 'dictionary' },
		{ heard: 'Matschin', meant: 'merge sie', kind: 'replacement' }
	]);
});

test('meeting times', () => {
	const t = at(now);
	assert.equal(ago(t - 10, t), 'gerade eben');
	assert.equal(ago(t + 2, t), 'gerade eben');
	assert.equal(ago(t - 70, t), 'vor 1 Min');
	assert.equal(ago(t - 25 * 60, t), 'vor 25 Min');
	assert.equal(ago(at(new Date(2026, 10, 1, 9, 5)), t), '09:05');
	assert.equal(ago(t - 10, t, 'en'), 'just now');
	assert.equal(ago(t - 70, t, 'en'), '1 min ago');
	assert.equal(ago(t - 25 * 60, t, 'en'), '25 min ago');
	assert.equal(elapsed(0), '0:00');
	assert.equal(elapsed(247.9), '4:07');
	assert.equal(elapsed(3750), '1:02:30');
});
