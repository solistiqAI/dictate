<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		footer,
		actions,
		children
	}: { title?: string; footer?: string; actions?: Snippet; children: Snippet } = $props();
</script>

<section>
	{#if title || actions}
		<header>
			{#if title}<h2>{title}</h2>{/if}
			{#if actions}<div class="actions">{@render actions()}</div>{/if}
		</header>
	{/if}
	<div class="box">{@render children()}</div>
	{#if footer}<p class="footer">{footer}</p>{/if}
</section>

<style>
	section {
		margin-bottom: 22px;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin: 0 4px 6px;
		min-height: 26px;
	}
	h2 {
		margin: 0;
		font-size: 13px;
		font-weight: 600;
	}
	.actions {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.box {
		background: var(--surface);
		border-radius: var(--radius-group);
		box-shadow: var(--shadow-group);
		overflow: hidden;
	}
	.box > :global(* + *) {
		border-top: 1px solid var(--separator);
	}
	.footer {
		margin: 6px 4px 0;
		font-size: 12px;
		color: var(--muted);
	}
</style>
