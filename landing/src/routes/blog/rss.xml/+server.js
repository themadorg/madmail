import { posts } from '$lib/blog.js';
import { siteName, siteOrigin } from '$lib/siteMeta.js';

export const prerender = true;

/** @param {string} value */
function escapeXml(value) {
	return value
		.replaceAll('&', '&amp;')
		.replaceAll('<', '&lt;')
		.replaceAll('>', '&gt;')
		.replaceAll('"', '&quot;')
		.replaceAll("'", '&apos;');
}

/** @type {import('./$types').RequestHandler} */
export function GET() {
	const items = posts
		.map((post) => {
			const link = `${siteOrigin}/blog/${post.slug}`;
			return `  <item>
    <title>${escapeXml(post.title)}</title>
    <link>${link}</link>
    <guid>${link}</guid>
    <pubDate>${new Date(`${post.date}T00:00:00Z`).toUTCString()}</pubDate>
    <description>${escapeXml(post.description)}</description>
  </item>`;
		})
		.join('\n');

	const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
  <title>${escapeXml(siteName)} blog</title>
  <link>${siteOrigin}/blog</link>
  <description>Notes from the Madmail project.</description>
${items}
</channel>
</rss>
`;

	return new Response(xml, {
		headers: {
			'Content-Type': 'application/rss+xml; charset=utf-8'
		}
	});
}
