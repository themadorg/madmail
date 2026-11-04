import { error } from '@sveltejs/kit';
import { loadDocModule } from '$lib/docModules.js';
import { docMetaForRoute, docSeoData } from '$lib/docPageMeta.js';

/** @type {import('./$types').PageLoad} */
export async function load() {
	const mod = await loadDocModule('project/user-guide/README');

	if (!mod) {
		error(404, 'Documentation index not found');
	}

	const meta = await docMetaForRoute('project/user-guide/README');

	return {
		href: '/docs',
		title: meta.title,
		description: meta.description,
		...docSeoData({ route: 'project/user-guide/README', href: '/docs', ...meta }),
		Content: mod.default
	};
}
