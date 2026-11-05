import { cleanMarkdownInline, descriptionFromMarkdown, descriptionFromPlainText } from './docDescription.js';
import {
	ancestorTrailForDirId,
	ancestorTrailForHref,
	docTreeNodes,
	findDirById,
	hrefForDirId,
	readmeHrefForDir
} from './docTreeData.js';
import { DOC_HREF_ALIASES, hrefFromRoute, isTxtRoute, labelFromRoute, routeFromHref } from './docs.js';
import { isMarkdownDocRoute, markdownUrlForRoute } from './markdownPages.js';
import { loadRawDoc } from './rawDocModules.js';
import { defaultDescription } from './siteMeta.js';
import { collectionJsonLd, docJsonLd } from './structuredData.js';

/** @param {string} href */
export function canonicalDocHref(href) {
	const alias = Object.entries(DOC_HREF_ALIASES).find(([, target]) => target === href);
	return alias ? alias[0] : href;
}

/** @param {string} route */
export async function docMetaForRoute(route) {
	const title = labelFromRoute(route);
	const raw = await loadRawDoc(route);

	if (!raw) {
		return { title, description: defaultDescription };
	}

	const description = isTxtRoute(route)
		? descriptionFromPlainText(raw, title)
		: descriptionFromMarkdown(raw) || defaultDescription;
	const h1 = isTxtRoute(route) ? null : raw.match(/^#\s+(.+?)\s*#*\s*$/m);
	const heading = h1 ? cleanMarkdownInline(h1[1]) : null;

	return { title, description, heading };
}

/** @param {import('./docTreeData.js').DocTreeDir} dir */
export async function dirMetaForDir(dir) {
	const readmeHref = readmeHrefForDir(dir);

	if (readmeHref) {
		const route = routeFromHref(readmeHref);
		const meta = await docMetaForRoute(route);
		return {
			title: dir.label,
			description: meta.description
		};
	}

	return {
		title: dir.label,
		description: `Browse ${dir.label} in the Madmail documentation.`
	};
}

/** @param {string} route */
export async function findDirMeta(route) {
	const dir = findDirById(docTreeNodes, route);

	if (!dir) return null;

	return dirMetaForDir(dir);
}

/**
 * Canonical URL, Markdown alternate, and JSON-LD shared by every doc page loader.
 * @param {{ route: string, href: string, title: string, description: string, heading?: string | null }} doc
 */
export function docSeoData({ route, href, title, description, heading }) {
	const canonical = canonicalDocHref(href);
	const markdownHref = isMarkdownDocRoute(route)
		? route === 'project/user-guide/README'
			? '/docs.md'
			: markdownUrlForRoute(route)
		: null;
	const trail = (ancestorTrailForHref(docTreeNodes, hrefFromRoute(route)) ?? []).map((dir) => ({
		label: dir.label,
		href: dir.readmeHref ?? hrefForDirId(dir.id)
	}));
	// A descriptive H1 ("Quick Start: Running a Chatmail Server") makes a better <title> than the slug label.
	const metaTitle = heading && heading.length <= 70 ? heading : title;
	const crumbs = uniqueCrumbs([
		{ label: 'Documentation', href: '/docs' },
		...trail,
		{ label: metaTitle, href: canonical }
	]);
	return {
		canonical,
		markdownHref,
		metaTitle,
		jsonLd: docJsonLd({ title: metaTitle, description, href: canonical, crumbs, markdownHref })
	};
}

/**
 * @param {import('./docTreeData.js').DocTreeDir} dir
 * @param {string} href
 * @param {string} title
 * @param {string} description
 */
export function dirSeoData(dir, href, title, description) {
	const trail = (ancestorTrailForDirId(docTreeNodes, dir.id) ?? []).map((parent) => ({
		label: parent.label,
		href: parent.readmeHref ?? hrefForDirId(parent.id)
	}));
	return {
		jsonLd: collectionJsonLd({
			title,
			description,
			href,
			crumbs: uniqueCrumbs([{ label: 'Documentation', href: '/docs' }, ...trail, { label: title, href }])
		})
	};
}

/** @param {{ label: string, href: string }[]} crumbs */
function uniqueCrumbs(crumbs) {
	const seen = new Set();
	return crumbs.filter((crumb) => !seen.has(crumb.href) && seen.add(crumb.href));
}
