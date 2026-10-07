<script>
	import { onMount, tick } from 'svelte';
	import DocHeader from '$lib/components/DocHeader.svelte';
	import { renderReleaseMarkdown } from '$lib/releaseMarkdown.js';
	import { releases as releaseSnapshot } from '$lib/releases.js';
	import { repo } from '$lib/nav.js';
	import SiteFooter from '$lib/components/SiteFooter.svelte';

	let { selectedRelease = null } = $props();
	const releases = selectedRelease ? [selectedRelease] : releaseSnapshot;
	const loading = false;
	const failed = false;
	let activeVersion = $state('');

	onMount(() => {
		let disposed = false;
		let cleanup = () => {};
		Promise.resolve(releases)
			.then(async (result) => {
				if (disposed) return;
				await tick();
				if (disposed) return;

				const articles = result.map((release) => document.getElementById(release.tag_name));
				let frame = 0;
				function updateActiveVersion() {
					frame = 0;
					const offset = window.innerWidth <= 700 ? 140 : 96;
					let current = articles[0];
					for (const article of articles) {
						if (article && article.getBoundingClientRect().top <= offset) current = article;
					}
					activeVersion = current?.id ?? '';
				}
				function scheduleUpdate() {
					if (!frame) frame = requestAnimationFrame(updateActiveVersion);
				}
				const target = articles.find((article) => `#${article?.id}` === window.location.hash);
				target?.scrollIntoView({ behavior: 'instant', block: 'start' });
				updateActiveVersion();
				window.addEventListener('scroll', scheduleUpdate, { passive: true });
				window.addEventListener('resize', scheduleUpdate);
				cleanup = () => {
					window.removeEventListener('scroll', scheduleUpdate);
					window.removeEventListener('resize', scheduleUpdate);
					cancelAnimationFrame(frame);
				};
			});
		return () => { disposed = true; cleanup(); };
	});
</script>

<svelte:head>
	<title>{selectedRelease ? `${selectedRelease.tag_name} · madmail` : 'Releases · madmail'}</title>
	<meta name="description" content="Latest madmail releases, release notes, and downloads." />
</svelte:head>

<DocHeader section="Releases" />

