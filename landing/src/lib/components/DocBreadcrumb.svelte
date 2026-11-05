<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t, localize } = useSiteLanguage();
	import { labelFromRoute, resolveDocHref, routeFromHref } from '$lib/docs.js';
	import {
		ancestorTrailForDirId,
		ancestorTrailForHref,
		docTreeNodes,
		findDirById,
		hrefForDirId,
		isDirListingHref
	} from '$lib/docTreeData.js';
	import { docPages } from '$lib/nav.js';

	/** @type {{ currentHref?: string, currentTitle?: string }} */
	let { currentHref = '', currentTitle } = $props();

	const isIndex = $derived(currentHref === '/docs');
	const onDirListing = $derived(isDirListingHref(currentHref));
	const currentLabel = $derived.by(() => {
		if (currentTitle) return currentTitle;
		if (onDirListing) {
			const dir = findDirById(docTreeNodes, routeFromHref(currentHref));
			if (dir) return dir.label;
		}

		return (
			docPages.find((page) => page.href === currentHref)?.label ??
			labelFromRoute(routeFromHref(currentHref))
		);
	});
	const ancestors = $derived.by(() => {
		if (onDirListing) {
			return ancestorTrailForDirId(docTreeNodes, routeFromHref(currentHref)) ?? [];
		}

		return ancestorTrailForHref(docTreeNodes, resolveDocHref(currentHref)) ?? [];
	});

	/** @param {import('$lib/docTreeData.js').DocTreeBreadcrumb} ancestor */
	function hrefForAncestor(ancestor) {
		return hrefForDirId(ancestor.id);
	}
</script>

<nav class="breadcrumb" aria-label={t("Breadcrumb")}>
	{#if isIndex}
		<span class="current" aria-current="page">{t("Documentation")}</span>
	{:else}
		<a href="/docs">{t("Documentation")}</a>
		{#each ancestors as ancestor (ancestor.id)}
			<span class="sep" aria-hidden="true">›</span>
			<a href={localize(hrefForAncestor(ancestor))}>{t(ancestor.label)}</a>
		{/each}
		<span class="sep" aria-hidden="true">›</span>
		<span class="current" aria-current="page">{t(currentLabel)}</span>
	{/if}
</nav>

<style>
	.breadcrumb {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.35rem;
		min-height: 2rem;
		font-size: 0.8rem;
		line-height: 1;
	}

	a,
	.current,
	.sep {
		display: inline-flex;
		align-items: center;
		line-height: 1;
	}

	a {
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	a:hover {
		color: var(--color-text);
		text-decoration: underline;
	}

	.sep {
		color: var(--color-text-faint);
		user-select: none;
	}

	.current {
		color: var(--color-text);
	}
</style>
