<script lang="ts">
	import Icon from './Icon.svelte';
	import Mark from './Mark.svelte';
	import type { IconName } from '../lib/icons.ts';
	import { app, send } from '../lib/bridge.svelte.ts';
	import { i18n, t } from '../lib/i18n.svelte.ts';
	import type { Page } from '../lib/pages.ts';

	let { page = $bindable() }: { page: Page } = $props();

	const pages = $derived<{ id: Page; label: string; icon: IconName }[]>([
		{ id: 'history', label: t('nav.history'), icon: 'history' },
		{ id: 'meeting', label: t('nav.meeting'), icon: 'prompt' },
		{ id: 'dictionary', label: t('nav.dictionary'), icon: 'book' },
		{ id: 'shortcuts', label: t('nav.shortcuts'), icon: 'keyboard' },
		{ id: 'recognition', label: t('nav.recognition'), icon: 'wave' },
		{ id: 'general', label: t('nav.general'), icon: 'gear' }
	]);

	const current = $derived(app.state);
	const missing = $derived(
		current
			? [
					current.accessibility !== 'granted' && ('accessibility' as const),
					current.microphone === 'missing' && ('microphone' as const)
				].filter((x) => x !== false)
			: []
	);
	// Where "Freigeben…" opens something: the Mac's privacy panes, the microphone page of Windows'
	// Settings. Linux has neither (the hint says what to do instead).
	const canOpen = (what: 'accessibility' | 'microphone') =>
		i18n.platform === 'macos' || (i18n.platform === 'windows' && what === 'microphone');
</script>

<aside>
	<div class="titlebar" data-drag>
		<!-- In a browser (mock) stand-ins for the window buttons, so the page looks like the window. -->
		{#if !window.ipc}<span class="lights" aria-hidden="true"><i></i><i></i><i></i></span>{/if}
	</div>
	<div class="wordmark" data-drag>
		<span class="logo"><Mark /></span>dictate{#if current?.profile} <span>/ {current.profile}</span>{/if}
	</div>
	<nav aria-label={t('nav.label')}>
		{#each pages as item (item.id)}
			<button
				type="button"
				class:active={page === item.id}
				aria-current={page === item.id ? 'page' : undefined}
				onclick={() => (page = item.id)}
			>
				<Icon name={item.icon} />
				<span>{item.label}</span>
				{#if item.id === 'meeting' && app.meeting?.running}<i class="live" title={t('nav.live')}></i>{/if}
			</button>
		{/each}
	</nav>
	<div class="status" data-status={current?.status ?? 'loading'}>
		<span class="dot"></span>{current?.status_text ?? t('sidebar.connecting')}
	</div>
	{#if missing.length}
		<div class="permission" role="alert">
			<div class="warn"><Icon name="warn" size={14} /> {t('sidebar.permission')}</div>
			{#each missing as what (what)}
				<p>
					{what === 'accessibility' ? t('sidebar.accessibility') : t('sidebar.microphone')}
				</p>
				{#if canOpen(what)}
					<button type="button" class="btn primary" onclick={() => send({ type: 'open_privacy', what })}>
						{t('sidebar.grant')}
					</button>
				{/if}
			{/each}
		</div>
	{/if}
</aside>

<style>
	aside {
		width: 216px;
		flex: none;
		display: flex;
		flex-direction: column;
		background: var(--sidebar);
		color: var(--sidebar-text);
		border-right: 1px solid var(--sidebar-border);
		padding: 0 10px 12px;
	}
	.titlebar {
		height: 38px;
		flex: none;
		display: flex;
		align-items: center;
		padding-left: 4px;
	}
	.lights {
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
	/* "<mark> dictate / <profile name>": the name is the profile's. */
	.wordmark {
		display: flex;
		align-items: baseline;
		gap: 5px;
		padding: 0 9px 16px;
		font-size: 21px;
		font-weight: 650;
		letter-spacing: -1px;
		line-height: 1;
	}
	/* The SOLISTIQ mark of the app icon (a trademark, NOTICE.md). */
	.wordmark .logo {
		align-self: center;
		display: flex;
		height: 24px;
		margin-right: 3px;
	}
	.wordmark span:not(.logo) {
		font-size: 10px;
		letter-spacing: 1px;
		font-weight: 500;
		color: var(--sidebar-muted);
	}
	.status {
		margin-top: auto;
		padding: 10px 9px 2px;
		font-size: 11.5px;
		color: var(--sidebar-muted);
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ok);
	}
	[data-status='recording'] .dot {
		background: var(--err);
	}
	[data-status='problem'] .dot {
		background: var(--warn);
	}
	[data-status='loading'] .dot,
	[data-status='transcribing'] .dot {
		background: var(--muted);
	}
	nav {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	nav button {
		display: flex;
		align-items: center;
		gap: 9px;
		height: 30px;
		padding: 0 9px;
		border: 0;
		border-radius: var(--radius);
		background: transparent;
		color: var(--sidebar-text);
		text-align: left;
	}
	nav button :global(svg) {
		color: var(--accent);
	}
	nav button:hover:not(.active) {
		background: color-mix(in srgb, var(--sidebar-text) 7%, transparent);
	}
	nav button.active {
		background: var(--sidebar-active);
		color: var(--sidebar-active-text);
	}
	nav button.active :global(svg) {
		color: inherit;
	}
	.live {
		margin-left: auto;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--err);
	}
	.permission {
		margin-top: 8px;
		padding: 12px;
		border-radius: var(--radius-group);
		background: color-mix(in srgb, var(--warn) 14%, var(--sidebar));
		border: 1px solid color-mix(in srgb, var(--warn) 35%, transparent);
		font-size: 12px;
	}
	.warn {
		display: flex;
		align-items: center;
		gap: 6px;
		font-weight: 600;
		color: var(--warn);
	}
	.permission p {
		margin: 6px 0 8px;
		color: var(--sidebar-text);
	}
	.permission .btn {
		width: 100%;
	}
</style>
