<script>
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import { theme, toggleLightMode } from '$lib/theme.svelte.js';

	const i18n = useI18n();
	const label = $derived(theme.light ? i18n.t('theme_dark') : i18n.t('theme_light'));
</script>

<button type="button" class="toggle" onclick={toggleLightMode} aria-label={label} title={label}>
	{#if theme.light}
		<svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
			<path d="M21 14.5A8.5 8.5 0 0 1 9.5 3 7 7 0 1 0 21 14.5Z" stroke="currentColor" stroke-width="1.75" stroke-linejoin="round" />
		</svg>
	{:else}
		<svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
			<circle cx="12" cy="12" r="4" stroke="currentColor" stroke-width="1.75" />
			<path
				d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"
				stroke="currentColor"
				stroke-width="1.75"
			/>
		</svg>
	{/if}
</button>

<style>
	.toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2.5rem;
		height: 2.5rem;
		padding: 0;
		border: 0;
		background: var(--paper-2);
		color: var(--ink);
		cursor: pointer;
		transition:
			background-color var(--dur-ui) var(--ease-out),
			transform var(--dur-press) var(--ease-out);
	}

	.toggle:active {
		transform: scale(0.94);
	}

	@media (hover: hover) and (pointer: fine) {
		.toggle:hover {
			background: var(--paper-3);
		}
	}
</style>
