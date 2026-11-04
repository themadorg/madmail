import { error } from '@sveltejs/kit';
import { loadDocModule } from '$lib/docModules.js';
import { docMetaForRoute, docSeoData } from '$lib/docPageMeta.js';
import { loadRawDoc } from '$lib/rawDocModules.js';
import { renderMarkdown } from '$lib/renderMarkdown.js';

export const prerender = true;

const ROUTE = 'project/user-guide/02-quick-start';
const HREF = '/docs/quick-setup';

/** @type {import('./$types').PageLoad} */
export async function load() {
	const compiled = await loadDocModule(ROUTE);
	const meta = await docMetaForRoute(ROUTE);
	const seo = docSeoData({ route: ROUTE, href: HREF, ...meta });

	if (compiled) {
		return {
			href: HREF,
			title: meta.title,
			description: meta.description,
			...seo,
			Content: compiled.default,
			html: null
		};
	}

	const markdown = await loadRawDoc(ROUTE);
	if (!markdown) {
		error(404, 'Documentation page not found');
	}

	return {
		href: HREF,
		title: meta.title,
		description: meta.description,
		...seo,
		Content: null,
		html: renderMarkdown(markdown, ROUTE)
	};
}
