<script>
	import { page } from '$app/state';
	import DocHeader from '$lib/components/DocHeader.svelte';
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import { renderReleaseMarkdown } from '$lib/releaseMarkdown.js';
	import { siteOrigin } from '$lib/siteMeta.js';
	import { posts as allPosts } from '$lib/blog.js';

	/** @type {{ posts: { slug: string, title: string, date: string, author?: string, language?: string, translation?: string, description: string, body: string }[], selected?: { slug: string, title: string, date: string, author?: string, language?: string, translation?: string, description: string, body: string } | null }} */
	let { posts, selected = null } = $props();
	const indexLanguage = $derived(['en', 'fa', 'ru'].includes(page.url.searchParams.get('lang')) ? page.url.searchParams.get('lang') : 'en');
	const groups = $derived([...new Set(posts.map((post) => post.translation))].map((key) => {
		const variants = posts.filter((post) => post.translation === key);
		return variants.find((post) => post.language === indexLanguage) ?? variants.find((post) => post.language === 'en') ?? variants[0];
	}));
	const visible = $derived(selected ? [selected] : groups);
	const languages = $derived(selected ? allPosts.filter((post) => post.translation === selected.translation).map((post) => ({ label: post.language === 'fa' ? 'فارسی' : post.language === 'en' ? 'English' : post.language === 'ru' ? 'Русский' : post.language, href: `/blog/${post.slug}`, current: post.slug === selected.slug })) : [...new Set(allPosts.map((post) => post.language))].map((language) => ({ label: language === 'fa' ? 'فارسی' : language === 'en' ? 'English' : language === 'ru' ? 'Русский' : language, href: `/blog?lang=${language}`, current: language === indexLanguage })));
</script>

<svelte:head>
	<title>{selected ? `${selected.title} · madmail` : 'Blog · madmail'}</title>
	<meta
		name="description"
		content={selected?.description || 'Notes from the Madmail project.'}
	/>
	<link rel="alternate" type="application/rss+xml" title="Madmail blog" href="/blog/rss.xml" />
</svelte:head>

<DocHeader section="Blog" {languages} article={selected} blogLanguage={selected?.language ?? indexLanguage} />

