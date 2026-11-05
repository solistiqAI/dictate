// The little Markdown the Souffleur's agent writes, as HTML: paragraphs, "- " and "1. " lists,
// **bold**, `code`, [links](https://…) and bare https links. Everything else is escaped text, links
// only for http(s) and mailto. The page catches clicks on links and sends `open_url`.

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

export function escapeHtml(text: string): string {
	return text.replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

function safeUrl(url: string): boolean {
	return /^(https?:\/\/[^\s]+|mailto:[^\s]+)$/i.test(url);
}

function link(url: string, html: string): string {
	const href = escapeHtml(url);
	return `<a href="${href}" title="${href}">${html}</a>`;
}

// `code` | **bold** | [text](url) | bare https://…
const INLINE = /`([^`\n]+)`|\*\*(.+?)\*\*|\[([^\]\n]+)\]\(([^()\s]+)\)|(https?:\/\/[^\s<>"]+)/g;

function inline(text: string, links = true): string {
	let out = '';
	let last = 0;
	for (const m of text.matchAll(INLINE)) {
		out += escapeHtml(text.slice(last, m.index));
		last = m.index + m[0].length;
		if (m[1] !== undefined) out += `<code>${escapeHtml(m[1])}</code>`;
		else if (m[2] !== undefined) out += `<strong>${inline(m[2], links)}</strong>`;
		else if (m[3] !== undefined) {
			out += links && safeUrl(m[4]) ? link(m[4], inline(m[3], false)) : `[${inline(m[3], links)}](${escapeHtml(m[4])})`;
		} else {
			// Sentence marks and a closing bracket after a bare link belong to the text.
			const trail = /[.,;:!?)\]'"]+$/.exec(m[5])?.[0] ?? '';
			const url = m[5].slice(0, m[5].length - trail.length);
			out += (links ? link(url, escapeHtml(url)) : escapeHtml(url)) + escapeHtml(trail);
		}
	}
	return out + escapeHtml(text.slice(last));
}

type Block = { kind: 'p' | 'ul' | 'ol'; lines: string[] };

/** Markdown → HTML that is safe to put into the page as it is. */
export function renderMarkdown(source: string): string {
	const blocks: Block[] = [];
	let current = null as Block | null;
	for (const raw of source.replace(/\r\n?/g, '\n').split('\n')) {
		const line = raw.trim();
		if (!line) {
			current = null;
			continue;
		}
		const bullet = /^[-*•]\s+(.*)$/.exec(line);
		const number = bullet ? null : /^\d+[.)]\s+(.*)$/.exec(line);
		// "### Heading" becomes a bold line: cards have one title already.
		const heading = /^#{1,6}\s+(.*)$/.exec(line);
		const kind = bullet ? 'ul' : number ? 'ol' : 'p';
		const text = bullet?.[1] ?? number?.[1] ?? (heading ? `**${heading[1]}**` : line);
		if (current?.kind !== kind) {
			current = { kind, lines: [] };
			blocks.push(current);
		}
		current.lines.push(text);
	}
	return blocks
		.map((block) =>
			block.kind === 'p'
				? `<p>${block.lines.map((l) => inline(l)).join('<br>')}</p>`
				: `<${block.kind}>${block.lines.map((l) => `<li>${inline(l)}</li>`).join('')}</${block.kind}>`
		)
		.join('');
}
