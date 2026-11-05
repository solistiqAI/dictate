import { test } from 'node:test';
import assert from 'node:assert/strict';

import { keyCaps } from './keys.ts';
import { translate, type Key, type Lang } from './strings.ts';
import type { Platform } from './protocol.ts';

const caps = (spec: string, platform: Platform, lang: Lang = 'de') =>
	keyCaps(spec, (key: Key, params?: Record<string, string | number>) => translate(lang, key, params, platform));

test('macOS draws modifiers as symbols', () => {
	assert.deepEqual(caps('shift+alt+cmd+5', 'macos'), ['⇧', '⌥', '⌘', '5']);
	assert.deepEqual(caps('ctrl+opt+space', 'macos'), ['⌃', '⌥', 'SPACE']);
	assert.deepEqual(caps('fn', 'macos'), ['fn']);
	assert.deepEqual(caps('right-option', 'macos'), ['rechts ⌥']);
	assert.deepEqual(caps('right-cmd', 'macos', 'en'), ['right ⌘']);
	assert.deepEqual(caps('mouse4', 'macos'), ['Maustaste 4']);
});

test('Windows names the keys, cmd is the Windows key', () => {
	assert.deepEqual(caps('shift+alt+cmd+5', 'windows'), ['Umschalt', 'Alt', 'Win', '5']);
	assert.deepEqual(caps('shift+ctrl+cmd+5', 'windows', 'en'), ['Shift', 'Ctrl', 'Win', '5']);
	assert.deepEqual(caps('right-alt', 'windows'), ['rechts Alt']);
	assert.deepEqual(caps('f13', 'windows'), ['F13']);
});

test('Linux calls cmd Super', () => {
	assert.deepEqual(caps('control+command+d', 'linux'), ['Strg', 'Super', 'D']);
	assert.deepEqual(caps('right-option', 'linux', 'en'), ['right Alt']);
	assert.deepEqual(caps('right-control', 'linux', 'en'), ['right Ctrl']);
	assert.deepEqual(caps('globe', 'linux'), ['Fn']);
	assert.deepEqual(caps('mouse3', 'linux', 'en'), ['Mouse button 3']);
});
