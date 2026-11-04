import { buildLlmsTxt } from '$lib/llmsText.js';

export const prerender = true;

/** @type {import('./$types').RequestHandler} */
export async function GET() {
	return new Response(await buildLlmsTxt(), {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' }
	});
}
