// The Souffleur's meeting as the page keeps it: one snapshot (`meeting`), then lines, cards and
// status ticks applied to it. A line or card with a known id replaces the old one, a new id is added.

import type { Event, Meeting } from './protocol.ts';

export type MeetingEvent = Extract<
	Event,
	{ type: 'meeting' | 'meeting_line' | 'meeting_card' | 'meeting_status' | 'meeting_highlights' }
>;

export function isMeetingEvent(event: Event): event is MeetingEvent {
	return (
		event.type === 'meeting' ||
		event.type === 'meeting_line' ||
		event.type === 'meeting_card' ||
		event.type === 'meeting_status' ||
		event.type === 'meeting_highlights'
	);
}

function upsert<T extends { id: number }>(list: T[], item: T) {
	// Ids grow, and updates are almost always for the newest items: look from the end.
	if (!list.length || list[list.length - 1].id < item.id) {
		list.push(item);
		return;
	}
	for (let i = list.length - 1; i >= 0; i--) {
		if (list[i].id === item.id) {
			list[i] = item;
			return;
		}
	}
	list.push(item);
}

/** Drops the item with this id (a line that turned out to be noise or the speakers' echo, a
 *  draft card the final answer no longer has). */
function remove<T extends { id: number }>(list: T[], id: number) {
	const i = list.findIndex((item) => item.id === id);
	if (i >= 0) list.splice(i, 1);
}

/** Applies `event` to `meeting` (changed in place) and returns it; parts before a snapshot are dropped.
 *  A line with empty text and a finished card without title and body mean: gone. */
export function applyMeeting(meeting: Meeting | null, event: MeetingEvent): Meeting | null {
	if (event.type === 'meeting') return event.meeting;
	if (!meeting) return null;
	if (event.type === 'meeting_line') {
		if (event.line.text) upsert(meeting.lines, event.line);
		else remove(meeting.lines, event.line.id);
	} else if (event.type === 'meeting_card') {
		const { card } = event;
		if (card.pending || card.title || card.body) upsert(meeting.cards, card);
		else remove(meeting.cards, card.id);
	} else if (event.type === 'meeting_highlights') meeting.highlights = event.highlights;
	else meeting.status = event.status;
	return meeting;
}