<div class="blog-page">
	<header class="page-cover">
		<div class="page-heading" lang={selected?.language ?? indexLanguage} dir={(selected?.language ?? indexLanguage) === 'fa' ? 'rtl' : undefined}>
			{#if selected}
				<h1>{selected.title}</h1>
				<p class="post-meta">
					<time datetime={selected.date}>{selected.date}</time>
					{#if selected.author}
						<svg class="meta-dot" viewBox="0 0 8 8" aria-hidden="true"><circle cx="4" cy="4" r="2" fill="currentColor" /></svg>
						<a href="https://github.com/{encodeURIComponent(selected.author)}">{selected.author}</a>
					{/if}
					<svg class="meta-dot" viewBox="0 0 8 8" aria-hidden="true"><circle cx="4" cy="4" r="2" fill="currentColor" /></svg>
					<a href="/blog/{selected.slug}/source.md">Source</a>
				</p>
			{:else}
				<h1>{indexLanguage === 'fa' ? 'وبلاگ' : indexLanguage === 'ru' ? 'Блог' : 'Blog'}</h1>
				<p>{indexLanguage === 'fa' ? 'یادداشت‌های پروژهٔ مدمیل.' : indexLanguage === 'ru' ? 'Заметки о проекте Madmail.' : 'Notes from the Madmail project.'} <a href="/blog/rss.xml">RSS</a></p>
			{/if}
		</div>
	</header>
	<main>
		{#if visible.length === 0}
			<p>No posts yet.</p>
		{:else}
			{#each visible as post (post.slug)}
				{#if !selected}
					<div class="post-entry">
					<a lang={post.language} dir={post.language === 'fa' ? 'rtl' : undefined} class="post-card" href="/blog/{post.slug}" aria-labelledby="post-title-{post.slug}">
						<h2 id="post-title-{post.slug}">{post.title}</h2>
						<p class="date"><time datetime={post.date}>{post.date}</time></p>
						{#if post.description}<p>{post.description}</p>{/if}
					</a>
					<div class="language-tags" aria-label="Available translations">
						{#each allPosts.filter((variant) => variant.translation === post.translation) as variant}
							<a href="/blog/{variant.slug}" lang={variant.language}>{variant.language.toUpperCase()}</a>
						{/each}
					</div>
					</div>
				{:else}
					<article>
						<div class="notes" lang={post.language} dir={post.language === 'fa' ? 'rtl' : undefined}>{@html renderReleaseMarkdown(post.body, { baseUrl: `${siteOrigin}/blog/${post.slug}`, internalLinks: true, wrapTables: true })}</div>
					</article>
				{/if}
			{/each}
		{/if}
	</main>
	<SiteFooter />
</div>

<style>
	.blog-page {
		min-height: calc(100vh - 3rem);
		display: flex;
		flex-direction: column;
		margin-top: 3rem;
		position: relative;
		z-index: 1;
		background: var(--color-bg);
	}
	main {
		flex: 1;
		width: 100%;
		max-width: 48rem;
		margin: 0 auto;
		padding: 2.5rem 2rem 0;
	}
	.page-cover {
		min-height: clamp(9rem, 20vw, 15rem);
		background: linear-gradient(
			135deg,
			color-mix(in srgb, var(--color-surface) 75%, #6b8afd),
			color-mix(in srgb, var(--color-surface) 92%, #6b8afd)
		);
	}
	.page-heading {
		max-width: 48rem;
		margin: 0 auto;
		padding: 4rem 2rem;
	}
	h1 {
		font-size: clamp(2rem, 4vw, 2.75rem);
		letter-spacing: -0.04em;
		margin: 0.5rem 0 1rem;
	}
	.page-heading > p:last-child,
	.date {
		color: var(--color-text-muted);
		line-height: 1.6;
	}
	.page-heading > p:last-child {
		margin-bottom: 0;
	}
	.post-meta {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0.65rem;
	}
	.meta-dot {
		width: 0.5rem;
		height: 0.5rem;
		flex-shrink: 0;
	}
	a {
		color: var(--color-text);
	}
	article {
		padding: 0 0 2.5rem;
		margin-bottom: 2.5rem;
		border-bottom: 1px solid var(--color-border);
	}
	h2 {
		margin: 0 0 0.5rem;
	}
	.post-entry {
		position: relative;
		margin-bottom: 1rem;
		padding: 1rem;
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		background: var(--color-surface);
		transition: background-color 250ms ease, border-color 250ms ease;
	}
	.post-entry:hover {
		background: var(--color-surface-raised);
		border-color: var(--color-border-strong);
	}
	.post-card::after { content: ''; position: absolute; inset: 0; border-radius: 0.75rem; }
	.language-tags { position: relative; display: flex; gap: 0.4rem; margin-top: 0.75rem; width: fit-content; }
	.language-tags a { padding: 0.2rem 0.4rem; border: 1px solid var(--color-border); border-radius: 0.3rem; font-size: 0.7rem; text-decoration: none; }
	.post-card {
		display: block;
		margin-bottom: 0;
		text-decoration: none;
	}
	.post-card h2 {
		font-size: 1.1rem;
	}
	.post-card p {
		font-size: 0.875rem;
		line-height: 1.65;
		margin: 0.5rem 0 0;
	}
	.post-card .date {
		font-size: 0.75rem;
	}
	@media (prefers-reduced-motion: reduce) {
		.post-entry { transition: none; }
	}
	.notes {
		overflow-wrap: anywhere;
		line-height: 1.75;
	}
	.notes :global(.external-link-icon) {
		display: inline-block;
		width: 0.75em;
		height: 0.75em;
		margin-left: 0.2em;
		vertical-align: super;
	}
	.notes :global(h2),
	.notes :global(h3) {
		margin: 2rem 0 0.75rem;
		line-height: 1.3;
	}
	.notes :global(p),
	.notes :global(ul),
	.notes :global(ol) {
		margin: 1rem 0;
	}
	.notes :global(.table-scroll) {
		margin: 1.5rem 0;
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: 0.65rem;
		background: var(--color-surface);
	}
	.notes :global(.table-scroll:focus-visible) {
		outline: 2px solid var(--color-text-muted);
		outline-offset: 3px;
	}
	.notes :global(table) {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.95em;
		line-height: 1.5;
	}
	.notes :global(th),
	.notes :global(td) {
		padding: 0.85rem 1rem;
		text-align: start;
		vertical-align: top;
		min-width: 9rem;
	}
	.notes :global(th) {
		background: var(--color-surface-raised);
		font-weight: 600;
	}
	.notes :global(tbody tr) {
		border-top: 1px solid var(--color-border);
	}
	.notes :global(pre) {
		overflow-x: auto;
		padding: 1rem;
		background: var(--color-surface-code);
		border-radius: 0.5rem;
	}
	.notes :global(code) {
		font-size: 0.85em;
		background: var(--color-surface-code);
		padding: 0.15em 0.3em;
		border-radius: 0.2rem;
	}
	.notes :global(pre code) {
		padding: 0;
	}
	.notes :global(blockquote) {
		border-left: 2px solid var(--color-border-strong);
		margin-left: 0;
		padding-left: 1rem;
		color: var(--color-text-muted);
	}
	.notes :global(img) {
		max-width: 100%;
		height: auto;
	}
	@media (max-width: 700px) {
		.page-heading {
			padding: 3rem 1rem;
		}
		main {
			padding: 2rem 1rem 0;
		}
	}
</style>
