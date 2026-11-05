import { browser } from '$app/environment';
import { isLang } from './i18n/index.js';

const STORAGE_KEY = 'madmail-lang';

/**
 * The reader's chosen language for the chrome (nav, footer, labels) on pages
 * whose content has no language of its own (docs, releases). Only ever set in
 * the browser, so prerendered HTML never depends on it.
 */
export const preference = $state({ lang: /** @type {string | null} */ (null) });

export function loadPreference() {
	if (!browser) return;
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (isLang(saved)) preference.lang = saved;
	} catch {
		/* Storage may be unavailable. */
	}
}

/** @param {string} lang */
export function savePreference(lang) {
	if (!browser || !isLang(lang)) return;
	preference.lang = lang;
	try {
		localStorage.setItem(STORAGE_KEY, lang);
	} catch {
		/* Storage may be unavailable. */
	}
}

/**
 * Language of the page chrome: the page's own language when it has one
 * (a localized home, a translated blog post), else the reader's preference.
 * @param {{ lang?: string | null } | undefined} data
 */
export function chromeLang(data) {
	if (isLang(data?.lang)) return /** @type {string} */ (data.lang);
	return preference.lang ?? 'en';
}
