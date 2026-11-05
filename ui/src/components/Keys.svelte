<script lang="ts">
	// A trigger spec ("shift+alt+cmd+5", "fn", "mouse4") as key caps, named as on this OS.
	import { t } from '../lib/i18n.svelte.ts';
	import { keyCaps } from '../lib/keys.ts';

	let { spec }: { spec: string } = $props();
</script>

<span class="keys">
	{#each keyCaps(spec, t) as cap, i (i)}<kbd>{cap}</kbd>{/each}
	{#if !spec}<span class="muted">{t('keys.off')}</span>{/if}
</span>

<style>
	.keys {
		display: inline-flex;
		gap: 4px;
	}
	kbd {
		font-family: inherit;
		font-size: 12px;
		min-width: 24px;
		height: 22px;
		padding: 0 6px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 5px;
		background: var(--key-bg);
		box-shadow: var(--key-shadow);
	}
</style>
