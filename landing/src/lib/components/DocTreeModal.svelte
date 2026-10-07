<script>
	import sunIcon from '$lib/icons/sun.svg?raw';
	import moonIcon from '$lib/icons/moon.svg?raw';
	import systemIcon from '$lib/icons/computer-desktop.svg?raw';
	import { theme, setTheme } from '$lib/theme.svelte.js';
	import { setDocFocus } from '$lib/docFocus.js';
	import focusIcon from '$lib/icons/arrows-pointing-out.svg?raw';
	import hideIcon from '$lib/icons/chevron-left.svg?raw';
	import Modal from '$lib/components/Modal.svelte';
	import { ancestorIdsForDirId, ancestorIdsForHref, docTreeNodes } from '$lib/docTreeData.js';
	import { filterDocTree } from '$lib/docTreeFilter.js';
	import { resolveDocHref, sameDocPage } from '$lib/docs.js';
	import { closeDocTree, docTreeModal } from '$lib/docTreeModal.svelte.js';
	import chevronRight from '$lib/icons/chevron-right.svg?raw';
	import magnifyingGlass from '$lib/icons/magnifying-glass.svg?raw';

	/** @type {{ currentHref?: string }} */
	let { currentHref = '' } = $props();

	let query = $state('');

	let folderPath = $state([]);
	let direction = $state(1);

	function resetFolderScroll(node) {
		const scrollContainer = node.closest('.body');
		if (scrollContainer) scrollContainer.scrollTop = 0;
	}

	function leaveFolder() {
		direction = -1;
		folderPath = folderPath.slice(0, -1);
		query = '';
	}

	function findFolder(id, nodes = docTreeNodes) {
		for (const node of nodes) {
			if (node.type !== 'dir') continue;
			if (node.id === id) return node;
			const found = findFolder(id, node.children);
			if (found) return found;
		}
		return null;
	}

	function searchFiles(nodes) {
		return nodes.flatMap((node) => node.type === 'dir' ? searchFiles(node.children) : [node]);
	}

	const currentFolder = $derived(findFolder(folderPath.at(-1)));
	const visibleNodes = $derived(query.trim()
		? searchFiles(filterDocTree(docTreeNodes, query, 'all'))
		: currentFolder?.children ?? docTreeNodes);

	function enterFolder(id) {
		direction = 1;
		folderPath = [...(ancestorIdsForDirId(docTreeNodes, id) ?? []), id];
		query = '';
	}

	function onFileClick() {
		if (!docTreeModal.docked || window.matchMedia('(max-width: 1023px)').matches) closeDocTree();
	}

	/** @param {import('$lib/docTreeData.js').DocTreeNode} node */
	function isActive(node) {
		return node.type === 'file' && sameDocPage(node.href, currentHref);
	}

	$effect(() => {
		if (!docTreeModal.open) {
			query = '';
			docTreeModal.focusDirId = null;
			return;
		}
		const focus = docTreeModal.focusDirId;
		folderPath = focus === '__root__' ? [] : focus
			? [...(ancestorIdsForDirId(docTreeNodes, focus) ?? []), focus]
			: ancestorIdsForHref(docTreeNodes, resolveDocHref(currentHref)) ?? [];
	});
</script>

