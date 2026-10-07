import { isTxtRoute } from './docs.js';

/** @type {Record<string, () => Promise<string>>} */
export const rawDocLoaders = {
	...import.meta.glob('/src/lib/content/docs/**/*.md', { query: '?raw', import: 'default' }),
	'/src/lib/content/docs/external/README.md': () => import('../../../external/README.md?raw').then((module) => module.default)
};

/** @type {Record<string, () => Promise<string>>} */
export const rawTxtLoaders = import.meta.glob('/src/lib/content/docs/**/*.txt', {
	query: '?raw',
	import: 'default'
});

/** @param {string} key @param {'.md' | '.txt'} ext */
function routeFromRawKey(key, ext) {
	return key.replace(/^\/src\/lib\/content\/docs\//, '').replace(new RegExp(`${ext}$`), '');
}

/** @returns {string[]} */
export function getAllDocRoutes() {
	const markdown = Object.keys(rawDocLoaders).map((key) => routeFromRawKey(key, '.md'));
	const plainText = Object.keys(rawTxtLoaders).map((key) => routeFromRawKey(key, '.txt'));
	return [...markdown, ...plainText].sort();
}

/** @param {string} route */
export async function loadRawDoc(route) {
	const ext = isTxtRoute(route) ? '.txt' : '.md';
	const loaders = ext === '.txt' ? rawTxtLoaders : rawDocLoaders;
	const suffix = `/${route}${ext}`;
	const entry = Object.entries(loaders).find(([key]) => key.endsWith(suffix));

	if (!entry) {
		return null;
	}

	return entry[1]();
}
