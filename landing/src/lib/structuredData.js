import { faqKeys } from './i18n/faq.js';
import { homePath, languages, translate } from './i18n/index.js';
import {
	defaultDescription,
	ogImageUrl,
	organizationGithub,
	organizationId,
	organizationName,
	organizationUrl,
	repoUrl,
	sameAs,
	siteName,
	siteOrigin
} from './siteMeta.js';

/** schema.org JSON-LD graphs, rendered by SiteMeta.svelte from `page.data.jsonLd`. */

const websiteId = `${siteOrigin}/#website`;
const softwareId = `${siteOrigin}/#software`;
const sourceId = `${siteOrigin}/#source`;

/**
 * TheMadOrg's full Organization node lives on themadorg.net under the same @id;
 * this is the short reference copy so the graph resolves on its own.
 */
const organization = {
	'@type': 'Organization',
	'@id': organizationId,
	name: organizationName,
	url: organizationUrl,
	logo: `${organizationUrl}/logo.png`,
	sameAs: [organizationGithub]
};

const website = {
	'@type': 'WebSite',
	'@id': websiteId,
	name: siteName,
	url: `${siteOrigin}/`,
	description: defaultDescription,
	inLanguage: languages.map((l) => l.code),
	publisher: { '@id': organizationId }
};

/**
 * The home page graph for one locale: who publishes Madmail, what the software and its
 * source are, this page, and the questions shown on it (FAQPage mirrors the visible list).
 * @param {{ tag_name: string, published_at?: string } | null | undefined} latest
 * @param {string} [lang]
 */
export function homeJsonLd(latest, lang = 'en') {
	/** @param {string} key */
	const t = (key) => translate(lang, key);
	const pageUrl = `${siteOrigin}${homePath(lang)}`;
	const pageId = `${pageUrl}#webpage`;
	const version = latest ? latest.tag_name.replace(/^v/, '') : null;
	return {
		'@context': 'https://schema.org',
		'@graph': [
			organization,
			website,
			{
				'@type': 'WebPage',
				'@id': pageId,
				url: pageUrl,
				name: t('meta_title'),
				description: t('meta_description'),
				inLanguage: lang,
				isPartOf: { '@id': websiteId },
				about: { '@id': softwareId },
				mainEntity: { '@id': softwareId },
				primaryImageOfPage: { '@type': 'ImageObject', url: ogImageUrl, width: 1200, height: 630 },
				publisher: { '@id': organizationId }
			},
			{
				'@type': 'SoftwareApplication',
				'@id': softwareId,
				name: siteName,
				alternateName: ['madmail', 'Madmail chatmail relay'],
				description: t('faq_what_a'),
				slogan: t('motto'),
				url: `${siteOrigin}/`,
				image: ogImageUrl,
				applicationCategory: 'CommunicationApplication',
				applicationSubCategory: 'Chatmail relay',
				operatingSystem: 'Linux, Windows',
				license: 'https://spdx.org/licenses/AGPL-3.0-or-later.html',
				isAccessibleForFree: true,
				offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
				downloadUrl: `${repoUrl}/releases/latest`,
				installUrl: `${siteOrigin}/docs/quick-setup`,
				softwareHelp: { '@type': 'CreativeWork', url: `${siteOrigin}/docs` },
				...(version ? { softwareVersion: version } : {}),
				...(latest?.published_at ? { dateModified: latest.published_at } : {}),
				featureList: ['f_mail', 'f_pgp', 'f_jit', 'f_fed', 'f_nolog', 'f_calls', 'f_tls', 'f_admin'].map(
					(key) => `${t(key)}: ${t(`${key}_body`)}`
				),
				sameAs,
				author: { '@id': organizationId },
				publisher: { '@id': organizationId }
			},
			{
				'@type': 'SoftwareSourceCode',
				'@id': sourceId,
				name: 'madmail',
				description: t('faq_what_a'),
				codeRepository: repoUrl,
				programmingLanguage: 'Rust',
				runtimePlatform: 'Linux',
				license: 'https://spdx.org/licenses/AGPL-3.0-or-later.html',
				targetProduct: { '@id': softwareId },
				...(version ? { version } : {}),
				author: { '@id': organizationId },
				publisher: { '@id': organizationId }
			},
			{
				'@type': 'FAQPage',
				'@id': `${pageUrl}#faq`,
				url: pageUrl,
				inLanguage: lang,
				isPartOf: { '@id': pageId },
				mainEntity: faqKeys.map((key) => ({
					'@type': 'Question',
					name: t(`faq_${key}_q`),
					acceptedAnswer: { '@type': 'Answer', text: t(`faq_${key}_a`) }
				}))
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
