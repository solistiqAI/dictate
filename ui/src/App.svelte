<script lang="ts">
	import Sidebar from './components/Sidebar.svelte';
	import History from './pages/History.svelte';
	import Dictionary from './pages/Dictionary.svelte';
	import Shortcuts from './pages/Shortcuts.svelte';
	import Recognition from './pages/Recognition.svelte';
	import General from './pages/General.svelte';
	import Meeting from './pages/Meeting.svelte';
	import Panel from './panel/Panel.svelte';
	import Icon from './components/Icon.svelte';
	import { t } from './lib/i18n.svelte.ts';
	import { app, view } from './lib/bridge.svelte.ts';
</script>

{#if view === 'panel'}
	<Panel meeting={app.meeting} />
	{#if app.note}
		<div class="note panel" role="status">{app.note}</div>
	{/if}
{:else}
	<div class="window">
		<Sidebar bind:page={app.page} />
		<main>
			<div class="titlebar" data-drag></div>
			{#if app.state?.no_tray}
				<div class="banner" role="note">
					<Icon name="panel" size={16} />
					<div>
						<strong>{t('window.no_tray')}</strong>
						<p>{t('window.no_tray.hint')}</p>
					</div>
				</div>
			{/if}
			{#if app.state}
				<div class="content">
					{#if app.page === 'history'}
						<History data={app.state} />
					{:else if app.page === 'meeting'}
						<Meeting data={app.state} meeting={app.meeting} />
					{:else if app.page === 'dictionary'}
						<Dictionary data={app.state} />
					{:else if app.page === 'shortcuts'}
						<Shortcuts data={app.state} />
					{:else if app.page === 'recognition'}
						<Recognition data={app.state} />
					{:else}
						<General data={app.state} />
					{/if}
				</div>
			{/if}
		</main>
		{#if app.note}
			<div class="note" role="status">{app.note}</div>
		{/if}
	</div>
{/if}

<style>
	.window {
		display: flex;
		height: 100%;
	}
	main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg);
	}
	.titlebar {
		height: 38px;
		flex: none;
	}
	/* Linux without a tray: stays above every page while the window is open. */
	.banner {
		flex: none;
		display: flex;
		gap: 10px;
		align-items: flex-start;
		margin: 0 28px 16px;
		padding: 10px 14px;
		border-radius: var(--radius-group);
		background: var(--accent-soft);
		border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
		font-size: 12px;
		color: var(--text);
	}
	.banner :global(svg) {
		flex: none;
		margin-top: 1px;
		color: var(--accent);
	}
	.banner p {
		margin: 2px 0 0;
		color: var(--muted);
	}
	.content {
		flex: 1;
		overflow-y: auto;
		padding: 0 28px 28px;
	}
	.note {
		position: fixed;
		bottom: 18px;
		left: 50%;
		transform: translateX(calc(-50% + 108px));
		padding: 7px 14px;
		border-radius: 999px;
		background: var(--text);
		color: var(--bg);
		font-size: 12px;
		font-weight: 500;
		box-shadow: 0 4px 14px rgba(0, 0, 0, 0.2);
	}
	.note.panel {
		bottom: 60px;
		transform: translateX(-50%);
	}
</style>
