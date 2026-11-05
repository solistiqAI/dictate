import { test } from 'node:test';
import assert from 'node:assert/strict';

import { LINUX, WINDOWS, de, en, translate, type Key } from './strings.ts';

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

test('English has every German text with the same placeholders', () => {
	assert.deepEqual(Object.keys(en).sort(), Object.keys(de).sort());
	for (const key of Object.keys(de) as Key[]) {
		assert.ok(en[key].trim(), `${key} is empty in English`);
		assert.deepEqual(placeholders(en[key]), placeholders(de[key]), key);
	}
});

test('translate fills placeholders', () => {
	assert.equal(translate('de', 'general.reports.target', { target: 'a/b#7' }), 'An a/b#7');
	assert.equal(translate('en', 'general.reports.target', { target: 'a/b#7' }), 'To a/b#7');
});

test('Windows and Linux reword the same texts in both languages, with the same placeholders', () => {
	for (const table of [WINDOWS, LINUX]) {
		assert.deepEqual(Object.keys(table.en).sort(), Object.keys(table.de).sort());
		for (const lang of ['de', 'en'] as const) {
			for (const [key, text] of Object.entries(table[lang]) as [Key, string][]) {
				assert.ok(key in de, `${key} is not a text`);
				assert.deepEqual(placeholders(text), placeholders(de[key]), `${lang} ${key}`);
			}
		}
	}
});

test('no Mac, Apple model or Neural Engine on Windows and Linux', () => {
	const mac = /\bMac\b|macOS|Apple|Neural Engine|Bedienungshilfen|Accessibility|Systemeinstellungen|System Settings|[⌘⌥⌃⇧]/;
	const shown = (Object.keys(de) as Key[]).filter((key) => !key.startsWith('mock.'));
	for (const platform of ['windows', 'linux'] as const) {
		for (const lang of ['de', 'en'] as const) {
			for (const key of shown) {
				assert.doesNotMatch(translate(lang, key, undefined, platform), mac, `${platform} ${lang} ${key}`);
			}
		}
	}
	assert.equal(translate('en', 'keys.cmd', undefined, 'windows'), 'Win');
	assert.equal(translate('de', 'keys.cmd', undefined, 'linux'), 'Super');
	assert.equal(translate('de', 'keys.cmd'), '⌘');
});
