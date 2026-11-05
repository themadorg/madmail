<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
	import { routeFromHref } from '$lib/docs.js';
	import pencilSquare from '$lib/icons/pencil-square.svg?raw';

	const EDIT_BASE = 'https://github.com/themadorg/madmail/edit/main/docs';

	/** @type {{ currentHref?: string }} */
	let { currentHref = '/docs' } = $props();

	const route = $derived(routeFromHref(currentHref));
	const editUrl = $derived(`${EDIT_BASE}/${route}.md`);
</script>

<a href={editUrl} class="edit-page" target="_blank" rel="noopener noreferrer">
	<span class="icon" aria-hidden="true">{@html pencilSquare}</span>
	<span>{t("Edit")}</span>
</a>

<style>
	.edit-page {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		flex-shrink: 0;
		padding: 0.45rem 0.7rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		color: var(--color-text);
		font: inherit;
		font-size: 0.8rem;
		line-height: 1;
		text-decoration: none;
	}

	.edit-page:hover {
		background: var(--color-hover);
	}

	.icon :global(svg) {
		display: block;
		width: 0.95rem;
		height: 0.95rem;
	}
</style>
