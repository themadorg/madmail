<script>
	import { page } from '$app/state';
	import {
		defaultDescription,
		defaultTitle,
		ogImageHeight,
		ogImageUrl,
		ogImageWidth,
		siteName,
		siteOrigin
	} from '$lib/siteMeta.js';

	const title = $derived.by(() => {
		if (page.status >= 400) {
			return page.status === 404 ? '404 · Madmail' : `${page.status} · Madmail`;
		}

		if (page.data?.title) {
			return `${page.data.title} · Madmail`;
		}

		return defaultTitle;
	});

	const description = $derived(page.data?.description ?? defaultDescription);
	const canonicalUrl = $derived(`${siteOrigin}${page.url.pathname}`);
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<link rel="canonical" href={canonicalUrl} />

	<meta property="og:type" content={page.data?.article ? 'article' : 'website'} />
	{#if page.data?.article}
		<meta property="article:published_time" content={`${page.data.published}T00:00:00Z`} />
		<meta property="article:author" content={`https://github.com/${encodeURIComponent(page.data.author)}`} />
		<meta property="og:locale" content={page.data.language === 'fa' ? 'fa_IR' : page.data.language === 'ru' ? 'ru_RU' : 'en_US'} />
	{/if}
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
</svelte:head>
