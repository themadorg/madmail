import { pageLang } from '$lib/pageLang.js';

export const prerender = true;

/** @type {import('./$types').LayoutLoad} */
export function load({ url }) {
	return { lang: pageLang(url.pathname) };
}
