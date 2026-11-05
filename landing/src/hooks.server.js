import { langDir } from '$lib/i18n/index.js';
import { pageLang } from '$lib/pageLang.js';

/**
 * Prerendered pages carry their own language on <html> before hydration, so
 * crawlers and no-JS readers see /fa as Persian and right to left.
 * @type {import('@sveltejs/kit').Handle}
 */
export function handle({ event, resolve }) {
	const lang = pageLang(event.url.pathname) ?? 'en';
	return resolve(event, {
		transformPageChunk: ({ html }) =>
			html.replace('<html lang="en" dir="ltr">', `<html lang="${lang}" dir="${langDir(lang)}">`)
	});
}
