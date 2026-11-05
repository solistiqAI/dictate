<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		label,
		hint,
		children
	}: { label: string; hint?: string; children?: Snippet } = $props();
</script>

<div class="row">
	<div class="text">
		<div>{label}</div>
		{#if hint}<div class="hint">{hint}</div>{/if}
	</div>
	{#if children}<div class="control">{@render children()}</div>{/if}
</div>

<style>
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: var(--row-pad);
		min-height: 44px;
	}
	.hint {
		font-size: 12px;
		color: var(--muted);
		margin-top: 1px;
	}
	.control {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: none;
	}
</style>
