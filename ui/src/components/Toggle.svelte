<script lang="ts">
	let {
		checked = $bindable(false),
		label,
		onchange
	}: { checked?: boolean; label: string; onchange?: (checked: boolean) => void } = $props();
</script>

<input
	type="checkbox"
	role="switch"
	aria-label={label}
	bind:checked
	onchange={() => onchange?.(checked)}
/>

<style>
	input {
		appearance: none;
		margin: 0;
		width: 34px;
		height: 20px;
		border-radius: 10px;
		background: var(--surface-2);
		box-shadow: inset 0 0 0 1px var(--border);
		position: relative;
		transition: background 0.15s;
		flex: none;
	}
	input::after {
		content: '';
		position: absolute;
		top: 2px;
		left: 2px;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
		transition: transform 0.15s;
	}
	input:checked {
		background: var(--accent);
		box-shadow: none;
	}
	input:checked::after {
		transform: translateX(14px);
	}
</style>
