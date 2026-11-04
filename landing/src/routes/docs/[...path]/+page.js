import { error } from '@sveltejs/kit';
import { loadDocModule } from '$lib/docModules.js';
import {
	docTreeNodes,
	findDirById,
	getAllDirRoutes,
	hrefForDirId,
	listingEntriesForDir
} from '$lib/docTreeData.js';
import { hrefFromRoute, isTxtRoute } from '$lib/docs.js';
import { dirMetaForDir, dirSeoData, docMetaForRoute, docSeoData } from '$lib/docPageMeta.js';
import { getAllDocRoutes, loadRawDoc } from '$lib/rawDocModules.js';
import { renderMarkdown } from '$lib/renderMarkdown.js';
import { renderPlainText } from '$lib/renderPlainText.js';

export const prerender = true;

export function entries() {
	const docRoutes = new Set(getAllDocRoutes());
	const dirRoutes = getAllDirRoutes().filter((path) => !docRoutes.has(path));
	return [
		...docRoutes.values(),
		...dirRoutes
	].map((path) => ({ path }));
}

/** @type {import('./$types').PageLoad} */
export async function load({ params }) {
	const route = params.path;
	const compiled = await loadDocModule(route);

	if (compiled) {
		const meta = await docMetaForRoute(route);
		const href = hrefFromRoute(route);
		return {
			kind: 'doc',
			href,
			title: meta.title,
			description: meta.description,
			...docSeoData({ route, href, ...meta }),
			Content: compiled.default,
			html: null
		};
	}

	const markdown = await loadRawDoc(route);
	if (!markdown) {
		const dir = findDirById(docTreeNodes, route);
		if (dir) {
			const meta = await dirMetaForDir(dir);
			const href = hrefForDirId(route);
			return {
				kind: 'dir',
				href,
				title: meta.title,
				description: meta.description,
				...dirSeoData(dir, href, meta.title, meta.description),
				entries: listingEntriesForDir(dir)
			};
		}

		error(404, 'Documentation page not found');
	}

	const meta = await docMetaForRoute(route);
	const href = hrefFromRoute(route);

	return {
		kind: 'doc',
		href,
		title: meta.title,
		description: meta.description,
		...docSeoData({ route, href, ...meta }),
		Content: null,
		html: isTxtRoute(route) ? renderPlainText(markdown, route) : renderMarkdown(markdown, route)
	};
}
