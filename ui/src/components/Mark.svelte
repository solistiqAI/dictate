<script lang="ts">
	// The SOLISTIQ mark from the app icon (assets/app-icon.svg), without its tile, in front of the
	// wordmark: light faces on the dark sidebar, navy ones on the light one (--mark-* in app.css).
	// A trademark, not under the MIT licence (NOTICE.md): keep its polygons intact. Decorative.
	const id = $props.id();
</script>

<svg class="mark" viewBox="245 183 310 434" aria-hidden="true">
	<defs>
		<linearGradient id="{id}-face" x1="250" x2="550" gradientUnits="userSpaceOnUse">
			<stop style="stop-color: var(--mark-face-0)" />
			<stop offset=".14" style="stop-color: var(--mark-face-1)" />
			<stop offset=".41" style="stop-color: var(--mark-face-2)" />
			<stop offset=".79" style="stop-color: var(--mark-face-3)" />
			<stop offset="1" style="stop-color: var(--mark-face-4)" />
		</linearGradient>
	</defs>
	<path
		style="fill: var(--mark-side)"
		d="M351.55 612.13 250 557.61V319.49L351.55 374zM549.99 602.39l-101.55-54.51V426.01l101.55 54.51zM550 420.1l-101.56-54.51V187.87L550 242.39z"
	/>
	<path
		fill="url(#{id}-face)"
		d="m250 557.61 101.55 54.52 198.44-131.6-101.55-54.52zM250 319.49 351.55 374 550 242.39l-101.56-54.52z"
	/>
</svg>

<style>
	.mark {
		display: block;
		flex: none;
		width: auto;
		height: 100%;
	}
</style>
