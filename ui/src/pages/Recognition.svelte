<script lang="ts">
	import Group from '../components/Group.svelte';
	import Row from '../components/Row.svelte';
	import Segmented from '../components/Segmented.svelte';
	import Toggle from '../components/Toggle.svelte';
	import { on, send } from '../lib/bridge.svelte.ts';
	import { setConfig } from '../lib/config.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { State } from '../lib/protocol.ts';

	let { data }: { data: State } = $props();

	const languages = $derived([
		{ value: 'auto', label: t('recognition.language.auto') },
		{ value: 'de', label: t('recognition.language.de') },
		{ value: 'en', label: t('recognition.language.en') }
	]);

	let level = $state<number | null>(null);
	let testing = $state(false);

	function test() {
		testing = true;
		send({ type: 'mic_test' });
	}

	$effect(() =>
		on((event) => {
			if (event.type !== 'mic_level') return;
			level = event.db;
			if (event.done) testing = false;
		})
	);

	// The chosen model, or Apple's while the chosen one is not downloaded (what Dictate then uses).
	const correcting = $derived(
		data.correct_models.find((m) => m.name === data.config.correct_model)?.downloaded ? data.config.correct_model : 'apple'
	);

	// The chosen microphone stays in the list while it is unplugged (recording falls back to the default).
	const chosenMissing = $derived(data.config.microphone !== '' && !data.microphones.includes(data.config.microphone));

	// -60 dBFS = empty, -10 dBFS = full; speech sits around -35…-20.
	const fill = $derived(level === null ? 0 : Math.max(0, Math.min(1, (level + 60) / 50)));
</script>

<header class="page">
	<h1>{t('recognition.title')}</h1>
	<p class="muted">{t('recognition.intro')}</p>
</header>

<Group title={t('recognition.model')}>
	{#each data.models as model (model.name)}
		{@const active = data.config.model === model.name}
		<label class="model" class:active>
			<input
				type="radio"
				name="model"
				checked={active}
				disabled={!model.downloaded}
				onchange={() => setConfig(data.config, { model: model.name })}
			/>
			<div class="what">
				<div>{model.label}</div>
				<div class="muted small">{model.size}</div>
			</div>
			{#if model.progress !== null}
				<div class="progress" aria-label={t('recognition.downloading')}>
					<div style:width="{Math.round(model.progress * 100)}%"></div>
				</div>
				<span class="muted small">{t('common.percent', { n: Math.round(model.progress * 100) })}</span>
			{:else if !model.downloaded}
				<button type="button" class="btn" onclick={() => send({ type: 'download_model', name: model.name })}>{t('recognition.download')}</button>
			{:else if active}
				<span class="badge">{t('recognition.active')}</span>
			{/if}
		</label>
	{/each}
</Group>

<Group
	title={t('recognition.tidy')}
	footer={t('recognition.tidy.footer')}
>
	{#each data.correct_models as model (model.name)}
		{@const active = correcting === model.name}
		<label class="model" class:active>
			<input
				type="radio"
				name="correct_model"
				checked={active}
				disabled={!model.downloaded}
				onchange={() => setConfig(data.config, { correct_model: model.name })}
			/>
			<div class="what">
				<div>{model.label}</div>
				<div class="muted small">{model.size}</div>
			</div>
			{#if !model.downloaded}
				<button type="button" class="btn" onclick={() => send({ type: 'download_model', name: model.name })}>{t('recognition.download')}</button>
			{:else if active}
				<span class="badge">{t('recognition.active')}</span>
			{/if}
		</label>
	{/each}
</Group>

<Group>
	<Row label={t('recognition.language')} hint={t('recognition.language.hint')}>
		<Segmented
			value={data.config.language}
			options={languages}
			label={t('recognition.language')}
			onchange={(language) => setConfig(data.config, { language })}
		/>
	</Row>
	<Row label={t('recognition.boost')} hint={t('recognition.boost.hint')}>
		<Toggle
			label={t('recognition.boost.label')}
			checked={data.config.boost_vocabulary}
			onchange={(v) => setConfig(data.config, { boost_vocabulary: v })}
		/>
	</Row>
</Group>

<Group title={t('recognition.microphone')}>
	<Row
		label={t('recognition.record_with')}
		hint={chosenMissing ? t('recognition.mic.missing') : t('recognition.mic.hint')}
	>
		<select
			class="btn mic"
			aria-label={t('recognition.microphone')}
			value={data.config.microphone}
			onchange={(e) => setConfig(data.config, { microphone: e.currentTarget.value })}
		>
			<option value=""
				>{t('recognition.mic.default')}{data.microphones[0] ? ` (${data.microphones[0]})` : ''}</option
			>
			{#each data.microphones as name (name)}
				<option value={name}>{name}</option>
			{/each}
			{#if chosenMissing}
				<option value={data.config.microphone}>{t('common.not_connected', { name: data.config.microphone })}</option>
			{/if}
		</select>
	</Row>
	<Row
		label={data.microphone_name}
		hint={data.microphone === 'granted' ? t('recognition.mic.granted') : t('recognition.mic.denied')}
	>
		<div class="meter" aria-hidden="true"><div style:width="{fill * 100}%"></div></div>
		<button type="button" class="btn" disabled={testing} onclick={test}>{testing ? t('recognition.mic.speak') : t('recognition.mic.test')}</button>
	</Row>
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
	.model {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 14px;
		min-height: 50px;
	}
	.model input {
		accent-color: var(--accent);
		margin: 0;
	}
	.what {
		flex: 1;
	}
	.small {
		font-size: 12px;
	}
	.badge {
		font-size: 11px;
		font-weight: 600;
		padding: 2px 8px;
		border-radius: 10px;
		background: var(--accent-soft);
		color: var(--accent);
	}
	.progress,
	.meter {
		width: 120px;
		height: 6px;
		border-radius: 3px;
		background: var(--surface-2);
		overflow: hidden;
	}
	.mic {
		max-width: 260px;
		text-overflow: ellipsis;
	}
	.progress div,
	.meter div {
		height: 100%;
		background: var(--accent);
		transition: width 0.1s;
	}
</style>
