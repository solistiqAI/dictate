<script lang="ts">
	import Icon from './Icon.svelte';
	import { send } from '../lib/bridge.svelte.ts';
	import { t } from '../lib/i18n.svelte.ts';
	import type { State } from '../lib/protocol.ts';

	let { data }: { data: State } = $props();

	// The chosen text model is Gemma but not downloaded while a shortcut tidies: Apple's model does
	// it instead (slower, catches fewer self-corrections), off the Mac nothing does (app.rs).
	const chosen = $derived(data.correct_models.find((m) => m.name === data.config.correct_model));
	const missing = $derived(
		chosen !== undefined &&
			chosen.name !== 'apple' &&
			!chosen.downloaded &&
			data.config.triggers.some((trigger) => trigger.format || trigger.correct)
	);
</script>

{#if missing && chosen}
	<div class="banner" role="note">
		<Icon name="warn" size={16} />
		<div class="what">
			<strong>{t('gemma_missing.title')}</strong>
			<p>{t('gemma_missing.hint')}</p>
		</div>
		<button type="button" class="btn" onclick={() => send({ type: 'download_model', name: chosen.name })}
			>{t('gemma_missing.download')}</button
		>
	</div>
{/if}

<style>
	.banner {
		display: flex;
		gap: 10px;
		align-items: center;
		margin: 0 0 18px;
		padding: 10px 12px 10px 14px;
		border-radius: var(--radius-group);
		background: var(--accent-soft);
		border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
		font-size: 12px;
		color: var(--text);
	}
	.banner :global(svg) {
		flex: none;
		align-self: flex-start;
		margin-top: 1px;
		color: var(--accent);
	}
	.what {
		flex: 1;
		min-width: 0;
	}
	p {
		margin: 2px 0 0;
		color: var(--muted);
	}
	.btn {
		flex: none;
	}
</style>
