import { collectionJsonLd } from '$lib/structuredData.js';

const description = 'News, release write-ups, and deployment notes from the Madmail chatmail relay project.';

export function load() {
	return {
		title: 'Blog',
		description,
		jsonLd: collectionJsonLd({ title: 'Blog', href: '/blog', description })
	};
}
