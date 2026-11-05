// A trigger spec ("shift+alt+cmd+5", "fn", "right-alt", "mouse4") as the names on its key caps.
// The spec names keys as macOS does on every OS (core/src/trigger.rs); `cmd` is the Windows key
// on Windows (core/src/hook.rs) and Super on Linux (core/src/linux.rs). The names come from the
// strings, so they follow the language and the OS: "⌥" on the Mac, "Alt" elsewhere.

import type { Key } from './strings.ts';

type Translate = (key: Key, params?: Record<string, string | number>) => string;

// Every spelling trigger.rs takes, by the key it names.
const MODIFIERS: Record<string, Key> = {
	shift: 'keys.shift',
	ctrl: 'keys.ctrl',
	control: 'keys.ctrl',
	alt: 'keys.alt',
	opt: 'keys.alt',
	option: 'keys.alt',
	cmd: 'keys.cmd',
	command: 'keys.cmd',
	fn: 'keys.fn',
	globe: 'keys.fn'
};

/** The caps of `spec`: "right-option" = "rechts ⌥" / "right Alt", "mouse4" = "Maustaste 4". */
export function keyCaps(spec: string, t: Translate): string[] {
	return spec
		.split('+')
		.filter(Boolean)
		.map((part) => {
			const key = part.toLowerCase();
			if (MODIFIERS[key]) return t(MODIFIERS[key]);
			const right = key.startsWith('right-') ? MODIFIERS[key.slice('right-'.length)] : undefined;
			if (right && right !== 'keys.fn') return t('keys.right', { key: t(right) });
			const mouse = /^mouse(\d+)$/.exec(key);
			if (mouse) return t('keys.mouse', { n: mouse[1] });
			return key.toUpperCase();
		});
}
