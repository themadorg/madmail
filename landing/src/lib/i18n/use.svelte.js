import { page } from '$app/state';
import { chromeLang } from '$lib/locale.svelte.js';
import { langDir, translate } from './index.js';

/**
 * Reactive translator for components. The language comes from the page
 * (a localized home, a translated post) or the reader's saved preference.
 */
export function useI18n() {
	return {
		get lang() {
			return chromeLang(page.data);
		},
		get dir() {
			return langDir(chromeLang(page.data));
		},
		/** @param {string} key @param {Record<string, string | number>} [vars] */
		t(key, vars) {
			return translate(chromeLang(page.data), key, vars);
		}
	};
}
