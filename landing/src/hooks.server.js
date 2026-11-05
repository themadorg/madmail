import { localizedHref } from '$lib/localization.js';
import { getAllDocRoutes } from '$lib/rawDocModules.js';
import { posts } from '$lib/blog.js';

/** Persian static pages also have the right language and links before hydration. */
export async function handle({ event, resolve }) {
	const persian = /^\/fa(?:\/|$)/.test(event.url.pathname);
	return resolve(event, {
		transformPageChunk: ({ html }) => {
			if (!persian) return html;
			return html.replace('<html lang="en">', '<html lang="fa" dir="rtl">').replace(/<a\b[^>]*>/g, (tag) => {
				if (/\bhreflang=|\bdownload(?:\s|=|>)/.test(tag)) return tag;
				return tag.replace(/\bhref="([^"]*)"/, (_, href) => `href="${localizedHref(href.replaceAll('&amp;', '&'), 'fa', getAllDocRoutes(), posts).replaceAll('&', '&amp;')}"`);
			});
		}
	});
}
