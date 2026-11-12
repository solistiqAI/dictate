<script lang="ts">
	// The Souffleur's side panel (index.html?view=panel): its own narrow window next to the call.
	import Icon from '../components/Icon.svelte';
	import Card from './Card.svelte';
	import Highlights from './Highlights.svelte';
	import Meter from './Meter.svelte';
	import Transcript from './Transcript.svelte';
	import { send } from '../lib/bridge.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { AgentPhase, Meeting } from '../lib/protocol.ts';
	import { clock } from '../lib/now.svelte.ts';
	import { elapsed } from '../lib/time.ts';

	let { meeting }: { meeting: Meeting | null } = $props();

	const now = $derived(clock.now);

	// Cards that come in while the panel is open slide in; the ones there at the start do not.
	const opened = Date.now() / 1000;

	const running = $derived(meeting?.running ?? false);
	const status = $derived(meeting?.status);
	// "Beenden" answers at once: the app needs a moment for the last words and the final key
	// facts, the panel says so from the click on (`stopFor`: the meeting the click was for).
	let stopFor = $state<number | null>(null);
	const ending = $derived(running && (!!status?.ending || (stopFor !== null && stopFor === meeting?.started)));

	function stop() {
		stopFor = meeting?.started ?? null;
		send({ type: 'stop_meeting' });
	}

	function resume() {
		stopFor = null;
		send({ type: 'resume_meeting' });
	}
	const cards = $derived(meeting ? [...meeting.cards].reverse() : []);
	// Raised: the newest answer, and questions still being looked up above it.
	const lead = $derived(cards.findIndex((c) => !c.pending));
	// The status's activity belongs to the newest card being worked on.
	const working = $derived(cards.findIndex((c) => c.pending));
	const lines = $derived(meeting?.lines ?? []);

	const phases = $derived<Record<AgentPhase, string>>({
		off: t('panel.phase.off'),
		starting: t('panel.phase.starting'),
		listening: t('panel.phase.listening'),
		thinking: t('panel.phase.thinking'),
		problem: t('panel.phase.problem')
	});
	const phase = $derived<AgentPhase>(status?.agent ?? 'off');
	const said = $derived(
		phase === 'problem'
			? status?.problem || phases.problem
			: status?.activity
				? t('panel.activity', { activity: status.activity })
				: ending
					? t('panel.ending')
					: phases[phase]
	);
	// A problem the agent did not report itself (e.g. the microphone) shows under the status.
	const problem = $derived(phase !== 'problem' ? (status?.problem ?? '') : '');
	const systemOff = $derived(!status || status.system_audio === 'off');

	let showTranscript = $state(true);
	let showHighlights = $state(true);
	const highlights = $derived(meeting?.highlights ?? []);
	let question = $state('');

	// Enter in the field sends the question.
	function submit(event: SubmitEvent) {
		event.preventDefault();
		const text = question.trim();
		if (!text || !running) return;
		send({ type: 'ask_meeting', text });
		question = '';
	}
</script>

