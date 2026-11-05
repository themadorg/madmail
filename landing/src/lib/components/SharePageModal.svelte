<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
	import { onDestroy, onMount } from 'svelte';
	import copyIcon from '$lib/icons/document-duplicate.svg?raw';
	import checkIcon from '$lib/icons/check.svg?raw';
	import webxdcIcon from '$lib/icons/webxdc.png';
	import shareIcon from '$lib/icons/share.svg?raw';
	import Modal from '$lib/components/Modal.svelte';
	let { open = $bindable(false), url = '', title = '', article = null } = $props();
	let linkInput;
	let status = $state('');
	let copied = $state(false);
	let resetTimer;
	let exporting = $state(false);
	let sharing = $state(false);
	let nativeShare = $state(false);
	onMount(() => { nativeShare = typeof navigator.share === 'function'; });
	async function sharePage() {
		if (sharing || !nativeShare) return;
		status = '';
		sharing = true;
		try {
			await navigator.share({ title, url });
		} catch (error) {
			if (error?.name !== 'AbortError') status = 'Could not open sharing. You can copy the page link instead.';
		} finally {
			sharing = false;
		}
	}
	async function shareWebxdc() {
		if (!article || exporting) return;
		exporting = true;
		status = '';
		try {
			const { downloadArticleWebxdc } = await import('$lib/articleWebxdc.js');
			downloadArticleWebxdc(article, url);
			status = 'Article downloaded. Attach the .xdc file in Delta Chat to share it.';
		} catch {
			status = 'Could not create the article file. Please try again.';
		} finally {
			exporting = false;
		}
	}
	onDestroy(() => clearTimeout(resetTimer));
	async function copyLink() {
		try {
			if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(url);
			else {
				linkInput.select();
				if (!document.execCommand('copy')) throw new Error('Copy failed');
			}
			status = '';
			copied = true;
			clearTimeout(resetTimer);
			resetTimer = setTimeout(() => { copied = false; }, 1400);
			return true;
		} catch { status = 'Select the link to copy it'; return false; }
	}
</script>

<Modal bind:open title={t("Share page")} maxWidth="30rem">
	<label for="share-page-link">{t("Page link")}</label>
	<div class="link-row">
		<input dir="ltr" id="share-page-link" bind:this={linkInput} value={url} readonly onclick={(event) => event.currentTarget.select()} />
		<button class="copy-icon" class:copied aria-label={t(copied ? 'Link copied' : 'Copy link')} title={t(copied ? 'Link copied' : 'Copy link')} onclick={copyLink}>{#key copied}<span class="copy-symbol" aria-hidden="true">{@html copied ? checkIcon : copyIcon}</span>{/key}</button>
	</div>
	<div class="share-options">
		{#if nativeShare}<button onclick={sharePage} disabled={sharing}><span class="service-icon" aria-hidden="true">{@html shareIcon}</span>{t("Share")}</button>{/if}
		{#if article}
			<button onclick={shareWebxdc} disabled={exporting}><img class="service-icon" src={webxdcIcon} alt="" width="18" height="18" />{t(exporting ? 'Creating webxdc…' : 'Download webxdc')}</button>
		{/if}
	</div>
	{#if status}<p role="status">{status}</p>{/if}
</Modal>

<style>
	.copy-icon { display: grid; place-items: center; width: 2.6rem; height: 2.6rem; box-sizing: border-box; padding: 0; flex-shrink: 0; }
	.copy-icon.copied { color: #3fb950; border-color: #3fb950; }
	.copy-symbol { display: block; animation: copy-pop 180ms ease-out; }
	.copy-symbol :global(svg), .service-icon :global(svg) { display: block; width: 1.1rem; height: 1.1rem; }
	.service-icon { display: block; width: 1.1rem; height: 1.1rem; flex: 0 0 1.1rem; }
	.share-options a, .share-options button { display: inline-flex; align-items: center; gap: 0.5rem; }
	@keyframes copy-pop { from { opacity: 0; transform: scale(0.6); } to { opacity: 1; transform: scale(1); } }
	@media (prefers-reduced-motion: reduce) { .copy-symbol { animation: none; } }

	label { font-size: 0.8rem; color: var(--color-text-muted); }
	.link-row { display: flex; align-items: center; gap: 0.5rem; min-width: 0; padding: 0.5rem; margin: 0.5rem 0 1.25rem; }
	input { flex: 1; min-width: 0; height: 2.6rem; box-sizing: border-box; padding: 0 0.65rem; direction: ltr; text-align: left; border: 1px solid var(--color-border); border-radius: 0.4rem; background: var(--color-bg); color: var(--color-text); font-family: var(--font-mono); font-size: 0.8rem; line-height: 1.4; }
	button, a { padding: 0.6rem 0.8rem; border: 1px solid var(--color-border); border-radius: 0.4rem; background: var(--color-surface); color: var(--color-text); font: inherit; font-size: 0.85rem; text-decoration: none; cursor: pointer; }
	button:disabled { opacity: 0.65; cursor: wait; }
	button:hover, a:hover { background: var(--color-hover); }
	.share-options { display: flex; flex-wrap: wrap; gap: 0.5rem; }
	p { color: var(--color-text-muted); font-size: 0.8rem; }
</style>