<div class="release-content">
<main>
	<div class="page-heading">
		<h1>{selectedRelease ? selectedRelease.tag_name : 'Releases'}</h1>
		<p>What’s new, what’s improved, and everything you need to upgrade.</p>
	</div>
	{#if loading}
		<p role="status">Loading releases…</p>
	{:else if failed}
		<p role="status">Release information is temporarily unavailable. <a href="{repo}/releases">View releases on GitHub</a>.</p>
	{:else if releases.length === 0}
		<p>No releases available yet.</p>
	{:else}
		<div class="release-layout" class:single={selectedRelease}>
			{#if !selectedRelease}<aside>
				<nav class="versions" aria-label="Release versions">
					<h2>Versions</h2>
					{#each releases as release, index (release.id)}
						<a href={`#${release.tag_name}`} class:active={activeVersion === release.tag_name} aria-current={activeVersion === release.tag_name ? 'location' : undefined} onclick={(event) => {
							event.preventDefault();
							document.getElementById(release.tag_name)?.scrollIntoView({
								behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth',
								block: 'start'
							});
							history.replaceState(history.state, '', `#${release.tag_name}`);
						}}>{release.tag_name}{#if index === 0}<span>Latest</span>{/if}</a>
					{/each}
				</nav>
			</aside>{/if}
			<div class="release-content">
		{#each releases as release, index (release.id)}
			<article id={release.tag_name}>
				{#if index === 0 && !selectedRelease}<p class="latest">Latest release</p>{/if}
				<h2><a class="release-title" href={`/releases/${encodeURIComponent(release.tag_name)}`}>{release.name || release.tag_name}</a></h2>
				<p class="date">{release.tag_name} · <time datetime={release.published_at}>{release.published_at.slice(0, 10)}</time></p>
				{#if release.body}<div class="notes">{@html renderReleaseMarkdown(release.body)}</div>{/if}
				{#if release.assets.length}
					<details class="assets" open>
						<summary>Assets <span>{release.assets.length}</span></summary>
					<ul>
						{#each release.assets as asset (asset.id)}
							<li>
								<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="18" height="18" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="m7.5 4.27 9 5.14M21 8.25l-9 5.25m0 0L3 8.25m9 5.25V21m9-12.75v7.5a2.25 2.25 0 0 1-1.13 1.95l-6.75 3.86a2.25 2.25 0 0 1-2.24 0L4.13 17.7A2.25 2.25 0 0 1 3 15.75v-7.5A2.25 2.25 0 0 1 4.13 6.3l6.75-3.86a2.25 2.25 0 0 1 2.24 0l6.75 3.86A2.25 2.25 0 0 1 21 8.25Z" /></svg>
								<div class="asset-info">
									<a href={asset.browser_download_url}>{asset.name}</a>
									{#if asset.digest}<code class="asset-digest" title={asset.digest}>{asset.digest}</code>{/if}
								</div>
								<span class="asset-size">{asset.size >= 1048576 ? `${(asset.size / 1048576).toFixed(1)} MB` : `${(asset.size / 1024).toFixed(1)} KB`}</span>
							</li>
						{/each}
					</ul>
					</details>
				{/if}
				<a class="github-release" href={release.html_url}>View release on GitHub
					<!-- Heroicons: arrow-top-right-on-square (MIT). -->
					<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true" width="18" height="18">
						<path stroke-linecap="round" stroke-linejoin="round" d="M13.5 6H5.25A2.25 2.25 0 0 0 3 8.25v10.5A2.25 2.25 0 0 0 5.25 21h10.5A2.25 2.25 0 0 0 18 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" />
					</svg>
				</a>
			</article>
		{/each}
			</div>
		</div>
	{/if}
</main>
</div>
<SiteFooter spacious />

<style>
	.site-header { position: sticky; top: 0; z-index: 5; background: var(--color-bg); border-bottom: 1px solid var(--color-border); }
	.header-inner { max-width: 72rem; margin: auto; padding: 0.65rem 2rem; display: flex; align-items: center; justify-content: space-between; gap: 1.5rem; }
	.brand { display: flex; align-items: center; gap: 0.75rem; font-size: 1rem; font-weight: 700; text-decoration: none; }
	.header-inner nav { display: flex; gap: 1.5rem; font-size: 0.9rem; }
	.header-inner nav a { text-decoration: none; color: var(--color-text-muted); }
	.header-inner nav a[aria-current] { color: var(--color-text); }
	.notes :global(.external-link-icon) { display: inline-block; width: 0.75em; height: 0.75em; margin-left: 0.2em; vertical-align: super; }
	.notes { color: var(--docs-text, var(--color-text)); }
	.notes :global(a) { color: var(--docs-link, var(--color-text)); text-underline-offset: 0.18em; }
	.notes :global(a:visited) { color: var(--docs-link-visited, var(--color-text)); }
	.notes :global(a:hover) { color: var(--docs-link-hover, var(--color-text)); }
	.notes :global(h1), .notes :global(h2), .notes :global(h3), .notes :global(h4) { color: var(--docs-heading, var(--color-text)); }
	.notes :global(code) { color: var(--color-code-text); }

	.release-content { position: relative; z-index: 1; background: var(--color-bg); padding-bottom: 3rem; }
	main { max-width: 72rem; margin: 0 auto; padding: 6rem 2rem 0; }
	.page-heading { margin-bottom: 3rem; }
	h1 { font-size: clamp(2.5rem, 5vw, 3.5rem); letter-spacing: -0.04em; margin: 0.5rem 0 1rem; }
	.page-heading > p:last-child { color: var(--color-text-muted); line-height: 1.6; }
	.release-layout { display: grid; grid-template-columns: 12rem minmax(0, 1fr); gap: 4rem; }
	.release-layout.single { display: block; max-width: 52rem; }
	.release-title { text-decoration: none; }
	.release-title:hover { text-decoration: underline; }
	.versions { position: sticky; top: 4rem; max-height: calc(100dvh - 5rem); overflow-y: auto; }
	.versions h2 { font-size: 0.75rem; color: var(--color-text-muted); margin: 0 0 0.5rem; }
	.versions a { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; padding: 0.4rem 0.65rem; border-radius: 0.4rem; text-decoration: none; font-size: 0.8rem; }
	.versions a:focus:not(:focus-visible) { outline: none; }
	.versions a:focus-visible { outline: 2px solid var(--color-text); outline-offset: -2px; }
	.versions a:hover { background: var(--color-hover); }
	.versions a.active { background: var(--color-surface-raised); font-weight: 600; }
	.github-release { display: inline-flex; align-items: center; gap: 0.4rem; }
	.github-release svg { flex-shrink: 0; }
	.versions span { font-size: 0.65rem; color: var(--color-text-muted); }
	article { padding: 0 0 3rem; margin-bottom: 3rem; border-bottom: 1px solid var(--color-border); scroll-margin-top: 5.5rem; }
	h2 { margin: 0 0 0.5rem; overflow-wrap: anywhere; }
	a { color: var(--color-text); }
	.latest, .date { color: var(--color-text-muted); font-size: 0.85rem; }
	.latest { display: inline-block; color: #3fb950; background: rgb(63 185 80 / 0.1); border: 1px solid rgb(63 185 80 / 0.5); padding: 0.25rem 0.6rem; border-radius: 2rem; margin: 0 0 1rem; }
	.notes { overflow-wrap: anywhere; line-height: 1.75; margin: 2rem 0; }
	.notes :global(h1), .notes :global(h2), .notes :global(h3), .notes :global(h4) { margin: 2rem 0 0.75rem; line-height: 1.3; font-size: 1.25rem; }
	.notes :global(p) { margin: 1rem 0; }
	.notes :global(li) { margin: 0.5rem 0; }
	.notes :global(pre) { overflow-x: auto; padding: 1rem; background: var(--color-surface-code); border-radius: 0.5rem; }
	.notes :global(code) { font-size: 0.85em; background: var(--color-surface-code); padding: 0.15em 0.3em; border-radius: 0.2rem; }
	.notes :global(pre code) { padding: 0; }
	.notes :global(blockquote) { border-left: 2px solid var(--color-border-strong); margin-left: 0; padding-left: 1rem; color: var(--color-text-muted); }
	.notes :global(img) { max-width: 100%; height: auto; }
	.notes :global(table) { display: block; max-width: 100%; overflow-x: auto; border-collapse: collapse; }
	.notes :global(th), .notes :global(td) { border: 1px solid var(--color-border); padding: 0.5rem; }
	.assets { margin: 2rem 0 1.5rem; border: 1px solid var(--color-border); border-radius: 0.5rem; overflow: hidden; }
	.assets summary { cursor: pointer; padding: 0.9rem 1rem; background: var(--color-surface); font-weight: 600; }
	.assets summary span { margin-left: 0.5rem; padding: 0.1rem 0.45rem; border-radius: 1rem; background: var(--color-surface-raised); font-size: 0.8rem; }
	.assets ul { list-style: none; margin: 0; padding: 0; }
	.assets li { display: flex; align-items: center; gap: 0.75rem; padding: 0.75rem 1rem; border-top: 1px solid var(--color-border); font-size: 0.85rem; }
	.assets li svg { flex-shrink: 0; color: var(--color-text-muted); }
	.assets li a { min-width: 0; text-decoration: none; overflow-wrap: anywhere; }
	.assets li a:hover { text-decoration: underline; }
	.asset-info { flex: 1; min-width: 0; }
	.asset-digest { display: block; margin-top: 0.35rem; font-size: 0.65rem; color: var(--color-text-muted); overflow-wrap: anywhere; user-select: all; line-height: 1.5; }
	.asset-size { margin-left: auto; flex-shrink: 0; color: var(--color-text-muted); font-size: 0.75rem; }
	ul { padding-left: 1.25rem; line-height: 1.8; overflow-wrap: anywhere; }
	@media (max-width: 700px) {
		.header-inner { padding: 0.65rem 1rem; }
		.header-inner nav { gap: 1rem; font-size: 0.8rem; }
		main { padding: 5rem 1rem 0; }
		.release-layout { grid-template-columns: 1fr; gap: 2rem; }
		aside { position: sticky; top: 3.25rem; z-index: 1; background: var(--color-bg); border-bottom: 1px solid var(--color-border); padding: 0.75rem 0; }
		.versions { position: static; display: flex; overflow-x: auto; max-height: none; }
		.versions h2 { display: none; }
		.versions a { flex-shrink: 0; }
		article { scroll-margin-top: 8rem; }
	}
</style>
