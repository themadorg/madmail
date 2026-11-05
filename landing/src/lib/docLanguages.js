import { hrefFromRoute } from './docs.js';

const labels = { en: 'English', fa: 'فارسی', ru: 'Русский', es: 'Español' };

/** Available translations use the same filename suffix convention as blog posts.
 * @param {string} route
 * @param {string[]} routes
 */
export function docLanguagesForRoute(route, routes) {
	const suffix = route.match(/[-_](en|fa|ru|es)$/);
	const language = suffix?.[1] ?? 'en';
	const base = suffix ? route.slice(0, -suffix[0].length) : route;
	const variants = Object.entries(labels).flatMap(([code, label]) => {
		const variant = docVariantRoute(base, code, routes);
		return routes.includes(variant)
			? [{ label, href: hrefFromRoute(variant), current: variant === route, code }]
			: [];
	});
	return {
		language,
		languages: variants.length > 1 ? variants : [],
		alternates: variants.length > 1 ? variants.map(({ code, href }) => ({ hreflang: code, href })) : []
	};
}

/** Resolve both unsuffixed English pages and explicit _en/-en translation families. */
export function docVariantRoute(route, language, routes) {
	const base = route.replace(/[-_](en|fa|ru|es)$/, '');
	const candidates = language === 'en' ? [base, `${base}-en`, `${base}_en`] : [`${base}-${language}`, `${base}_${language}`];
	return candidates.find((candidate) => routes.includes(candidate));
}
