import {
	defaultDescription,
	ogImageUrl,
	organizationName,
	organizationUrl,
	repoUrl,
	sameAs,
	siteName,
	siteOrigin
} from './siteMeta.js';

/** schema.org JSON-LD graphs, rendered by SiteMeta.svelte from `page.data.jsonLd`. */

const organizationId = `${siteOrigin}/#organization`;
const websiteId = `${siteOrigin}/#website`;
const softwareId = `${siteOrigin}/#software`;

const organization = {
	'@type': 'Organization',
	'@id': organizationId,
	name: organizationName,
	url: organizationUrl,
	logo: `${siteOrigin}/apple-touch-icon.png`,
	sameAs
};

const website = {
	'@type': 'WebSite',
	'@id': websiteId,
	name: siteName,
	url: `${siteOrigin}/`,
	description: defaultDescription,
	inLanguage: 'en',
	publisher: { '@id': organizationId }
};

/** @param {{ tag_name: string, published_at?: string } | null | undefined} latest */
export function homeJsonLd(latest) {
	return {
		'@context': 'https://schema.org',
		'@graph': [
			organization,
			website,
			{
				'@type': 'SoftwareApplication',
				'@id': softwareId,
				name: siteName,
				alternateName: ['madmail', 'Madmail chatmail relay'],
				description: defaultDescription,
				url: `${siteOrigin}/`,
				image: ogImageUrl,
				applicationCategory: 'CommunicationApplication',
				applicationSubCategory: 'Mail server',
				operatingSystem: 'Linux',
				programmingLanguage: 'Rust',
				license: 'https://www.gnu.org/licenses/agpl-3.0.html',
				isAccessibleForFree: true,
				offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
				downloadUrl: `${repoUrl}/releases/latest`,
				codeRepository: repoUrl,
				...(latest ? { softwareVersion: latest.tag_name.replace(/^v/, '') } : {}),
				...(latest?.published_at ? { dateModified: latest.published_at } : {}),
				featureList: [
					'SMTP submission and delivery',
					'IMAP mailbox access',
					'PGP-only policy: unencrypted mail is rejected',
					'Just-in-time account registration',
					'HTTP federation with SMTP fallback',
					'No-Log mode',
					'Built-in TURN and Iroh relay for calls and WebXDC',
					'Automatic ACME/TLS certificates'
				],
				author: { '@id': organizationId },
				publisher: { '@id': organizationId }
			}
		]
	};
}

/**
 * Madmail Mobile, the Android host and management app.
 * @param {{ version: string, date?: string, apk?: { browser_download_url: string } | null } | null} latest
 * @param {string} description
 */
export function phoneJsonLd(latest, description) {
	return {
		'@context': 'https://schema.org',
		'@graph': [
			organization,
			{
				'@type': 'MobileApplication',
				'@id': `${siteOrigin}/phone#app`,
				name: 'Madmail Mobile',
				alternateName: ['madmail phone', 'Madmail for Android'],
				description,
				url: `${siteOrigin}/phone`,
				image: ogImageUrl,
				applicationCategory: 'CommunicationApplication',
				applicationSubCategory: 'Mail server',
				operatingSystem: 'Android 8.0 or later (ARM64)',
				isAccessibleForFree: true,
				offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
				codeRepository: 'https://github.com/themadorg/madmail-phone',
				downloadUrl: latest?.apk?.browser_download_url ?? 'https://github.com/themadorg/madmail-phone/releases',
				...(latest ? { softwareVersion: latest.version } : {}),
				...(latest?.date ? { dateModified: latest.date } : {}),
				isRelatedTo: { '@id': softwareId },
				author: { '@id': organizationId },
				publisher: { '@id': organizationId }
			},
			breadcrumbList([
				{ label: 'Madmail', href: '/' },
				{ label: 'Madmail Mobile', href: '/phone' }
			])
		]
	};
}

