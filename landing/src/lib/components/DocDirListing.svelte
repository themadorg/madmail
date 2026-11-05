<script>
	import chevronRight from '$lib/icons/chevron-right.svg?raw';

	/** @type {{ title: string, entries: import('$lib/docTreeData.js').DocDirListingEntry[] }} */
	let { title, entries } = $props();

	const folders = $derived(entries.filter((entry) => entry.kind === 'dir'));
	const pages = $derived(entries.filter((entry) => entry.kind === 'file'));
</script>

<h1>{title}</h1>

{#if entries.length === 0}
	<p class="empty">This section has no pages yet.</p>
{:else}
	{#if folders.length}
		<section class="section">
			<p class="group-label">Folders</p>
			<ul class="list">
				{#each folders as entry (entry.href)}
					<li>
						<a href={entry.href} class="item item--dir">
							<span class="label">{entry.label}</span>
							<span class="icon" aria-hidden="true">{@html chevronRight}</span>
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if pages.length}
		<section class="section">
			<p class="group-label">{folders.length ? 'Pages' : 'Pages in this section'}</p>
			<ul class="list">
				{#each pages as entry (entry.href)}
					<li>
						<a href={entry.href} class="item">
							<span class="label">{entry.label}</span>
							<span class="icon" aria-hidden="true">{@html chevronRight}</span>
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
{/if}

<style>
	h1 {
		margin: 0 0 0.5rem;
		font-size: clamp(1.75rem, 5vw, 2.25rem);
		font-weight: 700;
		line-height: 1.2;
		letter-spacing: -0.03em;
	}

	.empty {
		margin: 1.5rem 0 0;
		color: var(--color-text-muted);
	}

	.section {
		margin-top: 2rem;
	}

	/* Selectors are intentionally specific to beat .prose global styles from DocPage. */
	.section p.group-label {
		margin: 0 0 0.75rem;
		font-size: 0.8rem;
		font-weight: 600;
		line-height: 1.4;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--color-text-faint);
	}

	.section ul.list {
		margin: 0;
		padding: 0;
		list-style: none;
		line-height: 1.4;
		border: 1px solid var(--color-border);
		border-radius: 0;
		overflow: hidden;
	}

	.section ul.list > li {
		margin: 0;
		padding: 0;
	}

	.section ul.list > li > a.item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		max-width: none;
		padding: 0.85rem 1rem;
		overflow: visible;
		border-top: 1px solid var(--color-border);
		color: var(--color-text);
		font-size: 0.95rem;
		font-weight: 500;
		text-decoration: none;
		white-space: normal;
		transition: background 0.15s ease;
	}

	.section ul.list > li:first-child > a.item {
		border-top: none;
	}

	.section ul.list > li > a.item:hover {
		background: var(--color-hover);
		text-decoration: none;
	}

	.label {
		min-width: 0;
	}

	.icon :global(svg) {
		display: block;
		flex-shrink: 0;
		width: 1rem;
		height: 1rem;
		color: var(--color-text-faint);
	}
</style>
