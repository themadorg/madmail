import { buildLlmsFullTxt } from '$lib/llmsText.js';

export const prerender = true;

/** @type {import('./$types').RequestHandler} */
export async function GET() {
	return new Response(await buildLlmsFullTxt(), {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' }
	});
}