/** @param {{ question: string, answer: string }[]} items */
export function faqJsonLd(items) {
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'FAQPage',
				url: `${siteOrigin}/phone/faq`,
				mainEntity: items.map((item) => ({
					'@type': 'Question',
					name: item.question,
					acceptedAnswer: { '@type': 'Answer', text: item.answer }
				}))
			},
			breadcrumbList([
				{ label: 'Madmail', href: '/' },
				{ label: 'Madmail Mobile', href: '/phone' },
				{ label: 'Questions', href: '/phone/faq' }
			])
		]
	};
}

/** @param {{ label: string, href: string }[]} items */
function breadcrumbList(items) {
	return {
		'@type': 'BreadcrumbList',
		itemListElement: items.map((item, index) => ({
			'@type': 'ListItem',
			position: index + 1,
			name: item.label,
			item: `${siteOrigin}${item.href}`
		}))
	};
}

/**
 * @param {{ title: string, description: string, href: string, crumbs: { label: string, href: string }[], markdownHref?: string | null }} doc
 */
export function docJsonLd({ title, description, href, crumbs, markdownHref }) {
	const url = `${siteOrigin}${href}`;
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'TechArticle',
				headline: title,
				description,
				url,
				mainEntityOfPage: url,
				inLanguage: 'en',
				isPartOf: { '@id': websiteId },
				about: { '@id': softwareId },
				publisher: organization,
				...(markdownHref
					? { encoding: { '@type': 'MediaObject', encodingFormat: 'text/markdown', contentUrl: `${siteOrigin}${markdownHref}` } }
					: {})
			},
			breadcrumbList([{ label: 'Madmail', href: '/' }, ...crumbs])
		]
	};
}

/**
 * @param {{ slug: string, title: string, date: string, author?: string, language?: string, description?: string }} post
 */
export function blogPostJsonLd(post) {
	const url = `${siteOrigin}/blog/${post.slug}`;
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'BlogPosting',
				headline: post.title,
				description: post.description || undefined,
				url,
				mainEntityOfPage: url,
				datePublished: post.date,
				inLanguage: post.language ?? 'en',
				image: ogImageUrl,
				author: post.author
					? { '@type': 'Person', name: post.author, url: `https://github.com/${encodeURIComponent(post.author)}` }
					: { '@id': organizationId },
				publisher: organization,
				isPartOf: { '@id': websiteId },
				about: { '@id': softwareId },
				encoding: { '@type': 'MediaObject', encodingFormat: 'text/markdown', contentUrl: `${url}.md` }
			},
			breadcrumbList([
				{ label: 'Madmail', href: '/' },
				{ label: 'Blog', href: '/blog' },
				{ label: post.title, href: `/blog/${post.slug}` }
			])
		]
	};
}

/** @param {{ title: string, href: string, description: string, crumbs?: { label: string, href: string }[] }} page */
export function collectionJsonLd({ title, href, description, crumbs = [{ label: title, href }] }) {
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'CollectionPage',
				name: title,
				description,
				url: `${siteOrigin}${href}`,
				isPartOf: { '@id': websiteId },
				about: { '@id': softwareId }
			},
			breadcrumbList([{ label: 'Madmail', href: '/' }, ...crumbs])
		]
	};
}

/**
 * @param {{ tag_name: string, name?: string, published_at?: string, html_url?: string }} release
 * @param {string} description
 */
export function releaseJsonLd(release, description) {
	const url = `${siteOrigin}/releases/${encodeURIComponent(release.tag_name)}`;
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'TechArticle',
				headline: `Madmail ${release.tag_name} release notes`,
				description,
				url,
				mainEntityOfPage: url,
				inLanguage: 'en',
				...(release.published_at ? { datePublished: release.published_at } : {}),
				publisher: organization,
				about: { '@id': softwareId }
			},
			breadcrumbList([
				{ label: 'Madmail', href: '/' },
				{ label: 'Releases', href: '/releases' },
				{ label: release.tag_name, href: `/releases/${encodeURIComponent(release.tag_name)}` }
			])
		]
	};
}

/**
 * Serialize JSON-LD for an inline <script>; `<` is escaped so content can never close the tag.
 * @param {unknown} data
 */
export function serializeJsonLd(data) {
	return JSON.stringify(data).replace(/</g, '\\u003c');
}
