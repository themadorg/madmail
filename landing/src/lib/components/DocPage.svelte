<script>
	import { docTreeModal, openDocTree } from '$lib/docTreeModal.svelte.js';
	import showSidebarIcon from '$lib/icons/chevron-right.svg?raw';
	import { setDocFocus } from '$lib/docFocus.js';
	import exitFocusIcon from '$lib/icons/arrows-pointing-in.svg?raw';
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import DocBreadcrumb from '$lib/components/DocBreadcrumb.svelte';
	import DocFooter from '$lib/components/DocFooter.svelte';
	import DocHeader from '$lib/components/DocHeader.svelte';
	import DocPageActions from '$lib/components/DocPageActions.svelte';
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import { enhanceDocProse } from '$lib/enhanceDocProse.js';

	/** @type {{ href: string, children: import('svelte').Snippet }} */
	let { href, children } = $props();
	const focused = $derived(browser && page.url.searchParams.get('focus') === 'true');

	$effect(() => {
		if (!browser) return;
		href;
		window.scrollTo(0, 0);
	});

	$effect(() => {
		if (!browser) return;
		const hash = page.url.hash;
		if (!hash) return;

		requestAnimationFrame(() => {
			document.querySelector(hash)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
		});
	});
</script>

<svelte:window onkeydown={(event) => { if (focused && event.key === 'Escape') setDocFocus(false); }} />
{#if !focused}<DocHeader currentHref={href} />{/if}
<main class="doc" class:focused>
	{#if !focused}
	<div class="page-breadcrumb">
		<DocBreadcrumb currentHref={href} />
		<div class="page-tools">
			<DocPageActions currentHref={href} />
		</div>
	</div>
	{/if}
	<article class="prose" use:enhanceDocProse>
		{@render children()}
	</article>
	{#if !focused}
	<DocPageActions currentHref={href} variant="footer" />
	<DocFooter {href} />
	{/if}
</main>
{#if !focused}<SiteFooter spacious />{/if}
{#if !focused && !docTreeModal.open}
	<button class="restore-sidebar" aria-label="Show documentation sidebar" title="Show sidebar" onclick={() => { docTreeModal.docked = true; openDocTree(); }}>
		<span aria-hidden="true">{@html showSidebarIcon}</span>
	</button>
{/if}
{#if focused}
	<button class="exit-focus" aria-label="Exit focus mode" title="Exit focus mode (Escape)" onclick={() => setDocFocus(false)}>
		<span aria-hidden="true">{@html exitFocusIcon}</span>
		<span>Exit focus</span>
	</button>
{/if}

<style>
	.restore-sidebar { position: fixed; bottom: 1rem; left: 1rem; z-index: 50; display: grid; place-items: center; width: 2rem; height: 2rem; border: 1px solid var(--color-border); border-radius: 0.5rem; background: var(--color-surface); color: var(--color-text-muted); cursor: pointer; }
	.restore-sidebar:hover { background: var(--color-surface-raised); color: var(--color-text); }
	.restore-sidebar :global(svg) { display: block; width: 1rem; height: 1rem; }

	.exit-focus {
		position: fixed;
		bottom: 1rem;
		left: 1rem;
		z-index: 50;
		display: flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.55rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		color: var(--color-text);
		font: inherit;
		font-size: 0.75rem;
		opacity: 0.35;
		cursor: pointer;
		transition: opacity 160ms ease, background-color 160ms ease;
	}
	.exit-focus:hover, .exit-focus:focus-visible { opacity: 1; background: var(--color-surface-raised); }
	.exit-focus :global(svg) { display: block; width: 1rem; height: 1rem; }
	@media (prefers-reduced-motion: reduce) { .exit-focus { transition: none; } }

	.doc.focused { padding-top: 2rem; }
	.page-tools { display: flex; align-items: center; gap: 0.5rem; }

	.doc {
		position: relative;
		z-index: 1;
		min-height: 100vh;
		min-height: 100dvh;
		min-width: 0;
		padding: 4.5rem 2rem 0;
		background: var(--color-bg);
		color: var(--color-text);
		transition: var(--transition-theme);
	}

	.page-breadcrumb {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex-wrap: wrap;
		max-width: 48rem;
		margin: 0 auto 1.25rem;
	}

	.prose {
		max-width: 48rem;
		margin: 0 auto;
		padding: 0 0 3.5rem;
		min-width: 0;
		font-size: 1.0625rem;
		line-height: 1.75;
		color: var(--color-text);
	}

	@media (max-width: 640px) {
		.doc {
		position: relative;
		z-index: 1;
			padding: 4rem 1rem 0;
		}
	}

	/*
	 * Headings are wrapped in .heading-wrap divs at runtime, so spacing
	 * relies on direct margins (which collapse through the wrapper)
	 * instead of adjacent-sibling selectors.
	 */
	.prose :global(p),
	.prose :global(ul),
	.prose :global(ol),
	.prose :global(table),
	.prose :global(blockquote) {
		margin: 1.25rem 0;
		min-width: 0;
	}

	.prose :global(h1) {
		margin: 0 0 1.75rem;
		font-size: 2rem;
		font-weight: 700;
		line-height: 1.35;
		letter-spacing: -0.03em;
		color: var(--color-text);
	}

	.prose :global(h2) {
		margin: 4.5rem 0 1.25rem;
		padding-bottom: 0.65rem;
		border-bottom: 1px solid var(--color-border);
		font-size: 1.45rem;
		font-weight: 650;
		line-height: 1.5;
		letter-spacing: -0.015em;
		color: var(--color-text);
	}

	.prose :global(h3) {
		margin: 3rem 0 0.85rem;
		font-size: 1.175rem;
		font-weight: 600;
		line-height: 1.45;
		color: var(--color-text);
	}

	.prose :global(h4) {
		margin: 2.25rem 0 0.65rem;
		font-size: 1.0625rem;
		font-weight: 600;
		line-height: 1.5;
		color: var(--color-text);
	}

	.prose > :global(:first-child),
	.prose > :global(.heading-wrap:first-child h1),
	.prose > :global(.heading-wrap:first-child h2),
	.prose > :global(.heading-wrap:first-child h3) {
		margin-top: 0;
	}

	.prose :global(ul),
	.prose :global(ol) {
		padding-left: 1.6rem;
		line-height: 1.7;
		min-width: 0;
	}

	.prose :global(li) {
		min-width: 0;
	}

	.prose :global(li + li) {
		margin-top: 0.45rem;
	}

	.prose :global(li > ul),
	.prose :global(li > ol) {
		margin: 0.45rem 0 0;
	}

	.prose :global(li > p) {
		margin: 0.45rem 0;
	}

	.prose :global(li::marker) {
		color: var(--color-text-subtle);
	}

	.prose :global(strong) {
		font-weight: 650;
		color: var(--color-text);
	}

	.prose :global(a) {
		color: var(--color-text);
		text-decoration-color: var(--color-border-strong);
		text-underline-offset: 0.2em;
	}

	.prose :global(a:not(.heading-link):not(.doc-toc__list a)) {
		display: inline-block;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		vertical-align: bottom;
	}

	.prose :global(a:hover) {
		text-decoration: none;
	}

	.prose :global(img) {
		display: block;
		max-width: 100%;
		margin: 1.5rem 0;
		border-radius: 0.5rem;
	}

	.prose :global(pre) {
		margin: 1.25rem 0;
		padding: 1rem 1.25rem;
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		font-family: var(--font-mono);
		font-size: 0.84rem;
		line-height: 1.65;
		color: var(--color-code-text);
	}

	.prose :global(.code-block) {
		margin: 1.25rem 0;
	}

	.prose :global(.code-block > pre) {
		margin: 0;
		border: none;
		border-radius: 0;
		background: transparent;
		font: inherit;
		color: inherit;
	}

	.prose :global(.rfc-text) {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 0.84rem;
		line-height: 1.55;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--color-text);
	}

	.prose :global(code) {
		font-family: var(--font-mono);
		font-size: 0.88em;
	}

	.prose :global(:not(pre) > code) {
		padding: 0.12rem 0.35rem;
		border-radius: 0.25rem;
		background: var(--color-surface-code);
		color: var(--color-code-text);
	}

	.prose :global(table) {
		display: block;
		width: 100%;
		overflow-x: auto;
		overflow-wrap: normal;
		word-break: normal;
		overscroll-behavior-x: contain;
		border-collapse: collapse;
		font-size: 0.925rem;
		line-height: 1.55;
	}

	.prose :global(th),
	.prose :global(td) {
		min-width: 10rem;
		padding: 0.65rem 0.85rem;
		border: 1px solid var(--color-border);
		text-align: left;
		vertical-align: top;
	}

	.prose :global(th) {
		white-space: nowrap;
		background: var(--color-surface-raised);
		font-weight: 600;
	}

	.prose :global(td > code),
	.prose :global(th > code) {
		white-space: nowrap;
	}

	.prose :global(blockquote) {
		padding: 0.5rem 1.1rem;
		border-left: 3px solid var(--color-border-strong);
		border-radius: 0 0.4rem 0.4rem 0;
		background: var(--color-surface);
		color: var(--color-text-subtle);
		line-height: 1.7;
	}

	.prose :global(blockquote > p) {
		margin: 0.35rem 0;
	}

	.prose :global(hr) {
		margin: 2.75rem 0;
		border: none;
		border-top: 1px solid var(--color-border);
	}

	.prose :global(.heading-wrap) {
		position: relative;
		margin-left: -1.75rem;
		padding-left: 1.75rem;
	}

	.prose :global(.heading-link) {
		position: absolute;
		left: 0;
		top: 0;
		display: flex;
		align-items: flex-start;
		justify-content: flex-end;
		width: 1.25rem;
		height: auto;
		max-width: none;
		overflow: visible;
		white-space: normal;
		border: none;
		background: none;
		color: var(--color-text-faint);
		font-size: 1.1em;
		font-weight: 600;
		line-height: 1;
		text-decoration: none;
		opacity: 0;
		outline: none;
		box-shadow: none;
		-webkit-tap-highlight-color: transparent;
		transition: opacity 0.15s ease, color 0.15s ease;
	}

	@media (max-width: 640px) {
		.prose :global(.heading-wrap) {
			margin-left: 0;
			padding-left: 0;
		}

		.prose :global(.heading-link) {
			display: none;
		}
	}

	.prose :global(.heading-wrap:hover .heading-link),
	.prose :global(.heading-link[data-copied]) {
		opacity: 1;
	}

	.prose :global(.heading-link:hover),
	.prose :global(.heading-link[data-copied]) {
		color: var(--color-text);
	}

	.prose :global(.heading-link:focus) {
		outline: none;
		box-shadow: none;
	}

	.prose :global(.heading-link:focus-visible) {
		opacity: 1;
		color: var(--color-text);
		outline: none;
		box-shadow: none;
	}

	.prose :global(h1[id]),
	.prose :global(h2[id]),
	.prose :global(h3[id]),
	.prose :global(h4[id]) {
		scroll-margin-top: 7rem;
	}

	.prose :global(.doc-toc) {
		margin-top: 1rem;
		padding: 0.8rem 1rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
	}

	.prose :global(.doc-toc__summary) {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 0.95rem;
		font-weight: 600;
		line-height: 1.4;
		color: var(--color-text);
		cursor: pointer;
		list-style: none;
		user-select: none;
	}

	.prose :global(.doc-toc__summary::-webkit-details-marker) {
		display: none;
	}

	.prose :global(.doc-toc__summary::before) {
		content: '';
		width: 0.45rem;
		height: 0.45rem;
		border-right: 1.5px solid var(--color-text-subtle);
		border-bottom: 1.5px solid var(--color-text-subtle);
		transform: rotate(-45deg);
		transition: transform 0.15s ease;
	}

	.prose :global(.doc-toc[open] .doc-toc__summary::before) {
		transform: rotate(45deg);
	}

	.prose :global(.doc-toc__list) {
		margin: 0.75rem 0 0;
		padding-left: 1.35rem;
		font-size: 0.9rem;
		line-height: 1.55;
	}

	.prose :global(.doc-toc__list a) {
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	.prose :global(.doc-toc__list a:hover) {
		color: var(--color-text);
		text-decoration: underline;
	}
</style>
