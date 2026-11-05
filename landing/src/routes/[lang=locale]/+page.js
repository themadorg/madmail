import { homeData } from '$lib/homeData.js';
import { prefixedLangs } from '$lib/i18n/index.js';

/** Prerender one home edition per URL-prefixed locale: /fa, /es, /ru. */
export function entries() {
	return prefixedLangs.map((lang) => ({ lang }));
}

/** @param {{ params: { lang: 'fa' | 'es' | 'ru' } }} event */
export function load({ params }) {
	return homeData(params.lang);
}
