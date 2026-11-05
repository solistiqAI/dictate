<script lang="ts">
	import Group from '../components/Group.svelte';
	import Icon from '../components/Icon.svelte';
	import Row from '../components/Row.svelte';
	import Segmented from '../components/Segmented.svelte';
	import Toggle from '../components/Toggle.svelte';
	import { send } from '../lib/bridge.svelte.ts';
	import { setConfig } from '../lib/config.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { Config, Sounds, State } from '../lib/protocol.ts';

	let { data }: { data: State } = $props();

	const sounds = $derived<{ value: Sounds; label: string }[]>([
		{ value: 'aus', label: t('general.sounds.off') },
		{ value: 'leise', label: t('general.sounds.quiet') },
		{ value: 'mittel', label: t('general.sounds.medium') }
	]);
	const languages = $derived<{ value: Config['ui_language']; label: string }[]>([
		{ value: 'auto', label: t('general.ui_language.auto') },
		{ value: 'de', label: t('general.ui_language.de') },
		{ value: 'en', label: t('general.ui_language.en') }
	]);
</script>

<header class="page">
	<h1>{t('general.title')}</h1>
</header>

<Group title={t('general.recording')}>
	<Row label={t('general.sounds')}>
		<Segmented
			value={data.config.sounds}
			options={sounds}
			label={t('general.sounds.label')}
			onchange={(sounds) => setConfig(data.config, { sounds })}
		/>
	</Row>
	<Row label={t('general.pause_media')} hint={t('general.pause_media.hint')}>
		<Toggle
			label={t('general.pause_media')}
			checked={data.config.pause_media}
			onchange={(v) => setConfig(data.config, { pause_media: v })}
		/>
	</Row>
	<Row label={t('general.mute')}>
		<Toggle
			label={t('general.mute')}
			checked={data.config.mute_while_recording}
			onchange={(v) => setConfig(data.config, { mute_while_recording: v })}
		/>
	</Row>
</Group>

<Group title={t('general.text')}>
	<Row label={t('general.voice_commands')} hint={t('general.voice_commands.hint')}>
		<Toggle
			label={t('general.voice_commands')}
			checked={data.config.voice_commands}
			onchange={(v) => setConfig(data.config, { voice_commands: v })}
		/>
	</Row>
</Group>

<Group title={t('general.after_paste')}>
	<Row label={t('general.learn')} hint={t('general.learn.hint')}>
		<Toggle
			label={t('general.learn')}
			checked={data.config.learn_from_corrections}
			onchange={(v) => setConfig(data.config, { learn_from_corrections: v })}
		/>
	</Row>
</Group>

{#if data.report_target}
	<Group title={t('general.improve')}>
		<Row
			label={t('general.reports')}
			hint={`${t('general.reports.hint')}. ${t('general.reports.target', { target: data.report_target })}`}
		>
			<Toggle
				label={t('general.reports')}
				checked={data.config.share_reports}
				onchange={(v) => setConfig(data.config, { share_reports: v })}
			/>
		</Row>
	</Group>
{/if}

<Group>
	<Row label={t('general.ui_language')} hint={t('general.ui_language.hint')}>
		<Segmented
			value={data.config.ui_language}
			options={languages}
			label={t('general.ui_language')}
			onchange={(ui_language) => setConfig(data.config, { ui_language })}
		/>
	</Row>
	<Row label={t('general.bar')} hint={t('general.bar.hint')}>
		<Toggle
			label={t('general.bar')}
			checked={data.config.show_bar}
			onchange={(v) => setConfig(data.config, { show_bar: v })}
		/>
	</Row>
	<Row label={t('general.login')}>
		<Toggle
			label={t('general.login')}
			checked={data.config.start_at_login}
			onchange={(v) => setConfig(data.config, { start_at_login: v })}
		/>
	</Row>
	<Row label={t('general.folder')} hint={t('general.folder.hint')}>
		<button type="button" class="btn icon-btn" onclick={() => send({ type: 'open_folder' })}>
			<Icon name="folder" size={14} />
			{t('general.folder.open')}
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
		margin: 0;
	}
	.icon-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
</style>
