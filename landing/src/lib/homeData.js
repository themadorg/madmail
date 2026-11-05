import { languages, translate } from './i18n/index.js';
import { releases } from './releases.js';
import { homeJsonLd } from './structuredData.js';

/** Every home edition links to every other, plus x-default for the English root. */
export const homeAlternates = [
	...languages.map((l) => ({ hreflang: l.code, href: l.home })),
	{ hreflang: 'x-default', href: '/' }
];

/**
 * Page data for one home edition. `fullTitle` skips the " · Madmail" suffix SiteMeta adds.
 * @param {'en' | 'fa' | 'es' | 'ru'} lang
 */
export function homeData(lang) {
	const home = languages.find((l) => l.code === lang)?.home ?? '/';
	return {
		lang,
		language: lang,
		fullTitle: translate(lang, 'meta_title'),
		description: translate(lang, 'meta_description'),
		ogImageAlt: translate(lang, 'og_alt'),
		canonical: home,
		alternates: homeAlternates,
		jsonLd: homeJsonLd(releases[0], lang)
	};
}
