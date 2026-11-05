<script lang="ts">
	import Icon from '../components/Icon.svelte';
	import { send } from '../lib/bridge.svelte.ts';
	import { i18n, t } from '../lib/i18n.svelte.ts';
	import type { MeetingCard } from '../lib/protocol.ts';
	import { ago } from '../lib/time.ts';
	import { links, markdown } from './attach.ts';

	let {
		card,
		now,
		newest = false,
		fresh = false,
		activity = ''
	}: { card: MeetingCard; now: number; newest?: boolean; fresh?: boolean; activity?: string } = $props();

	function copy() {
		send({ type: 'copy', text: [card.title, card.body].filter(Boolean).join('\n\n') });
	}
</script>

<article class="card" class:newest class:fresh class:pending={card.pending} aria-busy={card.pending}>
	{#if card.question}
		<p class="question">{t('panel.question', { question: card.question })}</p>
	{/if}
	<header>
		<h3>
			{#if card.title}{card.title}{:else if card.pending}<span class="seeking"
					>{t('panel.activity', { activity: activity || t('panel.seeking') })}</span
				>{/if}
		</h3>
		<span class="time">{card.pending && !card.title ? '' : ago(card.at, now, i18n.lang)}</span>
		{#if !card.pending}
			<button type="button" class="copy" title={t('common.copy')} aria-label={t('common.copy')} onclick={copy}>
				<Icon name="copy" size={13} />
			</button>
		{/if}
	</header>
	{#if card.body}
		<div class="body" {@attach links} {@attach markdown(card.body)}></div>
	{/if}
	{#if card.pending}
		<div class="shimmer" aria-hidden="true">
			<i></i>
			{#if !card.body}<i></i><i></i>{/if}
		</div>
	{/if}
</article>

<style>
	.card {
		position: relative;
		padding: 11px 14px 12px;
		border-bottom: 1px solid var(--separator);
	}
	/* The newest answer is the one to glance at: raised, with the accent at its edge. */
	.card.newest {
		margin: 0 8px 6px;
		padding: 12px 14px 13px 16px;
		border: 0;
		border-radius: var(--radius-group);
		background: var(--surface);
		box-shadow: var(--shadow-group), 0 2px 10px rgba(0, 0, 0, 0.05);
		overflow: hidden;
	}
	.card.newest::before {
		content: '';
		position: absolute;
		left: 0;
		top: 0;
		bottom: 0;
		width: 3px;
		background: var(--accent);
	}
	.card.newest.pending::before {
		background: color-mix(in srgb, var(--accent) 45%, transparent);
	}
	.fresh {
		animation: arrive 0.28s ease-out;
	}
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(-4px);
		}
	}
	.question {
		margin: 0 0 5px;
		font-size: 12px;
		color: var(--muted);
		overflow-wrap: anywhere;
	}
	header {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	h3 {
		flex: 1;
		min-width: 0;
		margin: 0;
		font-size: 13px;
		font-weight: 650;
		line-height: 1.3;
		letter-spacing: -0.1px;
	}
	.newest h3 {
		font-size: 13.5px;
	}
	.seeking {
		font-weight: 500;
		color: var(--muted);
	}
	.time {
		flex: none;
		font-size: 11px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	/* Over the time while the pointer is on the card. */
	.copy {
		position: absolute;
		top: 7px;
		right: 8px;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: var(--radius);
		background: var(--bg);
		box-shadow: -10px 0 8px var(--bg);
		color: var(--muted);
		opacity: 0;
	}
	.newest .copy {
		background: var(--surface);
		box-shadow: -10px 0 8px var(--surface);
	}
	.card:hover .copy,
	.copy:focus-visible {
		opacity: 1;
	}
	.copy:hover {
		color: var(--text);
	}
	.body {
		margin-top: 4px;
		font-size: 12.5px;
		line-height: 1.45;
		-webkit-user-select: text;
		user-select: text;
		overflow-wrap: anywhere;
	}
	.body :global(p) {
		margin: 0;
	}
	.body :global(p + p),
	.body :global(p + ul),
	.body :global(p + ol),
	.body :global(ul + p),
	.body :global(ol + p) {
		margin-top: 5px;
	}
	.body :global(ul),
	.body :global(ol) {
		margin: 0;
		padding-left: 16px;
	}
	.body :global(ul) {
		list-style: none;
		padding-left: 12px;
	}
	.body :global(ul li) {
		position: relative;
	}
	.body :global(ul li::before) {
		content: '';
		position: absolute;
		left: -10px;
		top: 0.62em;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--muted) 70%, transparent);
	}
	.body :global(li + li) {
		margin-top: 1px;
	}
	.body :global(strong) {
		font-weight: 650;
	}
	.body :global(code) {
		font-family: ui-monospace, 'SF Mono', Menlo, monospace;
		font-size: 11.5px;
		padding: 0 4px;
		border-radius: 4px;
		background: var(--surface-2);
	}
	.body :global(a) {
		color: var(--accent);
		text-decoration: none;
		border-bottom: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		cursor: pointer;
	}
	.body :global(a:hover) {
		border-bottom-color: var(--accent);
	}
	.shimmer {
		display: grid;
		gap: 6px;
		margin-top: 8px;
	}
	.shimmer i {
		height: 7px;
		border-radius: 4px;
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			color-mix(in srgb, var(--accent) 22%, var(--surface-2)) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
		animation: shimmer 1.4s linear infinite;
	}
	.shimmer i:nth-child(2) {
		width: 82%;
	}
	.shimmer i:nth-child(3) {
		width: 56%;
	}
	@keyframes shimmer {
		from {
			background-position: 100% 0;
		}
		to {
			background-position: -100% 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.fresh,
		.shimmer i {
			animation: none;
		}
	}
</style>
