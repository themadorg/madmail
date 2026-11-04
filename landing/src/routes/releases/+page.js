import { collectionJsonLd } from '$lib/structuredData.js';

const description = 'Madmail release notes, changelogs, and Linux binary downloads for every version.';

export function load() {
	return {
		title: 'Releases',
		description,
		jsonLd: collectionJsonLd({ title: 'Releases', href: '/releases', description })
	};
}