<div class="panel">
	<header>
		<div class="top">
			<div class="title" data-drag>
				<!-- The window keeps macOS's buttons top left; in a browser (mock) stand-ins. -->
				{#if !window.ipc}<span class="lights" aria-hidden="true"><i></i><i></i><i></i></span>{/if}
				<span class="name" data-drag>Souffleur</span>
				{#if running && meeting?.started}
					<span class="elapsed" data-drag>{elapsed(now - meeting.started)}</span>
				{:else if meeting?.started}
					<span class="elapsed" data-drag>{t('panel.ended')}</span>
				{/if}
			</div>
			{#if ending}
				<button type="button" class="btn stop" disabled aria-busy="true">
					<i class="spinner" aria-hidden="true"></i>{t('panel.ending')}
				</button>
			{:else if running}
				<button type="button" class="btn stop" onclick={stop}>
					<i class="square"></i>{t('common.stop')}
				</button>
			{:else if meeting?.resumable}
				<button type="button" class="btn primary" title={t('panel.resume.title')} onclick={resume}
					>{t('panel.resume')}</button
				>
				<button
					type="button"
					class="btn"
					title={t('panel.new.title')}
					onclick={() => send({ type: 'start_meeting' })}>{t('panel.new')}</button
				>
			{:else if cards.length}
				<button type="button" class="btn primary" onclick={() => send({ type: 'start_meeting' })}
					>{t('panel.start')}</button
				>
			{/if}
			<button
				type="button"
				class="icon"
				title={t('common.settings')}
				aria-label={t('common.settings')}
				onclick={() => send({ type: 'open_page', page: 'meeting' })}><Icon name="gear" size={15} /></button
			>
		</div>
		{#if running}
			<div class="status" data-drag>
				<span class="agent" data-phase={phase} data-drag>
					<i class="dot"></i><span class="said" data-drag>{said}</span>
				</span>
				{#if !ending}
					<Meter label={t('panel.me')} who="me" db={status?.mic_db ?? null} />
					<Meter label={t('panel.others')} who="others" db={status?.system_db ?? null} off={systemOff} />
				{/if}
			</div>
			{#if status?.system_audio === 'silent'}
				<p class="hint warn" title={t('panel.silent.title')}>
					{t('panel.silent')} <span class="muted">{t('panel.silent.where')}</span>
				</p>
			{:else if status?.system_audio === 'problem'}
				<p class="hint err">{t('panel.system_problem')}</p>
			{/if}
			{#if problem}<p class="hint err">{problem}</p>{/if}
		{/if}
	</header>

	{#if highlights.length}
		<Highlights items={highlights} bind:open={showHighlights} />
	{/if}

	<div class="cards">
		{#each cards as card, i (card.id)}
			<Card
				{card}
				{now}
				newest={lead === -1 || i <= lead}
				fresh={card.at > opened}
				activity={i === working ? (status?.activity ?? '') : ''}
			/>
		{:else}
			<div class="empty">
				{#if running && ending}
					<p>{t('panel.ending')}</p>
				{:else if running}
					<div class="ear" aria-hidden="true"><i></i><i></i><i></i></div>
					<p>{t('panel.listening')}</p>
					<p class="muted">{t('panel.ask_below')}</p>
				{:else}
					<p>{t('panel.idle')}</p>
					<p class="muted">{t('panel.idle.hint')}</p>
					{#if meeting?.resumable}
						<button type="button" class="btn primary big" title={t('panel.resume.title')} onclick={resume}
							>{t('panel.resume')}</button
						>
					{:else}
						<button type="button" class="btn primary big" onclick={() => send({ type: 'start_meeting' })}
							>{t('panel.start')}</button
						>
					{/if}
				{/if}
			</div>
		{/each}
	</div>

	{#if running || lines.length}
		<Transcript {lines} bind:open={showTranscript} />
	{/if}

	<form class="ask" onsubmit={submit}>
		<input
			class="field"
			type="text"
			placeholder={running ? t('panel.ask.placeholder') : t('panel.ask.off')}
			aria-label={t('panel.ask')}
			disabled={!running}
			bind:value={question}
		/>
		<button
			type="submit"
			class="send"
			title={t('panel.ask.send')}
			aria-label={t('panel.ask.send')}
			disabled={!running || !question.trim()}
		>
			<Icon name="send" size={14} />
		</button>
	</form>
</div>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg);
	}
	header {
		flex: none;
		padding: 0 12px 8px;
		border-bottom: 1px solid var(--separator);
	}
	/* Level with macOS's window buttons (centred 15 px from the top, 68 px wide with margin);
	   the buttons stay 22 px tall so they keep air to the window's top edge. */
	.top {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 30px;
		padding-left: 66px;
		position: relative;
	}
	.lights {
		position: absolute;
		left: 8px;
		top: 9px;
		display: flex;
		gap: 8px;
	}
	.lights i {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: #ff5f57;
	}
	.lights i:nth-child(2) {
		background: #febc2e;
	}
	.lights i:nth-child(3) {
		background: #28c840;
	}
	.title {
		flex: 1;
		min-width: 0;
		align-self: stretch;
		display: flex;
		align-items: center;
		gap: 8px;
		padding-left: 2px;
	}
	.name {
		font-size: 15px;
		font-weight: 650;
		letter-spacing: -0.3px;
	}
	.elapsed {
		font-size: 12px;
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.btn {
		height: 22px;
		padding: 0 10px;
		font-size: 12px;
	}
	.stop {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.square {
		width: 8px;
		height: 8px;
		border-radius: 2px;
		background: var(--err);
	}
	.stop:disabled {
		opacity: 1;
		color: var(--muted);
		cursor: default;
	}
	.spinner {
		width: 9px;
		height: 9px;
		border: 1.5px solid currentColor;
		border-right-color: transparent;
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	.icon {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: var(--radius);
		background: transparent;
		color: var(--muted);
	}
	.icon:hover {
		background: var(--surface-2);
		color: var(--text);
	}
	.status {
		display: flex;
		align-items: center;
		gap: 12px;
		height: 20px;
	}
	.agent {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 12px;
	}
	.said {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dot {
		flex: none;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--muted);
	}
	[data-phase='listening'] .dot {
		background: var(--ok);
		animation: pulse 2.4s ease-in-out infinite;
	}
	[data-phase='thinking'] .dot,
	[data-phase='starting'] .dot {
		background: var(--accent);
		animation: pulse 0.9s ease-in-out infinite;
	}
	[data-phase='thinking'] .said {
		color: var(--accent);
		font-weight: 500;
	}
	[data-phase='problem'] .dot {
		background: var(--err);
	}
	[data-phase='problem'] .said {
		color: var(--err);
	}
	@keyframes pulse {
		50% {
			opacity: 0.3;
			transform: scale(0.75);
		}
	}
	.hint {
		margin: 6px 0 0;
		font-size: 11.5px;
		line-height: 1.35;
	}
	.warn {
		color: var(--warn);
	}
	.err {
		color: var(--err);
	}
	.cards {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding-top: 8px;
	}
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 100%;
		padding: 24px 32px 40px;
		text-align: center;
		font-size: 12.5px;
	}
	.empty p {
		margin: 0;
		max-width: 270px;
	}
	.empty p + p {
		margin-top: 6px;
		font-size: 12px;
	}
	.big {
		margin-top: 16px;
		height: 30px;
		padding: 0 22px;
		font-size: 13px;
	}
	/* Three bars breathing: listening, nothing to say yet. */
	.ear {
		display: flex;
		align-items: center;
		gap: 3px;
		height: 22px;
		margin-bottom: 12px;
	}
	.ear i {
		width: 3px;
		height: 100%;
		border-radius: 2px;
		background: color-mix(in srgb, var(--accent) 55%, transparent);
		animation: breathe 1.8s ease-in-out infinite;
	}
	.ear i:nth-child(2) {
		animation-delay: 0.3s;
	}
	.ear i:nth-child(3) {
		animation-delay: 0.6s;
	}
	@keyframes breathe {
		0%,
		100% {
			transform: scaleY(0.35);
		}
		50% {
			transform: scaleY(1);
		}
	}
	.ask {
		flex: none;
		position: relative;
		padding: 8px 10px 10px;
		border-top: 1px solid var(--separator);
	}
	.ask .field {
		width: 100%;
		height: 32px;
		padding-right: 36px;
		border-radius: 8px;
		background: var(--surface);
	}
	.ask .field:disabled {
		opacity: 0.6;
	}
	.send {
		position: absolute;
		right: 15px;
		top: 13px;
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 6px;
		background: var(--accent);
		color: var(--accent-text);
	}
	.send:disabled {
		background: var(--surface-2);
		color: var(--muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.dot,
		.ear i,
		.spinner {
			animation: none !important;
		}
	}
</style>
