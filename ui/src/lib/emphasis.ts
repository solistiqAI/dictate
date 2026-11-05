// "Das Wichtigste" at a glance: dates, weekdays, times, amounts and percentages in a key fact are
// set in bold (Markdown `**…**`), whether the agent marked them or not. What is already bold, code,
// a link or a URL stays as it is.

const MONTHS =
	'Januar|Jänner|Februar|März|April|Mai|Juni|Juli|August|September|Oktober|November|Dezember|' +
	'January|February|March|May|June|July|October|December';
const DAYS =
	'Montag|Dienstag|Mittwoch|Donnerstag|Freitag|Samstag|Sonntag|' +
	'Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday';
const AMOUNT = String.raw`\d[\d.,]*(?:\s?(?:k|Tsd\.|Mio\.?|Mrd\.?))?`;

const PATTERNS = [
	// 14.11.2026, 14.11.
	String.raw`\d{1,2}\.\d{1,2}\.(?:\d{4}|\d{2}(?!\d))?`,
	// 14. November 2026, 14 November, November 14, 2026
	String.raw`\d{1,2}\.?\s(?:${MONTHS})(?:\s\d{4})?`,
	String.raw`(?:${MONTHS})\s\d{1,2}(?:,\s\d{4})?`,
	// 10:00 Uhr, 10 Uhr, 10:30, 3 pm
	String.raw`\d{1,2}:\d{2}(?:\s?Uhr)?`,
	String.raw`\d{1,2}\sUhr`,
	String.raw`\d{1,2}(?::\d{2})?\s?(?:am|pm)\b`,
	// 4.800 €, 4.800 Euro, € 4,800, $1.2k, 12 Mio. EUR
	String.raw`(?:€|\$)\s?${AMOUNT}`,
	String.raw`${AMOUNT}\s?(?:€|EUR\b|Euro\b|\$|USD\b)`,
	// 11,8 %
	String.raw`\d[\d.,]*\s?%`,
	// bis Freitag
	String.raw`\b(?:${DAYS})\b`
];
const FACT = new RegExp(`(?<![\\w.,])(?:${PATTERNS.join('|')})`, 'g');
// Spans that stay as they are: bold, code, links, URLs.
const KEPT = /(\*\*[^*]+\*\*|`[^`]*`|\[[^\]]*\]\([^)]*\)|https?:\/\/\S+)/;

export function emphasize(text: string): string {
	return text
		.split(KEPT)
		.map((part, i) => (i % 2 ? part : part.replace(FACT, (m) => `**${m.trimEnd()}**`)))
		.join('');
}
