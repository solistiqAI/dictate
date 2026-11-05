<script lang="ts" generics="T extends string">
	let {
		value = $bindable(),
		options,
		label,
		onchange
	}: {
		value: T;
		options: { value: T; label: string }[];
		label: string;
		onchange?: (value: T) => void;
	} = $props();
</script>

<div class="segmented" role="radiogroup" aria-label={label}>
	{#each options as option (option.value)}
		<button
			type="button"
			role="radio"
			aria-checked={value === option.value}
			class:on={value === option.value}
			onclick={() => {
				value = option.value;
				onchange?.(option.value);
			}}>{option.label}</button
		>
	{/each}
</div>

<style>
	.segmented {
		display: inline-flex;
		padding: 2px;
		gap: 2px;
		border-radius: var(--radius);
		background: var(--surface-2);
		box-shadow: inset 0 0 0 1px var(--border);
	}
	button {
		border: 0;
		background: transparent;
		padding: 0 10px;
		height: 22px;
		border-radius: calc(var(--radius) - 2px);
		color: var(--muted);
	}
	button.on {
		background: var(--surface);
		color: var(--text);
		box-shadow: var(--key-shadow);
	}
</style>
