import { buildSitemapXml } from '$lib/sitemapUrls.js';

export const prerender = true;

/** @type {import('./$types').RequestHandler} */
export function GET() {
	return new Response(buildSitemapXml(), {
		headers: {
			'Content-Type': 'application/xml; charset=utf-8',
			'Cache-Control': 'public, max-age=3600'
		}
	});
}
