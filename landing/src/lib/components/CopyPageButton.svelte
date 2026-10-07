<script>
	import { browser } from '$app/environment';
	import { loadRawDoc } from '$lib/rawDocModules.js';
	import { routeFromHref } from '$lib/docs.js';
	import chevronDown from '$lib/icons/chevron-down.svg?raw';
	import documentDuplicate from '$lib/icons/document-duplicate.svg?raw';
	import linkIcon from '$lib/icons/link.svg?raw';
	import githubIcon from '$lib/icons/github.svg?raw';

	const VIEW_BASE = 'https://github.com/themadorg/madmail/blob/main/docs';

	/** @type {{ currentHref?: string }} */
	let { currentHref = '/docs' } = $props();

	let menuOpen = $state(false);
	let status = $state('');

	/** @type {ReturnType<typeof setTimeout> | undefined} */
	let statusTimer;

	const route = $derived(routeFromHref(currentHref));
	const pageUrl = $derived(browser ? `${window.location.origin}${currentHref}` : currentHref);
	const githubUrl = $derived(`${VIEW_BASE}/${route}.md`);

	/** @param {string} message */
	function showStatus(message) {
		status = message;
		clearTimeout(statusTimer);
		statusTimer = setTimeout(() => {
			status = '';
		}, 1800);
	}

	/** @param {string} text */
	async function copyText(text) {
		if (navigator.clipboard?.writeText) {
			try { await navigator.clipboard.writeText(text); return; } catch { /* Try the HTTP-compatible fallback. */ }
		}
		const previousFocus = document.activeElement;
		const input = document.createElement('textarea');
		input.value = text;
		input.style.position = 'fixed';
		input.style.opacity = '0';
		document.body.append(input);
		try {
			input.select();
			if (!document.execCommand('copy')) throw new Error('Copy failed');
		} finally {
			input.remove();
			if (previousFocus instanceof HTMLElement) previousFocus.focus({ preventScroll: true });
		}
	}

	async function copyMarkdown() {
		try {
			const markdown = await loadRawDoc(route);
			if (markdown === null || markdown === undefined) throw new Error('Page source unavailable');
			await copyText(markdown);
			showStatus('Copied');
			menuOpen = false;
		} catch {
			showStatus('Copy failed');
		}
	}

	async function copyLink() {
		try {
			await copyText(pageUrl);
			showStatus('Link copied');
			menuOpen = false;
		} catch {
			showStatus('Copy failed');
		}
	}

	function toggleMenu() {
		menuOpen = !menuOpen;
	}

	function closeMenu() {
		menuOpen = false;
	}

	/** @param {MouseEvent} event */
	function handleDocumentClick(event) {
		if (!(event.target instanceof Element)) return;
		if (!event.target.closest('.copy-page')) closeMenu();
	}

	$effect(() => {
		if (!browser || !menuOpen) return;
		document.addEventListener('click', handleDocumentClick);
		return () => document.removeEventListener('click', handleDocumentClick);
	});
</script>

<div class="copy-page">
	<div class="copy-page__button">
		<button type="button" class="copy-page__main" onclick={copyMarkdown}>
			<span class="icon" aria-hidden="true">{@html documentDuplicate}</span>
			<span>{status || 'Copy page'}</span>
		</button>
		<button
			type="button"
			class="copy-page__menu-toggle"
			aria-label="Copy options"
			aria-expanded={menuOpen}
			aria-haspopup="menu"
			onclick={toggleMenu}
		>
			<span class="icon menu-chevron" class:open={menuOpen} aria-hidden="true">{@html chevronDown}</span>
		</button>
	</div>

	{#if menuOpen}
		<div class="copy-page__menu" role="menu">
			<button type="button" role="menuitem" onclick={copyMarkdown}><span class="icon" aria-hidden="true">{@html documentDuplicate}</span>Copy page</button>
			<button type="button" role="menuitem" onclick={copyLink}><span class="icon" aria-hidden="true">{@html linkIcon}</span>Copy link</button>
			<a href={githubUrl} role="menuitem" target="_blank" rel="noopener noreferrer"><span class="icon" aria-hidden="true">{@html githubIcon}</span>View on GitHub</a>
		</div>
	{/if}
</div>

<style>
	.copy-page {
		position: relative;
		flex-shrink: 0;
	}

	.copy-page__button {
		display: flex;
		align-items: stretch;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		overflow: hidden;
	}

	.copy-page__main,
	.copy-page__menu-toggle {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-text);
		font: inherit;
		font-size: 0.8rem;
		line-height: 1;
		cursor: pointer;
	}

	.copy-page__main {
		padding: 0.45rem 0.7rem;
	}

	.copy-page__menu-toggle {
		justify-content: center;
		width: 2rem;
		border-left: 1px solid var(--color-border);
	}

	.copy-page__main:hover,
	.copy-page__menu-toggle:hover,
	.copy-page__menu a:hover,
	.copy-page__menu button:hover {
		background: var(--color-hover);
	}

	.menu-chevron { transition: transform 160ms ease; }
	.menu-chevron.open { transform: rotate(180deg); }
	@media (prefers-reduced-motion: reduce) { .menu-chevron { transition: none; } }
	.icon { flex-shrink: 0; }
	.icon :global(svg) {
		display: block;
		width: 0.95rem;
		height: 0.95rem;
	}

	.copy-page__menu {
		position: absolute;
		top: calc(100% + 0.35rem);
		right: 0;
		z-index: 20;
		display: flex;
		flex-direction: column;
		min-width: 10.5rem;
		padding: 0.25rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		box-shadow: 0 8px 24px var(--color-shadow);
	}

	.copy-page__menu button,
	.copy-page__menu a {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.5rem 0.65rem;
		border: none;
		border-radius: 0.35rem;
		background: transparent;
		color: var(--color-text);
		font: inherit;
		font-size: 0.8rem;
		line-height: 1.3;
		text-align: left;
		text-decoration: none;
		cursor: pointer;
	}
</style>
