// Attachments the panel's Markdown blocks share (cards, "Das Wichtigste").
import { send } from '../lib/bridge.svelte.ts';
import { renderMarkdown } from '../lib/markdown.ts';

/** renderMarkdown escapes everything and only writes its own tags, so its HTML can go in as it is. */
export function markdown(source: string) {
	return (node: HTMLElement) => {
		node.innerHTML = renderMarkdown(source);
	};
}

/** Links open in the browser (open_url); the page itself never navigates. */
export function links(node: HTMLElement) {
	const click = (event: MouseEvent) => {
		const a = (event.target as Element).closest?.('a');
		if (!a) return;
		event.preventDefault();
		send({ type: 'open_url', url: a.getAttribute('href') ?? '' });
	};
	node.addEventListener('click', click);
	node.addEventListener('auxclick', click);
	return () => {
		node.removeEventListener('click', click);
		node.removeEventListener('auxclick', click);
	};
}
