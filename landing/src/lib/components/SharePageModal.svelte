<script>
	import { onDestroy } from 'svelte';
	import copyIcon from '$lib/icons/document-duplicate.svg?raw';
	import checkIcon from '$lib/icons/check.svg?raw';
	import deltaIcon from '$lib/icons/deltachat.svg?raw';
	import telegramIcon from '$lib/icons/telegram.svg?raw';
	import Modal from '$lib/components/Modal.svelte';
	let { open = $bindable(false), url = '', title = '' } = $props();
	let linkInput;
	let status = $state('');
	let copied = $state(false);
	let resetTimer;
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
		} catch { status = 'Select the link to copy it'; }
	}
</script>

<Modal bind:open title="Share page" maxWidth="30rem">
	<label for="share-page-link">Page link</label>
	<div class="link-row">
		<input id="share-page-link" bind:this={linkInput} value={url} readonly onclick={(event) => event.currentTarget.select()} />
		<button class="copy-icon" class:copied aria-label={copied ? 'Link copied' : 'Copy link'} title={copied ? 'Link copied' : 'Copy link'} onclick={copyLink}>{#key copied}<span class="copy-symbol" aria-hidden="true">{@html copied ? checkIcon : copyIcon}</span>{/key}</button>
	</div>
	<div class="share-options">
		<a href={`mailto:?body=${encodeURIComponent(`${title}\n${url}`)}`}><span class="service-icon" aria-hidden="true">{@html deltaIcon}</span>Delta Chat</a>
		<a href={`https://t.me/share/url?url=${encodeURIComponent(url)}&text=${encodeURIComponent(title)}`} target="_blank" rel="noopener noreferrer"><span class="service-icon" aria-hidden="true">{@html telegramIcon}</span>Telegram</a>
	</div>
	{#if status}<p role="status">{status}</p>{/if}
</Modal>

<style>
	.copy-icon { display: grid; place-items: center; width: 2.6rem; flex-shrink: 0; }
	.copy-icon.copied { color: #3fb950; border-color: #3fb950; }
	.copy-symbol { display: block; animation: copy-pop 180ms ease-out; }
	.copy-symbol :global(svg), .service-icon :global(svg) { display: block; width: 1.1rem; height: 1.1rem; }
	.service-icon { display: block; width: 1.1rem; height: 1.1rem; flex: 0 0 1.1rem; }
	.share-options a { display: inline-flex; align-items: center; gap: 0.5rem; }
	@keyframes copy-pop { from { opacity: 0; transform: scale(0.6); } to { opacity: 1; transform: scale(1); } }
	@media (prefers-reduced-motion: reduce) { .copy-symbol { animation: none; } }

	label { font-size: 0.8rem; color: var(--color-text-muted); }
	.link-row { display: flex; gap: 0.5rem; margin: 0.5rem 0 1.25rem; }
	input { flex: 1; min-width: 0; padding: 0.6rem; border: 1px solid var(--color-border); border-radius: 0.4rem; background: var(--color-bg); color: var(--color-text); font: inherit; font-size: 0.8rem; }
	button, a { padding: 0.6rem 0.8rem; border: 1px solid var(--color-border); border-radius: 0.4rem; background: var(--color-surface); color: var(--color-text); font: inherit; font-size: 0.85rem; text-decoration: none; cursor: pointer; }
	button:hover, a:hover { background: var(--color-hover); }
	.share-options { display: flex; flex-wrap: wrap; gap: 0.5rem; }
	p { color: var(--color-text-muted); font-size: 0.8rem; }
</style>
