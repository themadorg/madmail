<script>
	import { onMount, untrack, tick, setContext } from 'svelte';
	import { onNavigate } from '$app/navigation';
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import { createSiteLanguage, languageContext } from '$lib/siteLanguage.svelte.js';
	import { languageFromPath, stripLanguagePrefix } from '$lib/localization.js';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import DocTreeModal from '$lib/components/DocTreeModal.svelte';
	import SiteMeta from '$lib/components/SiteMeta.svelte';
	import { docTreeModal } from '$lib/docTreeModal.svelte.js';
	import madmailLogoUrl from '$lib/logoUrl.js';
	import { madMode } from '$lib/madMode.svelte.js';
	import { theme, watchSystemTheme } from '$lib/theme.svelte.js';
	import '$lib/styles/global.css';
	import { localizeSiteLinks } from '$lib/localizeSiteLinks.js';

	let { children } = $props();
	const scope = createSiteLanguage(languageFromPath(page.url.pathname));
	setContext(languageContext, scope);
	const { siteLanguage, setSiteLanguage, localize, t } = scope;
	const pathname = $derived(stripLanguagePrefix(page.url.pathname));
	onMount(watchSystemTheme);
	$effect(() => {
		const language = languageFromPath(page.url.pathname);
		untrack(() => setSiteLanguage(language));
	});
	$effect(() => {
		if (!browser) return;
		page.url.pathname;
		document.documentElement.lang = siteLanguage.code === 'fa' ? 'fa' : 'en';
		document.documentElement.dir = siteLanguage.code === 'fa' ? 'rtl' : 'ltr';
		queueMicrotask(() => {
			for (const [selector, message] of [['.code-copy', 'Copy code'], ['.code-edit', 'Edit code'], ['.code-undo', 'Undo changes']]) {
				document.querySelectorAll(selector).forEach((button) => button.setAttribute('aria-label', t(message)));
			}
			document.querySelectorAll('.heading-link').forEach((link) => {
				const heading = link.closest('.heading-wrap')?.querySelector('h1,h2,h3,h4,h5,h6');
				link.setAttribute('aria-label', `${t('Copy link to')} ${heading?.textContent ?? ''}`);
			});
		});
	});

	$effect(() => {
		if (!browser) return;
		document.documentElement.classList.toggle('docs-theme', pathname === '/docs' || pathname.startsWith('/docs/') || pathname === '/releases' || pathname.startsWith('/releases/') || pathname === '/blog' || pathname.startsWith('/blog/'));
	});

	let activeTransition;
	if (browser) {
		onNavigate((navigation) => {
			activeTransition?.skipTransition();
			if (!document.startViewTransition) return;
			if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

			const fromDocs = stripLanguagePrefix(navigation.from?.url.pathname ?? '').startsWith('/docs');
			const toDocs = stripLanguagePrefix(navigation.to?.url.pathname ?? '').startsWith('/docs');
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
		const onDocs = pathname === '/docs' || pathname.startsWith('/docs/');
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

		const onDocs = pathname.startsWith('/docs');
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
	<link rel="stylesheet" href="/fonts/fonts.css" />
</svelte:head>

<div style="display: contents" use:localizeSiteLinks={{ localize, code: siteLanguage.code }}>

{#key page.url.pathname}
	{@render children()}
{/key}
<CommandPalette />
{#if !browser || page.url.searchParams.get('focus') !== 'true'}
	<DocTreeModal currentHref={pathname} />
{/if}

</div>
