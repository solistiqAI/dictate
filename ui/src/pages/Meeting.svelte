<script lang="ts">
	import Group from '../components/Group.svelte';
	import Icon from '../components/Icon.svelte';
	import Row from '../components/Row.svelte';
	import Segmented from '../components/Segmented.svelte';
	import Toggle from '../components/Toggle.svelte';
	import { send } from '../lib/bridge.svelte.ts';
	import { setConfig } from '../lib/config.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { Meeting, State } from '../lib/protocol.ts';
	import { clock } from '../lib/now.svelte.ts';
	import { elapsed } from '../lib/time.ts';

	let { data, meeting }: { data: State; meeting: Meeting | null } = $props();

	const MODELS = [
		{ value: 'sonnet', label: 'Sonnet' },
		{ value: 'haiku', label: 'Haiku' },
		{ value: 'opus', label: 'Opus' }
	];

	const now = $derived(clock.now);

	const running = $derived(meeting?.running ?? false);
	const answers = $derived(meeting?.cards.filter((c) => !c.pending).length ?? 0);

	// The chosen microphone stays in the list while it is unplugged (the Souffleur then takes the default).
	const chosenMissing = $derived(
		data.config.meeting_microphone !== '' && !data.microphones.includes(data.config.meeting_microphone)
	);

	// Typed locally, saved when the field is left.
	let instructions = $derived(data.config.meeting_instructions);

	function saveInstructions() {
		if (instructions.trim() !== data.config.meeting_instructions.trim())
			setConfig(data.config, { meeting_instructions: instructions.trim() });
	}
</script>

<header class="page">
	<h1>{t('meeting.title')}</h1>
	<p class="muted">{t('meeting.intro')}</p>
</header>

<Group footer={t('meeting.footer')}>
	<div class="hero" class:running>
		<div class="state">
			{#if running && meeting?.started}
				<div class="headline">
					<i class="live"></i>{t('meeting.running', { time: elapsed(now - meeting.started) })}
				</div>
				<div class="muted small">
					{t('meeting.transcribed', {
						answers: answers === 1 ? t('meeting.answers.one') : t('meeting.answers', { n: answers }),
						sentences:
							meeting.lines.length === 1
								? t('meeting.sentences.one')
								: t('meeting.sentences', { n: meeting.lines.length })
					})}
				</div>
			{:else}
				<div class="headline">{t('meeting.ready')}</div>
				<div class="muted small">{t('meeting.ready.hint')}</div>
			{/if}
		</div>
		<button type="button" class="btn panel-btn" onclick={() => send({ type: 'open_panel' })}>
			<Icon name="panel" size={14} /> {t('meeting.show_panel')}
		</button>
		{#if running}
			<button type="button" class="btn big stop" onclick={() => send({ type: 'stop_meeting' })}>
				<i class="square"></i>{t('common.stop')}
			</button>
		{:else}
			<button type="button" class="btn primary big" onclick={() => send({ type: 'start_meeting' })}>
				{t('meeting.start')}
			</button>
		{/if}
	</div>
</Group>

<Group title={t('meeting.listen')}>
	<Row label={t('meeting.mic')} hint={chosenMissing ? t('meeting.mic.missing') : t('meeting.mic.hint')}>
		<select
			class="btn mic"
			aria-label={t('meeting.mic.label')}
			value={data.config.meeting_microphone}
			onchange={(e) => setConfig(data.config, { meeting_microphone: e.currentTarget.value })}
		>
			<option value=""
				>{t('meeting.mic.default')}{data.microphones[0] ? ` (${data.microphones[0]})` : ''}</option
			>
			{#each data.microphones as name (name)}
				<option value={name}>{name}</option>
			{/each}
			{#if chosenMissing}
				<option value={data.config.meeting_microphone}
					>{t('common.not_connected', { name: data.config.meeting_microphone })}</option
				>
			{/if}
		</select>
	</Row>
	<Row label={t('meeting.system')} hint={t('meeting.system.hint')}>
		<Toggle
			label={t('meeting.system')}
			checked={data.config.meeting_system_audio}
			onchange={(v) => setConfig(data.config, { meeting_system_audio: v })}
		/>
	</Row>
</Group>

<Group title={t('meeting.agent')}>
	<Row label={t('meeting.model')} hint={t('meeting.model.hint')}>
		<Segmented
			value={data.config.meeting_model}
			options={MODELS}
			label={t('meeting.model')}
			onchange={(meeting_model) => setConfig(data.config, { meeting_model })}
		/>
	</Row>
	<div class="instructions">
		<label for="meeting-instructions">{t('meeting.instructions')}</label>
		<div class="hint">{t('meeting.instructions.hint')}</div>
		<textarea
			id="meeting-instructions"
			class="field"
			rows="3"
			placeholder={t('meeting.instructions.placeholder')}
			value={instructions}
			oninput={(e) => (instructions = e.currentTarget.value)}
			onblur={saveInstructions}
		></textarea>
	</div>
</Group>

<Group>
	<Row label={t('meeting.transcripts')} hint={t('meeting.transcripts.hint')}>
		<button type="button" class="btn icon-btn" onclick={() => send({ type: 'open_meetings' })}>
			<Icon name="folder" size={14} /> {t('common.open')}
		</button>
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
	.hero {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 16px 16px 16px 18px;
	}
	.state {
		flex: 1;
		min-width: 0;
	}
	.headline {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 15px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
		margin-bottom: 2px;
	}
	.live {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--err);
		animation: pulse 2s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			opacity: 0.35;
		}
	}
	.small {
		font-size: 12px;
	}
	.big {
		height: 32px;
		padding: 0 18px;
		font-size: 13px;
	}
	.panel-btn,
	.icon-btn,
	.stop {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.panel-btn {
		height: 32px;
	}
	.square {
		width: 9px;
		height: 9px;
		border-radius: 2px;
		background: var(--err);
	}
	.mic {
		max-width: 260px;
		text-overflow: ellipsis;
	}
	.instructions {
		padding: var(--row-pad);
	}
	.instructions .hint {
		font-size: 12px;
		color: var(--muted);
		margin: 1px 0 8px;
	}
	textarea {
		display: block;
		width: 100%;
		height: auto;
		min-height: 64px;
		padding: 7px 9px;
		line-height: 1.4;
		resize: vertical;
	}
	@media (prefers-reduced-motion: reduce) {
		.live {
			animation: none;
		}
	}
</style>
