import { collectionJsonLd } from '$lib/structuredData.js';

const description =
	'Download the signed Madmail Mobile APK for ARM64 Android 8 or later, check its signature, set up hosting and update it in place.';

export function load() {
	return {
		title: 'Install Madmail Mobile',
		description,
		jsonLd: collectionJsonLd({
			title: 'Install Madmail Mobile',
			href: '/phone/install',
			description,
			crumbs: [
				{ label: 'Madmail Mobile', href: '/phone' },
				{ label: 'Install', href: '/phone/install' }
			]
		})
	};
}
