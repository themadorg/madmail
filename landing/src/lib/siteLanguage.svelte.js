import { browser } from '$app/environment';
import { getContext } from 'svelte';
import { translate, localizedHref, localizedDocTree } from './localization.js';
import { getAllDocRoutes } from './rawDocModules.js';
import { posts } from './blog.js';

export const languageContext = Symbol('site-language');
// Browser-only helpers such as the command palette share the active language.
export const siteLanguage = $state({ code: 'en' });
const docRoutes = getAllDocRoutes();

function languageScope(state) {
	function setSiteLanguage(code) {
		if (!['en', 'fa', 'ru', 'es'].includes(code)) return;
		state.code = code;
		if (browser) siteLanguage.code = code;
	}
	function t(message) { return translate(message, state.code); }
	function localize(href, code = state.code) { return localizedHref(href, code, docRoutes, posts); }
	function localizeTree(nodes) { return localizedDocTree(nodes, state.code, docRoutes, posts); }
	return { siteLanguage: state, setSiteLanguage, t, localize, localizeTree };
}

/** A separate scope for each layout also keeps prerendered languages isolated. */
export function createSiteLanguage(code) { const state = $state({ code }); return languageScope(state); }
const fallback = languageScope(siteLanguage);
export const { setSiteLanguage, t, localize, localizeTree } = fallback;
export function useSiteLanguage() { return getContext(languageContext) ?? fallback; }
