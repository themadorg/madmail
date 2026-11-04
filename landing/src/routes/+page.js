import { releases } from '$lib/releases.js';
import { homeJsonLd } from '$lib/structuredData.js';

export function load() {
	return { jsonLd: homeJsonLd(releases[0]) };
}
