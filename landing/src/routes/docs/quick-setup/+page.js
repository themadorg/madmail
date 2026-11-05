import { error } from '@sveltejs/kit';
import { loadDocModule } from '$lib/docModules.js';
import { docMetaForRoute, docSeoData } from '$lib/docPageMeta.js';
import { docLanguagesForRoute, docVariantRoute } from '$lib/docLanguages.js';
import { getAllDocRoutes, loadRawDoc } from '$lib/rawDocModules.js';
import { renderMarkdown } from '$lib/renderMarkdown.js';

export const prerender = true;

const BASE_ROUTE = 'project/user-guide/02-quick-start';
const HREF = '/docs/quick-setup';

/** @type {import('./$types').PageLoad} */
export async function load({ params }) {
	const routes = getAllDocRoutes();
	const route = params.locale === 'fa' ? docVariantRoute(BASE_ROUTE, 'fa', routes) ?? BASE_ROUTE : BASE_ROUTE;
	const languages = docLanguagesForRoute(route, routes);
	const compiled = await loadDocModule(route);
	const meta = await docMetaForRoute(route);
	const seo = docSeoData({ route, href: HREF, ...meta });

	if (compiled) {
		return {
			href: HREF,
			...languages,
			title: meta.title,
			description: meta.description,
			...seo,
			Content: compiled.default,
			html: null
		};
	}

	const markdown = await loadRawDoc(route);
	if (!markdown) {
		error(404, 'Documentation page not found');
	}

	return {
		href: HREF,
		...languages,
		title: meta.title,
		description: meta.description,
		...seo,
		Content: null,
		html: renderMarkdown(markdown, route)
	};
}
