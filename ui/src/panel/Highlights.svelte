<script lang="ts">
	// "Das Wichtigste": the agent's short list of the meeting's key facts, pinned above the cards.
	// The agent sends the whole list each time it changes; lines that are new light up briefly.
	// Dates, times, amounts and percentages are bold (`emphasize`), so they stand out at a glance.
	import Icon from '../components/Icon.svelte';
	import { send } from '../lib/bridge.svelte.ts';
	import { emphasize } from '../lib/emphasis.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import { links, markdown } from './attach.ts';

	let { items, open = $bindable(true) }: { items: string[]; open?: boolean } = $props();

	// Lines seen before (the first list counts as seen: no flash when the panel opens mid-meeting).
	let seen: Set<string> | null = null;
	const fresh = $derived.by(() => {
		const before = seen;
		seen = new Set(items);
		return before ? new Set(items.filter((item) => !before.has(item))) : new Set<string>();
	});

	function copy() {
		send({ type: 'copy', text: items.map((item) => `- ${item}`).join('\n') });
	}
</script>

<section class="highlights" class:open aria-label={t('panel.highlights')}>
	<div class="head">
		<button type="button" class="toggle" aria-expanded={open} onclick={() => (open = !open)}>
			<span class="star"><Icon name="star" size={13} /></span>
			{t('panel.highlights')}
			{#if !open}<span class="count">{items.length}</span>{/if}
		</button>
		{#if open}
			<button type="button" class="copy" title={t('common.copy')} aria-label={t('common.copy')} onclick={copy}>
				<Icon name="copy" size={13} />
			</button>
		{/if}
	</div>
	{#if open}
		<ul {@attach links}>
			{#each items as item (item)}
				<li class:fresh={fresh.has(item)} {@attach markdown(emphasize(item))}></li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.highlights {
		flex: none;
		margin: 8px 8px 0;
		border-radius: var(--radius-group);
		background: color-mix(in srgb, var(--accent) 9%, var(--surface));
		box-shadow: inset 0 0 0 0.5px color-mix(in srgb, var(--accent) 30%, transparent);
	}
	.head {
		display: flex;
		align-items: center;
	}
	.toggle {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 12px;
		border: 0;
		background: transparent;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.4px;
		text-transform: uppercase;
		color: var(--accent);
		text-align: left;
	}
	.star {
		display: grid;
	}
	.count {
		margin-left: auto;
		min-width: 18px;
		height: 16px;
		padding: 0 5px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		font-size: 10.5px;
		line-height: 16px;
		text-align: center;
		letter-spacing: 0;
	}
	.copy {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		margin-right: 6px;
		border: 0;
		border-radius: var(--radius);
		background: transparent;
		color: var(--muted);
		opacity: 0;
		transition: opacity 0.12s;
	}
	.highlights:hover .copy,
	.copy:focus-visible {
		opacity: 1;
	}
	.copy:hover {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--text);
	}
	ul {
		margin: 0;
		padding: 0 12px 10px 26px;
		list-style: none;
		font-size: 13px;
		line-height: 1.4;
		-webkit-user-select: text;
		user-select: text;
		overflow-wrap: anywhere;
	}
	li {
		position: relative;
		padding: 2px 4px;
		margin-left: -4px;
		border-radius: 4px;
	}
	li::before {
		content: '';
		position: absolute;
		left: -10px;
		top: calc(0.7em - 2px);
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--accent);
	}
	li :global(p) {
		margin: 0;
	}
	li :global(strong) {
		font-weight: 700;
		color: color-mix(in srgb, var(--accent) 55%, var(--text));
	}
	li :global(code) {
		font-family: ui-monospace, 'SF Mono', Menlo, monospace;
		font-size: 11.5px;
	}
	li :global(a) {
		color: var(--accent);
		text-decoration: none;
		border-bottom: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
		cursor: pointer;
	}
	.fresh {
		animation: lit 2.4s ease-out;
	}
	@keyframes lit {
		0%,
		30% {
			background: color-mix(in srgb, var(--accent) 22%, transparent);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.fresh {
			animation: none;
		}
	}
</style>
