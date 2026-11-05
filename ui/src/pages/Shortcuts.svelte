<script lang="ts">
	import Group from '../components/Group.svelte';
	import Icon from '../components/Icon.svelte';
	import Keys from '../components/Keys.svelte';
	import Segmented from '../components/Segmented.svelte';
	import { on, send } from '../lib/bridge.svelte.ts';
	import { setConfig } from '../lib/config.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { Mode, State, Trigger } from '../lib/protocol.ts';

	let { data }: { data: State } = $props();

	let recording = $state<number | null>(null);
	let message = $state('');

	// What happens to the text after recording: one Gemma pass lays it out and, with Korrigieren,
	// drops self-corrections and filler words (config: `format`, `correct`).
	type Tidy = 'off' | 'format' | 'both';
	const tidyOptions = $derived<{ value: Tidy; label: string }[]>([
		{ value: 'off', label: t('shortcuts.tidy.off') },
		{ value: 'format', label: t('shortcuts.tidy.format') },
		{ value: 'both', label: t('shortcuts.tidy.both') }
	]);
	const tidyOf = (trigger: Trigger): Tidy => (trigger.correct ? 'both' : trigger.format ? 'format' : 'off');
	const tidyFlags = (tidy: Tidy) => ({ format: tidy !== 'off', correct: tidy === 'both' });

	const modes = $derived<{ value: Mode; label: string }[]>([
		{ value: 'hold', label: t('shortcuts.mode.hold') },
		{ value: 'toggle', label: t('shortcuts.mode.toggle') },
		{ value: 'hybrid', label: t('shortcuts.mode.hybrid') }
	]);

	function update(triggers: Trigger[]) {
		setConfig(data.config, { triggers });
	}

	function record(row: number) {
		recording = row;
		message = t('shortcuts.capture');
		send({ type: 'record_trigger', row });
	}

	$effect(() =>
		on((event) => {
			if (event.type !== 'captured') return;
			recording = null;
			message = event.message;
			if (event.key === null) return;
			const triggers = data.config.triggers.map((trigger, i) =>
				i === event.row ? { ...trigger, key: event.key! } : trigger
			);
			if (event.row >= triggers.length) triggers.push({ key: event.key, mode: 'hybrid', ...tidyFlags('both') });
			update(triggers);
		})
	);
</script>

<header class="page">
	<h1>{t('shortcuts.title')}</h1>
	<p class="muted">{t('shortcuts.intro')}</p>
</header>

<Group footer={t('shortcuts.footer')}>
	{#each data.config.triggers as trigger, row (row)}
		<div class="trigger">
			<div class="key">
				{#if recording === row}
					<span class="listening">{t('shortcuts.listening')}</span>
				{:else}
					<Keys spec={trigger.key} />
				{/if}
			</div>
			<div class="options">
				<Segmented
					value={trigger.mode}
					options={modes}
					label={t('shortcuts.mode')}
					onchange={(mode) => update(data.config.triggers.map((x, i) => (i === row ? { ...x, mode } : x)))}
				/>
				<Segmented
					value={tidyOf(trigger)}
					options={tidyOptions}
					label={t('shortcuts.tidy')}
					onchange={(value) =>
						update(data.config.triggers.map((x, i) => (i === row ? { ...x, ...tidyFlags(value) } : x)))}
				/>
			</div>
			<button type="button" class="btn change" disabled={recording !== null} onclick={() => record(row)}>{t('shortcuts.change')}</button>
			<button
				type="button"
				class="remove"
				aria-label={t('shortcuts.remove')}
				disabled={data.config.triggers.length <= 1}
				onclick={() => update(data.config.triggers.filter((_, i) => i !== row))}><Icon name="x" size={12} /></button
			>
		</div>
	{/each}
	<div class="trigger add">
		<button type="button" class="btn" disabled={recording !== null} onclick={() => record(data.config.triggers.length)}>
			<Icon name="plus" size={13} /> {t('shortcuts.add')}
		</button>
		{#if message}<span class="muted message">{message}</span>{/if}
	</div>
</Group>

<style>
	.page {
		margin: 4px 0 20px;
	}
	h1 {
		font-size: var(--title-size);
		font-weight: 650;
		margin: 0 0 4px;
	}
	.page p {
		margin: 0;
		font-size: 12.5px;
	}
	/* Fixed places, so nothing wraps on its own: narrow = keys, Ändern… and × on top, mode and tidying
	   underneath; wide = one line. Same columns in every row, so the controls line up. */
	.trigger {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto auto;
		grid-template-areas:
			'key change remove'
			'options options options';
		align-items: center;
		gap: 10px 12px;
		padding: 10px 10px 10px 14px;
		min-height: 48px;
	}
	.trigger.add {
		display: flex;
		gap: 12px;
	}
	.key {
		grid-area: key;
		min-width: 0;
	}
	.options {
		grid-area: options;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px 18px;
	}
	.change {
		grid-area: change;
	}
	@media (min-width: 1100px) {
		.trigger {
			grid-template-columns: minmax(0, 1fr) auto auto auto;
			grid-template-areas: 'key options change remove';
		}
	}
	.listening {
		color: var(--accent);
		font-weight: 500;
	}
	.remove {
		grid-area: remove;
		width: 24px;
		height: 24px;
		display: grid;
		place-items: center;
		border: 0;
		border-radius: 50%;
		background: transparent;
		color: var(--muted);
	}
	.remove:hover:not(:disabled) {
		background: color-mix(in srgb, var(--text) 10%, transparent);
	}
	.remove:disabled {
		opacity: 0.3;
	}
	.add .btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.message {
		font-size: 12px;
	}
</style>
