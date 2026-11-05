import { posts } from './blog.js';
import { isLang, prefixedLangs } from './i18n/index.js';

/**
 * The language a page is written in, from its path alone (so prerendering and
 * hooks agree): a localized home, or a translated blog post. Docs and releases
 * return null: their chrome follows the reader's preference instead.
 * @param {string} pathname
 * @returns {string | null}
 */
export function pageLang(pathname) {
	const parts = pathname.split('/').filter(Boolean);
	if (parts.length === 0) return 'en';
	if (parts.length === 1 && prefixedLangs.includes(/** @type {any} */ (parts[0]))) return parts[0];
	if (parts[0] === 'blog' && parts.length === 2) {
		const post = posts.find((candidate) => candidate.slug === parts[1]);
		if (post && isLang(post.language)) return post.language;
	}
	return null;
}
