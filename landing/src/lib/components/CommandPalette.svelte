<script>
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { loadSearchPages, searchPalette } from '$lib/commandSearch.js';
	import {
		closeCommandPalette,
		commandPalette,
		openCommandPalette
	} from '$lib/commandPalette.svelte.js';
	import { theme } from '$lib/theme.svelte.js';
	import magnifyingGlass from '$lib/icons/magnifying-glass.svg?raw';
	import { fade } from 'svelte/transition';
	import { cubicInOut } from 'svelte/easing';

	let query = $state('');
	let selectedIndex = $state(0);
	let loading = $state(false);

	/** @type {import('$lib/commandSearch.js').SearchPage[] | null} */
	let pages = $state(null);

	const results = $derived.by(() => {
		theme.light;
		return searchPalette(pages ?? [], query);
	});

	/** @type {HTMLInputElement | undefined} */
	let inputEl = $state();
	/** @type {HTMLDivElement | undefined} */
	let listEl = $state();

	const open = $derived(commandPalette.open);

	$effect(() => {
		results.length;
		selectedIndex = 0;
	});

	$effect(() => {
		if (!open || !browser) return;
		query = '';
		selectedIndex = 0;
		queueMicrotask(() => inputEl?.focus());

		if (pages || loading) return;

		loading = true;
		loadSearchPages()
			.then((loaded) => {
				pages = loaded;
			})
			.finally(() => {
				loading = false;
			});
	});

	$effect(() => {
		if (!open || !listEl) return;
		const active = listEl.querySelector('[data-active="true"]');
		active?.scrollIntoView({ block: 'nearest' });
	});

	function close() {
		closeCommandPalette();
	}

	function openPalette() {
		openCommandPalette();
	}

	/** @param {KeyboardEvent} event */
	function onWindowKeydown(event) {
		if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
			event.preventDefault();
			if (open) {
				inputEl?.focus();
				inputEl?.select();
			} else {
				openPalette();
			}
			return;
		}

		if (!open) return;

		if (event.key === 'Escape') {
			event.preventDefault();
			close();
			return;
		}

		if (event.key === 'ArrowDown') {
			event.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, Math.max(results.length - 1, 0));
			return;
		}

		if (event.key === 'ArrowUp') {
			event.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
			return;
		}

		if (event.key === 'Enter' && results[selectedIndex]) {
			event.preventDefault();
			runItem(results[selectedIndex]);
		}
	}

	/** @param {import('$lib/commandSearch.js').PaletteItem} item */
	function runItem(item) {
		if (item.kind === 'command') {
			item.run();
			return;
		}

		navigate(item.href);
	}

	/** @param {string} href */
	function navigate(href) {
		close();
		goto(href);
	}
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#if commandPalette.open}
	<button type="button" class="backdrop" aria-label="Close command palette" transition:fade={{ duration: 180, easing: cubicInOut }} onclick={close}></button>
	<div
		class="palette"
		role="dialog"
		aria-modal="true"
		aria-label="Command palette"
		transition:fade={{ duration: 180, easing: cubicInOut }}
	>
		<label class="search">
			<span class="icon" aria-hidden="true">{@html magnifyingGlass}</span>
			<input
				bind:this={inputEl}
				type="search"
				placeholder="Search pages or run commands…"
				bind:value={query}
				aria-label="Search pages or run commands"
				autocomplete="off"
				spellcheck="false"
			/>
			<kbd class="hint" aria-hidden="true">Esc</kbd>
		</label>

		<div class="results" bind:this={listEl} role="listbox" aria-label="Commands and pages">
			{#if loading && !pages}
				<p class="status">Loading search index…</p>
			{:else if results.length}
				{#each results as item, index (item.id)}
					<button
						type="button"
						class="result"
						class:result--command={item.kind === 'command'}
						role="option"
						aria-selected={index === selectedIndex}
						data-active={index === selectedIndex ? 'true' : undefined}
						onmouseenter={() => (selectedIndex = index)}
						onclick={() => runItem(item)}
					>
						<span class="result-main">
							<span class="label">{item.label}</span>
							{#if item.hint && (query.trim() || item.kind === 'command')}
								<span class="match">{item.hint}</span>
							{/if}
						</span>
						<span class="group">{item.group}</span>
					</button>
				{/each}
			{:else}
				<p class="empty">No commands or pages match your search.</p>
			{/if}
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 120;
		padding: 0;
		border: none;
		background: var(--color-overlay);
		cursor: default;
	}

	.palette {
		position: fixed;
		top: min(18vh, 8rem);
		left: 50%;
		z-index: 121;
		display: flex;
		flex-direction: column;
		width: min(92vw, 36rem);
		max-height: min(70vh, 32rem);
		overflow: hidden;
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		background: var(--color-modal-bg);
		box-shadow: 0 1rem 3rem var(--color-shadow);
		transform: translateX(-50%);
	}

	.search {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		flex-shrink: 0;
		padding: 0.85rem 1rem;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-text-subtle);
	}

	.search:focus-within {
		color: var(--color-text);
	}

	.icon :global(svg) {
		display: block;
		width: 1rem;
		height: 1rem;
		flex-shrink: 0;
	}

	.search input {
		flex: 1;
		min-width: 0;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-text);
		font: inherit;
		font-size: 0.95rem;
		line-height: 1.4;
		outline: none;
	}

	.search input:focus-visible {
		outline: none;
		box-shadow: none;
	}

	.search input::placeholder {
		color: var(--color-text-faint);
	}

	.hint {
		flex-shrink: 0;
		padding: 0.15rem 0.4rem;
		border: 1px solid var(--color-border);
		border-radius: 0.35rem;
		background: var(--color-surface);
		font-family: var(--font-mono);
		font-size: 0.68rem;
		line-height: 1.2;
		color: var(--color-text-faint);
	}

	.results {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 0.35rem;
		scrollbar-width: thin;
		scrollbar-color: var(--color-scrollbar-thumb) var(--color-scrollbar-track);
	}

	.results::-webkit-scrollbar {
		width: 0.5rem;
	}

	.results::-webkit-scrollbar-thumb {
		border-radius: 0.25rem;
		background: var(--color-scrollbar-thumb);
	}

	.result {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		width: 100%;
		padding: 0.65rem 0.75rem;
		border: none;
		border-radius: 0.45rem;
		background: transparent;
		color: var(--color-text);
		font: inherit;
		font-size: 0.9rem;
		line-height: 1.35;
		text-align: left;
		cursor: pointer;
	}

	.result[data-active='true'],
	.result:hover {
		background: var(--color-hover);
	}

	.result--command .label {
		font-weight: 600;
	}

	.result-main {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}

	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.match {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.78rem;
		line-height: 1.35;
		color: var(--color-text-subtle);
	}

	.group {
		flex-shrink: 0;
		font-size: 0.75rem;
		color: var(--color-text-faint);
	}

	.status,
	.empty {
		margin: 0;
		padding: 1.25rem 0.75rem;
		font-size: 0.875rem;
		color: var(--color-text-subtle);
		text-align: center;
	}
</style>
