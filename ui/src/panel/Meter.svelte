<script lang="ts">
	// A small live level: -60 dBFS = empty, -10 dBFS = full (speech sits around -35…-20).
	import { t } from '../lib/i18n.svelte.ts';

	let { label, who, db, off = false }: { label: string; who: 'me' | 'others'; db: number | null; off?: boolean } =
		$props();

	const fill = $derived(off || db === null ? 0 : Math.max(0, Math.min(1, (db + 60) / 50)));
</script>

<span class="meter {who}" class:off title={off ? t('panel.meter.off', { label }) : label}>
	<span class="label">{label}</span>
	<span class="bar" aria-hidden="true"><span style:transform="scaleX({fill})"></span></span>
</span>

<style>
	.meter {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
		color: var(--muted);
	}
	.bar {
		width: 34px;
		height: 4px;
		border-radius: 2px;
		background: var(--surface-2);
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 100%;
		transform-origin: left;
		transition: transform 0.12s linear;
	}
	.me .bar span {
		background: var(--me);
	}
	.others .bar span {
		background: var(--others);
	}
	.off {
		opacity: 0.45;
	}
	.off .label {
		text-decoration: line-through;
		text-decoration-color: color-mix(in srgb, var(--muted) 60%, transparent);
	}
</style>
