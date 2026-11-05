<script>
	import { onMount, untrack, tick } from 'svelte';
	import { onNavigate } from '$app/navigation';
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import DocTreeModal from '$lib/components/DocTreeModal.svelte';
	import SiteMeta from '$lib/components/SiteMeta.svelte';
	import { docTreeModal } from '$lib/docTreeModal.svelte.js';
	import { homePath, langDir } from '$lib/i18n/index.js';
	import { loadPreference, savePreference } from '$lib/locale.svelte.js';
	import madmailLogoUrl from '$lib/logoUrl.js';
	import { madMode } from '$lib/madMode.svelte.js';
	import { theme, watchSystemTheme } from '$lib/theme.svelte.js';
	import '$lib/styles/global.css';

	let { children } = $props();
	onMount(watchSystemTheme);
	onMount(loadPreference);

	/* Keep <html lang/dir> right across client navigation; hooks.server.js sets it for prerendered HTML. */
	$effect(() => {
		if (!browser) return;
		const lang = page.data?.lang ?? 'en';
		document.documentElement.lang = lang;
		document.documentElement.dir = langDir(lang);
		/* Opening a home edition is a language choice: docs and releases chrome follow it. */
		if (page.data?.lang && page.url.pathname === homePath(page.data.lang)) {
			untrack(() => savePreference(page.data.lang));
		}
	});

	$effect(() => {
		if (!browser) return;
		document.documentElement.classList.toggle('docs-theme', /^\/(docs|releases|blog)(\/|$)/.test(page.url.pathname));
	});

	let activeTransition;
	if (browser) {
		onNavigate((navigation) => {
			activeTransition?.skipTransition();
			if (!document.startViewTransition) return;
			if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

			const fromDocs = navigation.from?.url.pathname.startsWith('/docs');
			const toDocs = navigation.to?.url.pathname.startsWith('/docs');
			if (fromDocs && toDocs) return;

			return new Promise((resolve) => {
				activeTransition = document.startViewTransition(async () => {
					resolve();
					await navigation.complete;
					await tick();
				});
				activeTransition.ready.catch(() => resolve());
			});
		});
	}

	$effect(() => {
		if (!browser) return;

		document.documentElement.dataset.theme = theme.light ? 'light' : 'dark';
		if (theme.light) {
			document.documentElement.dataset.light = '';
		} else {
			delete document.documentElement.dataset.light;
		}
	});

	$effect(() => {
		if (!browser) return;

		if (madMode.active) {
			document.documentElement.dataset.mad = '';
		} else {
			delete document.documentElement.dataset.mad;
		}
	});

	$effect(() => {
		if (!browser) return;
		document.documentElement.style.setProperty('--doc-sidebar-width', `${docTreeModal.width}px`);
	});

	let wasOnDocs = false;
	$effect(() => {
		if (!browser) return;
		const onDocs = page.url.pathname === '/docs' || page.url.pathname.startsWith('/docs/');
		untrack(() => {
			if (onDocs && !wasOnDocs && window.matchMedia('(min-width: 1024px)').matches) {
				docTreeModal.open = true;
				docTreeModal.docked = true;
			}
			wasOnDocs = onDocs;
		});
	});

	$effect(() => {
		if (!browser) return;

		const onDocs = page.url.pathname.startsWith('/docs');
		const dockedOpen = onDocs && page.url.searchParams.get('focus') !== 'true' && docTreeModal.open && docTreeModal.docked;

		document.documentElement.classList.toggle('doc-tree-docked', dockedOpen);

		if (docTreeModal.open && !onDocs) {
			docTreeModal.open = false;
		}
	});
</script>

<SiteMeta />

<svelte:head>
	<link rel="preload" href={madmailLogoUrl} as="image" type="image/png" fetchpriority="high" />
	<link rel="icon" href="/favicon.ico" sizes="any" />
	<link rel="icon" href="/favicon.png" type="image/png" sizes="32x32" />
	<link rel="apple-touch-icon" href="/apple-touch-icon.png" />
	<link rel="manifest" href="/site.webmanifest" />
	<link rel="stylesheet" href="/fonts/fonts.css" />
</svelte:head>

{#key page.url.pathname}
	{@render children()}
{/key}
<CommandPalette />
{#if !browser || page.url.searchParams.get('focus') !== 'true'}
	<DocTreeModal currentHref={page.url.pathname} />
{/if}
