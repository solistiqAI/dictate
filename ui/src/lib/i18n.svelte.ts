// The page's language and OS: what the app resolved (state.lang, state.platform), set by
// bridge.svelte.ts on every state event. `t` reads both, so texts in templates switch with them.

import type { Platform } from './protocol.ts';
import { translate, locale, type Key, type Lang } from './strings.ts';

export const i18n = $state<{ lang: Lang; platform: Platform }>({
	// Before the first state arrives (a split second): what the browser says.
	lang: navigator.language?.toLowerCase().startsWith('de') ? 'de' : 'en',
	platform: /Windows/.test(navigator.userAgent) ? 'windows' : /Linux/.test(navigator.userAgent) ? 'linux' : 'macos'
});

export function t(key: Key, params?: Record<string, string | number>): string {
	return translate(i18n.lang, key, params, i18n.platform);
}

/** The locale for numbers and dates ("de-DE", "en-US"). */
export function currentLocale(): string {
	return locale(i18n.lang);
}
