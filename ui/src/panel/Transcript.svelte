<script lang="ts">
	import Icon from '../components/Icon.svelte';
	import { t } from '../lib/i18n.svelte.ts';
	import type { MeetingLine } from '../lib/protocol.ts';

	let { lines, open = $bindable(true) }: { lines: MeetingLine[]; open?: boolean } = $props();

	// The panel shows the end of the meeting; the whole transcript is saved as Markdown.
	const SHOWN = 150;
	const shown = $derived(lines.slice(-SHOWN));

	// Follows new lines while the view is at the bottom; scrolling up stops that until back down.
	let following = $state(true);

	function scrolled(event: UIEvent) {
		const box = event.currentTarget as HTMLElement;
		following = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
	}

	// Runs again for every new or changed last line, and when following starts again.
	function follow(_last: MeetingLine | undefined, on: boolean) {
		return (box: HTMLElement) => {
			if (on) box.scrollTop = box.scrollHeight;
		};
	}
</script>

<section class="transcript" class:open>
	<button type="button" class="toggle" aria-expanded={open} onclick={() => (open = !open)}>
		<span class="chevron"><Icon name="chevron" size={12} /></span>
		{t('panel.transcript')}
		<span class="count"
			>{lines.length === 1 ? t('meeting.sentences.one') : t('meeting.sentences', { n: lines.length })}</span
		>
	</button>
	{#if open}
		<div class="lines" onscroll={scrolled} {@attach follow(shown.at(-1), following)}>
			{#if !shown.length}
				<p class="none">{t('panel.transcript.none')}</p>
			{/if}
			{#each shown as line, i (line.id)}
				{@const first = i === 0 || shown[i - 1].who !== line.who}
				<p class="line {line.who}" class:first class:live={line.live}>
					<span class="who">{first ? (line.who === 'me' ? t('panel.me') : t('panel.others')) : ''}</span>
					<span class="text">{line.text}</span>
				</p>
			{/each}
		</div>
		{#if !following}
			<button type="button" class="end" onclick={() => (following = true)}><Icon name="down" size={12} />
				{t('panel.transcript.latest')}</button
			>
		{/if}
	{/if}
</section>

<style>
	.transcript {
		position: relative;
		flex: none;
		border-top: 1px solid var(--separator);
	}
	.toggle {
		display: flex;
		align-items: center;
		gap: 5px;
		width: 100%;
		height: 30px;
		padding: 0 12px;
		border: 0;
		background: transparent;
		font-size: 12px;
		font-weight: 600;
		color: var(--text);
		text-align: left;
	}
	.toggle:hover {
		background: color-mix(in srgb, var(--text) 4%, transparent);
	}
	.chevron {
		display: grid;
		color: var(--muted);
		transition: transform 0.15s;
	}
	.open .chevron {
		transform: rotate(90deg);
	}
	.count {
		margin-left: auto;
		font-weight: 400;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.lines {
		height: 196px;
		overflow-y: auto;
		padding: 0 12px 10px;
		font-size: 12px;
		line-height: 1.4;
		-webkit-user-select: text;
		user-select: text;
	}
	.line {
		display: grid;
		grid-template-columns: 46px 1fr;
		gap: 6px;
		margin: 0;
		padding: 1px 0;
	}
	.line.first {
		margin-top: 7px;
	}
	.line:first-child {
		margin-top: 0;
	}
	.who {
		font-size: 11px;
		font-weight: 600;
		padding-top: 0.5px;
	}
	.me .who {
		color: var(--me);
	}
	.others .who {
		color: var(--others);
	}
	.text {
		padding-left: 8px;
		border-left: 2px solid transparent;
		overflow-wrap: anywhere;
	}
	.me .text {
		border-left-color: color-mix(in srgb, var(--me) 45%, transparent);
	}
	.others .text {
		border-left-color: color-mix(in srgb, var(--others) 45%, transparent);
	}
	.live .text {
		color: var(--muted);
		font-style: italic;
	}
	.none {
		margin: 4px 0 0;
		color: var(--muted);
	}
	.end {
		position: absolute;
		right: 14px;
		bottom: 10px;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		height: 22px;
		padding: 0 9px;
		border: 0;
		border-radius: 11px;
		font-size: 11px;
		background: var(--text);
		color: var(--bg);
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
	}
</style>
