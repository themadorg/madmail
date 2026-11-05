import { error } from '@sveltejs/kit';
import { docMarkdown, getMarkdownDocRoutes, markdownResponse } from '$lib/markdownPages.js';

export const prerender = true;

export function entries() {
	return getMarkdownDocRoutes().map((route) => ({ path: `${route}.md` }));
}

/** @type {import('./$types').RequestHandler} */
export async function GET({ params }) {
	const markdown = await docMarkdown(params.path.slice(0, -'.md'.length));
	if (!markdown) error(404, 'Documentation page not found');
	return markdownResponse(markdown);
}
