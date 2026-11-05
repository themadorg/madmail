import { posts } from '$lib/blog.js';
import { homeAlternates } from '$lib/homeData.js';
import { prefixedLangs } from '$lib/i18n/index.js';
import { DOC_HREF_ALIASES, hrefFromRoute } from '$lib/docs.js';
import { getAllDirRoutes, hrefForDirId } from '$lib/docTreeData.js';
import { getAllDocRoutes } from '$lib/rawDocModules.js';
import { releases } from '$lib/releases.js';
import { siteOrigin } from '$lib/siteMeta.js';

/**
 * Canonical public pathnames for the landing site sitemap.
 * Omits redirect-only aliases (.md short links, /docs/features, etc.).
 * @returns {string[]}
 */
export function getSitemapPathnames() {
	const paths = new Set(['/', ...prefixedLangs.map((lang) => `/${lang}`), '/docs', '/docs/quick-setup', '/blog', '/releases', '/phone', '/phone/install', '/phone/faq']);
	for (const post of posts) {
		paths.add(`/blog/${post.slug}`);
	}
	for (const release of releases) {
		paths.add(`/releases/${encodeURIComponent(release.tag_name)}`);
	}

	for (const route of getAllDocRoutes()) {
		paths.add(hrefFromRoute(route));
	}

	const docRoutes = new Set(getAllDocRoutes());
	for (const dirId of getAllDirRoutes()) {
		if (docRoutes.has(dirId)) continue;
		paths.add(hrefForDirId(dirId));
	}

	// Alias targets render the same page as their short URL, which is the canonical one.
	for (const target of Object.values(DOC_HREF_ALIASES)) paths.delete(target);

	return [...paths].sort((a, b) => {
		if (a === '/') return -1;
		if (b === '/') return 1;
		const aHome = homeAlternates.some((alt) => alt.href === a);
		const bHome = homeAlternates.some((alt) => alt.href === b);
		if (aHome !== bHome) return aHome ? -1 : 1;
		return a.localeCompare(b);
	});
}

/**
 * hreflang links between editions of one page: the localized homes, or the translations of a blog post.
 * @param {string} pathname
 */
function alternatesFor(pathname) {
	if (homeAlternates.some((alt) => alt.href === pathname && alt.hreflang !== 'x-default')) return homeAlternates;
	const post = posts.find((candidate) => `/blog/${candidate.slug}` === pathname);
	if (!post) return [];
	const translations = posts.filter((other) => other.translation === post.translation);
	if (translations.length < 2) return [];
	const original = translations.find((other) => other.slug === post.translation) ?? translations[0];
	return [
		...translations.map((other) => ({ hreflang: other.language, href: `/blog/${other.slug}` })),
		{ hreflang: 'x-default', href: `/blog/${original.slug}` }
	];
}

/** @param {string} value */
function escapeXml(value) {
	return value
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;')
		.replaceAll("'", '&apos;');
}

/**
 * Build a sitemaps.org protocol XML document for the landing site.
 * @returns {string}
 */
export function buildSitemapXml() {
	/** @type {Map<string, string>} only real dates; docs have none in a shallow CI checkout */
	const lastmod = new Map();
	for (const post of posts) lastmod.set(`/blog/${post.slug}`, post.date);
	if (posts[0]) lastmod.set('/blog', posts[0].date);
	for (const release of releases) {
		if (release.published_at) lastmod.set(`/releases/${encodeURIComponent(release.tag_name)}`, release.published_at);
	}
	if (releases[0]?.published_at) lastmod.set('/releases', releases[0].published_at);

	const urls = getSitemapPathnames()
		.map((pathname) => {
			const loc = `${siteOrigin}${pathname === '/' ? '/' : pathname}`;
			const lines = [`    <loc>${escapeXml(loc)}</loc>`];
			if (lastmod.has(pathname)) lines.push(`    <lastmod>${lastmod.get(pathname)}</lastmod>`);
			for (const alternate of alternatesFor(pathname)) {
				lines.push(
					`    <xhtml:link rel="alternate" hreflang="${alternate.hreflang}" href="${escapeXml(`${siteOrigin}${alternate.href}`)}"/>`
				);
			}
			return `  <url>\n${lines.join('\n')}\n  </url>`;
		})
		.join('\n');

	return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls}
</urlset>
`;
}
