<script>
	import { onNavigate } from '$app/navigation';
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import DocTreeModal from '$lib/components/DocTreeModal.svelte';
	import SiteMeta from '$lib/components/SiteMeta.svelte';
	import { docTreeModal } from '$lib/docTreeModal.svelte.js';
	import { madMode } from '$lib/madMode.svelte.js';
	import { theme } from '$lib/theme.svelte.js';
	import '$lib/styles/global.css';

	let { children } = $props();

	if (browser) {
		onNavigate((navigation) => {
			if (!document.startViewTransition) return;
			if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

			const fromDocs = navigation.from?.url.pathname.startsWith('/docs');
			const toDocs = navigation.to?.url.pathname.startsWith('/docs');
			if (fromDocs && toDocs) return;

			return new Promise((resolve) => {
				document.startViewTransition(async () => {
					resolve();
					await navigation.complete;
				});
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

		const onDocs = page.url.pathname.startsWith('/docs');
		const dockedOpen = onDocs && docTreeModal.open && docTreeModal.docked;

		document.documentElement.classList.toggle('doc-tree-docked', dockedOpen);

		if (docTreeModal.open && !onDocs) {
			docTreeModal.open = false;
		}
	});
</script>

<SiteMeta />

<svelte:head>
	<link rel="icon" href="/favicon.ico" sizes="any" />
	<link rel="icon" href="/favicon.png" type="image/png" sizes="32x32" />
	<link rel="apple-touch-icon" href="/apple-touch-icon.png" />
	<link rel="stylesheet" href="/fonts/fonts.css" />
</svelte:head>

{@render children()}
<CommandPalette />
<DocTreeModal currentHref={page.url.pathname} />
