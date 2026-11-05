import phoneReleases from './assets/phoneReleases.json';

/** Madmail Mobile: the Android app that hosts or manages a Madmail server. */

export const phoneRepo = 'https://github.com/themadorg/madmail-phone';
export const phoneReleasesUrl = `${phoneRepo}/releases`;

/** Signing certificate of every release APK, from the app's README and release notes. */
export const phoneCertSha256 = '3a03365df303f177286f3abdcda98f9907a4648e94b54e7a88027ed52912b81f';

/** Ports the phone listens on (unprivileged, so no root is needed). */
export const phonePorts = [
	{ label: 'SMTP over TLS', value: '11465' },
	{ label: 'IMAP over TLS', value: '11993' },
	{ label: 'HTTPS', value: '18443' }
];

export const phonePages = [
	{ label: 'Overview', href: '/phone' },
	{ label: 'Install', href: '/phone/install' },
	{ label: 'Questions', href: '/phone/faq' }
];

/** Build-time snapshot, newest first. Every 0.x release is a prerelease. */
export { phoneReleases };

/**
 * @typedef {{ name: string, size: number, browser_download_url: string }} PhoneAsset
 * @typedef {{ tag_name: string, name: string, html_url: string, published_at: string, prerelease: boolean, body: string, assets: PhoneAsset[] }} PhoneRelease
 */

/** @param {PhoneRelease} release @param {(name: string) => boolean} test */
function findAsset(release, test) {
	return release.assets.find((asset) => test(asset.name)) ?? null;
}

/** @param {PhoneRelease | undefined} release */
export function describePhoneRelease(release) {
	if (!release) return null;
	const version = release.tag_name.replace(/^v/, '');
	const engine = release.body.match(/Madmail engine (v\d+\.\d+\.\d+)/)?.[1] ?? null;
	return {
		version,
		tag: release.tag_name,
		url: release.html_url,
		date: release.published_at,
		prerelease: release.prerelease,
		engine,
		apk: findAsset(release, (name) => name.endsWith('-arm64.apk')),
		sums: findAsset(release, (name) => name === 'SHA256SUMS'),
		source: findAsset(release, (name) => name.endsWith('-source.tar.gz'))
	};
}

export const latestPhone = describePhoneRelease(phoneReleases[0]);

/** @param {number} bytes */
export function formatMegabytes(bytes) {
	return `${Math.round(bytes / 1_000_000)} MB`;
}

/** @param {string} iso */
export function formatReleaseDate(iso, language = 'en') {
	return new Date(iso).toLocaleDateString(language === 'fa' ? 'fa-IR' : 'en', { year: 'numeric', month: 'long', day: 'numeric', timeZone: 'UTC' });
}