{#snippet toolbar()}
	<div class="doc-tree-toolbar">
		<label class="search">
			<span class="icon" aria-hidden="true">{@html magnifyingGlass}</span>
			<input
				type="search"
				placeholder="Filter documentation…"
				bind:value={query}
				aria-label="Search documentation"
			/>
		</label>

	</div>
{/snippet}

{#snippet footerControls()}
	<button class="panel-action" aria-label={`Color theme: ${theme.mode}. Switch to ${theme.mode === 'system' ? 'dark' : theme.mode === 'dark' ? 'light' : 'system'} mode`} title={`Theme: ${theme.mode}`} onclick={() => setTheme(theme.mode === 'system' ? 'dark' : theme.mode === 'dark' ? 'light' : 'system')}>
		<span aria-hidden="true">{@html theme.mode === 'system' ? systemIcon : theme.mode === 'dark' ? moonIcon : sunIcon}</span>
	</button>
	<button class="panel-action hide-sidebar" aria-label="Hide documentation sidebar" title="Hide sidebar" onclick={closeDocTree}><span aria-hidden="true">{@html hideIcon}</span></button>
	<button class="panel-action" aria-label="Enter focus mode" title="Focus mode (Escape to exit)" onclick={() => setDocFocus(true)}><span aria-hidden="true">{@html focusIcon}</span></button>
{/snippet}

<Modal
	bind:open={docTreeModal.open}
	bind:docked={docTreeModal.docked}
	title={docTreeModal.docked ? undefined : 'Documentation'}
	hideClose={docTreeModal.docked}
	controlsBottom={docTreeModal.docked}
	sidebarLayout
	bind:sidebarWidth={docTreeModal.width}
	maxWidth="var(--modal-max-width-lg)"
	height="var(--modal-height-lg)"
	label="Documentation"
	{toolbar}
	{footerControls}
>
	<nav class="doc-tree" aria-label="Documentation pages">
		{#if currentFolder}
			<button class="folder-back" onclick={leaveFolder}>
				<span aria-hidden="true">‹</span> {currentFolder.label}
			</button>
		{/if}
		{#key currentFolder?.id ?? 'root'}
		<div class="folder-content" style={`--slide-direction: ${direction}`} use:resetFolderScroll>
		{#if visibleNodes.length}
			<ul class="tree-root">
				{#each visibleNodes as node (node.id)}
					{@render treeNode(node, 0)}
				{/each}
			</ul>
		{:else}
			<p class="empty">No documentation matches your search.</p>
		{/if}
		</div>
		{/key}
	</nav>
</Modal>

{#snippet treeNode(node, depth)}
	<li class="node" class:dir={node.type === 'dir'}>
		{#if node.type === 'dir'}
			<button type="button" class="folder-row" onclick={() => enterFolder(node.id)} aria-label={`Open ${node.label}`}>
				<span>{node.label}</span>
				<span class="icon" aria-hidden="true">{@html chevronRight}</span>
			</button>
		{:else}
			<a href={node.href} class="file-link" class:active={isActive(node)} aria-current={isActive(node) ? 'page' : undefined} onclick={onFileClick}>
				{node.label}
			</a>
		{/if}
	</li>
{/snippet}

<style>
	.hide-sidebar { order: -1; margin-right: auto; }
	.panel-action { display: grid; place-items: center; width: 1.9rem; height: 1.9rem; border: 0; border-radius: 50%; background: var(--color-surface-raised); color: var(--color-text-muted); cursor: pointer; }
	.panel-action:hover { color: var(--color-text); background: var(--color-hover); }
	.panel-action :global(svg) { display: block; width: 1rem; height: 1rem; }

	.doc-tree-toolbar {
		display: flex;
		align-items: center;
		gap: 0.6rem;
	}

	.search {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		flex: 1;
		min-width: 0;
		padding: 0.4rem 0.65rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		background: var(--color-surface);
		color: var(--color-modal-text-faint);
	}

	.search:focus-within {
		border-color: var(--color-border-strong);
		color: var(--color-modal-text);
	}

	.search input {
		width: 100%;
		min-width: 0;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-modal-text);
		font: inherit;
		font-size: 0.8rem;
		line-height: 1.3;
		outline: none;
	}

	.search input::placeholder {
		color: var(--color-modal-text-faint);
	}

	.folder-row { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 0.4rem 0.45rem; border: 0; border-radius: 0.25rem; background: transparent; color: var(--color-text); font: inherit; font-size: 0.8rem; text-align: left; cursor: pointer; }
	.folder-row:hover, .folder-back:hover { background: var(--color-hover); }
	.folder-row .icon :global(svg) { width: 1rem; height: 1rem; }
	.folder-back { position: sticky; top: 0; z-index: 2; flex-shrink: 0; display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; padding: 0.5rem 0.45rem; background: var(--color-bg); border: 0; border-bottom: 1px solid var(--color-border); color: var(--color-text-muted); font: inherit; font-size: 0.8rem; text-align: left; cursor: pointer; }

	.folder-content { animation: folder-enter 180ms ease-out; }
	@keyframes folder-enter {
		from { opacity: 0; transform: translateX(calc(var(--slide-direction) * 12px)); }
		to { opacity: 1; transform: translateX(0); }
	}
	@media (prefers-reduced-motion: reduce) {
		.folder-content { animation: none; }
	}

	.doc-tree {
		display: flex;
		flex-direction: column;
	}

	.node-head {
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.collapse {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: 1.5rem;
		height: 1.5rem;
		padding: 0;
		border: none;
		border-radius: 0.25rem;
		background: transparent;
		color: var(--color-modal-text-faint);
		cursor: pointer;
	}

	.collapse:hover {
		background: var(--color-hover);
		color: var(--color-modal-text);
	}

	.collapse[aria-expanded='true'] .icon :global(svg) {
		transform: rotate(90deg);
	}

	.icon :global(svg) {
		display: block;
		width: 0.85rem;
		height: 0.85rem;
		transition: transform 0.15s ease;
	}

	.search .icon :global(svg) {
		width: 0.9rem;
		height: 0.9rem;
		flex-shrink: 0;
	}

	.file-link {
		display: block;
		padding: 0.2rem 0;
		color: var(--color-modal-text);
		font-size: 0.8rem;
		line-height: 1.4;
		text-decoration: none;
	}

	.dir-label {
		flex: 1;
		min-width: 0;
		padding: 0.2rem 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 0.8rem;
		font-weight: 600;
		line-height: 1.4;
		color: var(--color-modal-text);
		text-align: left;
		cursor: pointer;
	}

	.dir-label:hover {
		color: var(--color-modal-link-hover);
	}

	.tree-root,
	.nested {
		margin: 0.15rem 0 0;
		padding: 0;
		list-style: none;
	}

	.nested {
		margin-left: 0.35rem;
		padding-left: 0.55rem;
		border-left: 1px solid var(--color-border);
	}

	.node.dir + .node,
	.node + .node {
		margin-top: 0.1rem;
	}

	.node .file-link {
		padding-left: 1.85rem;
	}

	.tree-root > .node > .file-link {
		padding-left: 0;
	}

	.empty {
		margin: 0.5rem 0 0;
		font-size: 0.8rem;
		color: var(--color-modal-text-faint);
	}

	.folder-row, .file-link, .tree-root > .node > .file-link, .node .file-link {
		padding: 0.4rem 0.45rem;
		border-radius: 0.25rem;
		font-weight: 400;
		color: var(--color-text-muted);
		line-height: 1.4;
		transition: background-color 160ms ease, color 160ms ease;
	}
	.folder-row:hover { color: var(--color-text); }
	.file-link:hover { color: var(--color-text); background: var(--color-hover); text-decoration: none; }
	.file-link.active { color: var(--color-text); background: var(--color-surface-raised); font-weight: 400; }
	@media (prefers-reduced-motion: reduce) {
		.folder-row, .file-link { transition: none; }
	}
</style>
