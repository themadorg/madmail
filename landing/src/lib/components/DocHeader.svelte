<script>
	import HeaderDropdown from '$lib/components/HeaderDropdown.svelte';
	import SharePageModal from '$lib/components/SharePageModal.svelte';
	import MadmailLogo from '$lib/components/MadmailLogo.svelte';
	import { openCommandPalette } from '$lib/commandPalette.svelte.js';
	import { docTreeModal, openDocTree } from '$lib/docTreeModal.svelte.js';
	import shareIcon from '$lib/icons/share.svg?raw';
	import github from '$lib/icons/github.svg?raw';
	import magnifyingGlass from '$lib/icons/magnifying-glass.svg?raw';
	import queueList from '$lib/icons/queue-list.svg?raw';
	import chevronDown from '$lib/icons/chevron-down.svg?raw';
	import languageIcon from '$lib/icons/language.svg?raw';
	import bookOpen from '$lib/icons/book-open.svg?raw';
	import tagIcon from '$lib/icons/tag.svg?raw';
	import newspaper from '$lib/icons/newspaper.svg?raw';
	import { repo } from '$lib/nav.js';

	/** @type {{ article?: ReturnType<typeof import("$lib/blogSource.js").parseBlogSource> | null, currentHref?: string, section?: string, blogLanguage?: string, languages?: { label: string, href: string, current: boolean }[] }} */
	let { currentHref = '', section = 'Docs', languages = [], article = null, blogLanguage = 'en' } = $props();

	const blogHref = $derived(blogLanguage === 'en' ? '/blog' : `/blog?lang=${encodeURIComponent(blogLanguage)}`);

	let shareOpen = $state(false);
	let shareUrl = $state('');
	let shareTitle = $state('');
	function sharePage() {
		shareUrl = window.location.href;
		shareTitle = document.title;
		shareOpen = true;
	}

</script>

<header class="doc-header">
	<div class="brand">
		<div class="brand-start">
			<MadmailLogo href="/" size="1.5rem" class="doc-logo" transitionName="madmail-logo" />
			<div class="docs-label">
				<a href="/">Madmail</a>
				<span aria-hidden="true"> / </span>
				<a href={section === 'Blog' ? blogHref : section === 'Releases' ? '/releases' : '/docs'}>{section}</a>
			</div>
			<HeaderDropdown id="section-navigation" label="Switch section" menuLabel="Site sections" icon={chevronDown} items={[
				{ label: 'Docs', href: '/docs', current: section === 'Docs', icon: bookOpen },
				{ label: 'Releases', href: '/releases', current: section === 'Releases', icon: tagIcon },
				{ label: 'Blog', href: blogHref, current: section === 'Blog', icon: newspaper }
			]} />
			{#if section === 'Docs' && !(docTreeModal.open && docTreeModal.docked)}
			<button
				type="button"
				class="docs-tree"
				aria-label="Documentation tree"
				onclick={openDocTree}
			>
				<span class="icon" aria-hidden="true">{@html queueList}</span>
			</button>
			{/if}
		</div>
		<button type="button" class="search-bar" aria-label="Search pages" onclick={() => openCommandPalette()}>
			<span class="icon" aria-hidden="true">{@html magnifyingGlass}</span>
			<span>Search documentation…</span>
			<kbd>Ctrl K</kbd>
		</button>
		<div class="brand-end">
			{#if languages.length}
				<HeaderDropdown id="post-languages" label="Change post language" menuLabel="Post languages" icon={languageIcon} align="right" items={languages.map((language) => ({ ...language, lang: language.label === 'فارسی' ? 'fa' : language.label === 'Русский' ? 'ru' : language.label === 'English' ? 'en' : undefined }))} />
			{/if}
			<button
				type="button"
				class="search"
				aria-label="Search pages"
				onclick={() => openCommandPalette()}
			>
				<span class="icon" aria-hidden="true">{@html magnifyingGlass}</span>
			</button>
			<button type="button" class="share" aria-label="Share page" title="Share page" onclick={sharePage}>
				<span class="icon" aria-hidden="true">{@html shareIcon}</span>
			</button>
			<a
				href={repo}
				class="github"
				aria-label="GitHub repository"
				target="_blank"
				rel="noopener noreferrer"
			>
				<span class="icon" aria-hidden="true">{@html github}</span>
			</a>
		</div>
	</div>
</header>
<SharePageModal bind:open={shareOpen} url={shareUrl} title={shareTitle} {article} />

<style>
	.doc-header {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		height: 3rem;
		z-index: 110;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		max-width: none;
		margin: 0;
		padding: 0 1rem;
		justify-content: center;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg);
		transition: var(--transition-theme);
	}

	@media (max-width: 640px) {
		.doc-header {
			padding: 0 0.75rem;
			margin-bottom: 0;
		}
	}

	.brand {
		display: grid;
		grid-template-columns: 1fr auto;
		align-items: center;
	}

	.docs-label { color: var(--color-text-muted); text-decoration: none; font-size: 0.85rem; }
	.docs-label a { color: inherit; text-decoration: none; }
	.docs-label a:hover { text-decoration: underline; }

	.search-bar { display: none; }
	@media (min-width: 900px) {
		.brand { grid-template-columns: 1fr minmax(16rem, 28rem) 1fr; gap: 1rem; }
		.brand > .brand-end { grid-column: 3; }
		.brand-end .search { display: none; }
		.search-bar { display: flex; align-items: center; gap: 0.6rem; width: 100%; padding: 0.35rem 0.65rem; border: 1px solid transparent; border-radius: 0.4rem; background: transparent; color: var(--color-text-subtle); font: inherit; font-size: 0.8rem; text-align: left; cursor: pointer; transition: background-color 160ms ease; }
		.search-bar:hover { background: var(--color-hover); }
		.search-bar kbd { margin-left: auto; font-size: 0.7rem; border: 1px solid var(--color-border); border-radius: 0.2rem; padding: 0.1rem 0.3rem; opacity: 0.6; }
	}

	.brand-start {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		grid-column: 1;
		justify-self: start;
	}

	.share-status { position: absolute; top: 3.25rem; right: 1rem; padding: 0.4rem 0.65rem; border: 1px solid var(--color-border); border-radius: 0.4rem; background: var(--color-surface); color: var(--color-text-muted); font-size: 0.75rem; }

	.brand-end {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		grid-column: 2;
		justify-self: end;
	}

	.share,
	.docs-tree,
	.search {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 1.9rem;
		padding: 0;
		border: none;
		border-radius: 0.5rem;
		background: transparent;
		color: var(--color-text-subtle);
		cursor: pointer;
	}

	.share:hover,
	.docs-tree:hover,
	.search:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.github {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 1.9rem;
		border-radius: 0.5rem;
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	.github:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.icon :global(svg) {
		display: block;
		width: 1.25rem;
		height: 1.25rem;
	}
</style>
