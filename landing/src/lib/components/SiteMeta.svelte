<script>
	import { page } from '$app/state';
	import {
		defaultDescription,
		defaultTitle,
		ogImageHeight,
		ogImageUrl,
		ogImageWidth,
		ogLocales,
		siteName,
		siteOrigin
	} from '$lib/siteMeta.js';
	import { serializeJsonLd } from '$lib/structuredData.js';

	const isError = $derived(page.status >= 400);

	const title = $derived.by(() => {
		if (isError) {
			return page.status === 404 ? '404 · Madmail' : `${page.status} · Madmail`;
		}

		if (page.data?.metaTitle || page.data?.title) {
			return `${page.data.metaTitle ?? page.data.title} · Madmail`;
		}

		return defaultTitle;
	});

	const description = $derived(page.data?.description || defaultDescription);
	// Pages that render the same content as another URL point their canonical at it.
	const canonicalUrl = $derived(`${siteOrigin}${page.data?.canonical ?? page.url.pathname}`);
	const locale = $derived(ogLocales[page.data?.language ?? 'en'] ?? 'en_US');
	/** @type {{ hreflang: string, href: string }[]} */
	const alternates = $derived(isError ? [] : (page.data?.alternates ?? []));
	const markdownHref = $derived(isError ? null : (page.data?.markdownHref ?? null));
	const jsonLd = $derived(isError || !page.data?.jsonLd ? '' : serializeJsonLd(page.data.jsonLd));
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	{#if isError}
		<meta name="robots" content="noindex" />
	{:else}
		<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1, max-video-preview:-1" />
		<link rel="canonical" href={canonicalUrl} />
	{/if}
	<meta name="author" content={page.data?.author || siteName} />

	{#each alternates as alternate (alternate.hreflang)}
		<link rel="alternate" hreflang={alternate.hreflang} href={`${siteOrigin}${alternate.href}`} />
	{/each}
	{#if markdownHref}
		<link rel="alternate" type="text/markdown" title="Markdown" href={markdownHref} />
	{/if}
	<link rel="alternate" type="application/rss+xml" title="Madmail blog" href="/blog/rss.xml" />
	<link rel="alternate" type="text/plain" title="llms.txt" href="/llms.txt" />

	<meta property="og:type" content={page.data?.article ? 'article' : 'website'} />
	{#if page.data?.article}
		<meta property="article:published_time" content={`${page.data.published}T00:00:00Z`} />
		<meta property="article:author" content={`https://github.com/${encodeURIComponent(page.data.author)}`} />
	{/if}
	<meta property="og:locale" content={locale} />
	<meta property="og:site_name" content={siteName} />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:url" content={canonicalUrl} />
	<meta property="og:image" content={ogImageUrl} />
	<meta property="og:image:width" content={String(ogImageWidth)} />
	<meta property="og:image:height" content={String(ogImageHeight)} />
	<meta property="og:image:alt" content="Madmail logo and tagline" />

	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content={ogImageUrl} />
	<meta name="twitter:image:alt" content="Madmail logo and tagline" />

	{#if jsonLd}
		{@html `<script type="application/ld+json">${jsonLd}</script>`}
	{/if}
</svelte:head>
