// The link to the app: requests go to window.ipc (wry) or, in a browser, to the mock app.
// Events from the app land in `app` and in the listeners registered with `on`.

import { i18n } from './i18n.svelte.ts';
import { applyMeeting, isMeetingEvent } from './meeting.ts';
import type { Page } from './pages.ts';
import type { Event, Meeting, Request, State } from './protocol.ts';

declare global {
	interface Window {
		ipc?: { postMessage(message: string): void };
		dictate: { receive(event: Event): void };
	}
}

/** Which window this page is: the main window, or the Souffleur's side panel (index.html?view=panel). */
export const view: 'main' | 'panel' = new URLSearchParams(location.search).get('view') === 'panel' ? 'panel' : 'main';

export const app = $state<{
	state: State | null;
	note: string;
	page: Page;
	open: number | null;
	meeting: Meeting | null;
}>({
	state: null,
	note: '',
	page: (new URLSearchParams(location.search).get('page') as Page | null) ?? 'history',
	// A dictation to open for correcting (menu "Letztes Diktat korrigieren…"); History takes it.
	open: null,
	// The Souffleur's meeting: a snapshot (`meeting`) with lines, cards and status applied since.
	meeting: null
});

type Listener = (event: Event) => void;
const listeners = new Set<Listener>();

/** Calls `listener` for every event from the app until the returned function is called. */
export function on(listener: Listener): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

let noteTimer: ReturnType<typeof setTimeout> | undefined;

function receive(event: Event) {
	if (event.type === 'state') {
		app.state = event.state;
		i18n.lang = event.state.lang;
		i18n.platform = event.state.platform;
		document.documentElement.lang = event.state.lang;
	}
	if (event.type === 'show') {
		app.page = event.page;
		app.open = event.entry;
	}
	if (isMeetingEvent(event)) app.meeting = applyMeeting(app.meeting, event);
	if (event.type === 'note') {
		app.note = event.text;
		clearTimeout(noteTimer);
		noteTimer = setTimeout(() => (app.note = ''), 2500);
	}
	for (const listener of listeners) listener(event);
}

window.dictate = { receive };

let mock: ((request: Request) => void) | undefined;

export async function connect() {
	if (!window.ipc) {
		const { createMock } = await import('./mock.ts');
		mock = createMock(receive);
	}
	send({ type: 'load' });
}

// The window has no title bar of its own: the top strip ([data-drag]) moves it.
addEventListener('mousedown', (event) => {
	if (event.button === 0 && (event.target as Element).closest?.('[data-drag]')) send({ type: 'drag' });
});

export function send(request: Request) {
	if (window.ipc) window.ipc.postMessage(JSON.stringify(request));
	else mock?.(request);
}
