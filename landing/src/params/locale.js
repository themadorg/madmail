import { prefixedLangs } from '$lib/i18n/index.js';

/** @param {string} param */
export function match(param) {
	return prefixedLangs.includes(/** @type {any} */ (param));
}
