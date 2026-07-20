import { hrefFromRoute } from '$lib/docs.js';
import { getAllDirRoutes, hrefForDirId } from '$lib/docTreeData.js';
import { getAllDocRoutes } from '$lib/rawDocModules.js';
import { siteOrigin } from '$lib/siteMeta.js';

/**
 * Canonical public pathnames for the landing site sitemap.
 * Omits redirect-only aliases (.md short links, /docs/features, etc.).
 * @returns {string[]}
 */
export function getSitemapPathnames() {
	const paths = new Set(['/', '/docs', '/docs/quick-setup']);

	for (const route of getAllDocRoutes()) {
		paths.add(hrefFromRoute(route));
	}

	const docRoutes = new Set(getAllDocRoutes());
	for (const dirId of getAllDirRoutes()) {
		if (docRoutes.has(dirId)) continue;
		paths.add(hrefForDirId(dirId));
	}

	return [...paths].sort((a, b) => {
		if (a === '/') return -1;
		if (b === '/') return 1;
		return a.localeCompare(b);
	});
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
	const urls = getSitemapPathnames()
		.map((pathname) => {
			const loc = `${siteOrigin}${pathname === '/' ? '/' : pathname}`;
			return `  <url>\n    <loc>${escapeXml(loc)}</loc>\n  </url>`;
		})
		.join('\n');

	return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls}
</urlset>
`;
}
