// Grouping and labels for the history: "Heute", "Gestern", "Donnerstag, 30. Oktober", "Früher"
// (English: "Today", "Yesterday", "Thursday, October 30", "Earlier").

import type { Entry } from './protocol.ts';
import { translate, type Lang } from './strings.ts';

const WEEKDAYS: Record<Lang, string[]> = {
	de: ['Sonntag', 'Montag', 'Dienstag', 'Mittwoch', 'Donnerstag', 'Freitag', 'Samstag'],
	en: ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']
};
const MONTHS: Record<Lang, string[]> = {
	de: [
		'Januar', 'Februar', 'März', 'April', 'Mai', 'Juni',
		'Juli', 'August', 'September', 'Oktober', 'November', 'Dezember'
	],
	en: [
		'January', 'February', 'March', 'April', 'May', 'June',
		'July', 'August', 'September', 'October', 'November', 'December'
	]
};

function startOfDay(date: Date): number {
	const d = new Date(date);
	d.setHours(0, 0, 0, 0);
	return d.getTime();
}

export function dayLabel(at: number | null, now: Date, lang: Lang = 'de'): string {
	if (at === null) return translate(lang, 'time.earlier');
	const date = new Date(at * 1000);
	const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
	if (days === 0) return translate(lang, 'time.today');
	if (days === 1) return translate(lang, 'time.yesterday');
	const label = translate(lang, 'time.date', {
		weekday: WEEKDAYS[lang][date.getDay()],
		day: date.getDate(),
		month: MONTHS[lang][date.getMonth()]
	});
	if (date.getFullYear() === now.getFullYear()) return label;
	return translate(lang, 'time.date_year', { date: label, year: date.getFullYear() });
}

export function clock(at: number | null): string {
	if (at === null) return '';
	const d = new Date(at * 1000);
	return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

/** Newest first, one group per day; entries without a time last. */
export function groupByDay(entries: Entry[], now: Date, lang: Lang = 'de'): { label: string; entries: Entry[] }[] {
	const groups: { label: string; entries: Entry[] }[] = [];
	const sorted = [...entries].sort((x, y) => (y.at ?? -1) - (x.at ?? -1) || y.id - x.id);
	for (const entry of sorted) {
		const label = dayLabel(entry.at, now, lang);
		const last = groups.at(-1);
		if (last?.label === label) last.entries.push(entry);
		else groups.push({ label, entries: [entry] });
	}
	return groups;
}

/** "gerade eben", "vor 1 Min", "vor 25 Min" ("just now", "1 min ago"), then the clock time. */
export function ago(at: number, now: number, lang: Lang = 'de'): string {
	const seconds = Math.max(0, now - at);
	if (seconds < 50) return translate(lang, 'time.just_now');
	const minutes = Math.max(1, Math.round(seconds / 60));
	if (minutes < 60) return translate(lang, 'time.minutes_ago', { n: minutes });
	return clock(at);
}

/** Running time of a meeting: "4:07", "1:02:30". */
export function elapsed(seconds: number): string {
	const s = Math.max(0, Math.floor(seconds));
	const pad = (n: number) => String(n).padStart(2, '0');
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
}
