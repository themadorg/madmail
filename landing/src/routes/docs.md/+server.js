import { error } from '@sveltejs/kit';
import { docMarkdown, markdownResponse } from '$lib/markdownPages.js';

export const prerender = true;

/** @type {import('./$types').RequestHandler} */
export async function GET() {
	const markdown = await docMarkdown('project/user-guide/README');
	if (!markdown) error(404, 'Documentation index not found');
	return markdownResponse(markdown);
}
