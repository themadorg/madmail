import { docVariantRoute } from './docLanguages.js';
import fa from './locales/fa.js';
import { resolveDocHref, routeFromHref } from './docs.js';

export function translate(message, language) {
	return language === 'fa' ? fa[message] ?? message : message;
}
export function languageFromPath(pathname) { return /^\/fa(?:\/|$)/.test(pathname) ? 'fa' : 'en'; }
export function stripLanguagePrefix(pathname) { return pathname.replace(/^\/fa(?=\/|$)/, '') || '/'; }

/** English uses the original URL; Persian has its own shareable /fa URL. */
export function localizedHref(href, language, docRoutes = [], posts = []) {
	if (typeof href !== 'string') return href;
	if (!href.startsWith('/') || href.startsWith('//')) return href;
	const url = new URL(href, 'https://local.invalid');
	url.pathname = stripLanguagePrefix(url.pathname);
	// Files, feeds and API/download endpoints keep their original locations.
	if (/\.(?:md|txt|xml|json|html?|css|js|png|jpe?g|webp|svg|ico|woff2?|ttf|pdf|apk|exe|zip|gz)$/i.test(url.pathname) || !/^\/(?:$|docs(?:\/|$)|blog(?:\/|$)|releases(?:\/|$)|phone(?:\/|$)|mobile$)/.test(url.pathname)) return href;
	if (url.pathname.startsWith('/docs/')) {
		const route = resolveDocHref(url.pathname).slice('/docs/'.length);
		const candidate = docVariantRoute(route, ['en', 'fa'].includes(language) ? 'en' : language, docRoutes);
		if (candidate) url.pathname = `/docs/${candidate}`;
	} else if (url.pathname.startsWith('/blog/')) {
		const post = posts.find((item) => `/blog/${item.slug}` === url.pathname);
		const translated = post && posts.find((item) => item.translation === post.translation && item.language === (['en', 'fa'].includes(language) ? 'en' : language));
		if (translated) url.pathname = `/blog/${translated.slug}`;
	}
	if (url.pathname === '/blog') url.searchParams.delete('lang');
	if (language === 'fa') url.pathname = `/fa${url.pathname === '/' ? '' : url.pathname}`;
	return url.pathname + url.search + url.hash;
}

/** Pair navigation labels with the chosen language, regardless of source-file order. */
export function localizedDocTree(nodes, language, routes = [], posts = []) {
	const choices = new Map();
	const links = new Map(nodes.filter((node) => node.href).map((node) => [node, localizedHref(node.href, language, routes, posts)]));
	for (const [node, href] of links) {
		const sourceLanguage = node.href.match(/[-_](en|fa|ru|es)$/)?.[1] ?? 'en';
		if (!choices.has(href) || sourceLanguage === language) choices.set(href, node.label);
	}
	const seen = new Set();
	return nodes.flatMap((node) => {
		if (node.children) return [{ ...node, label: translate(node.label, language), children: localizedDocTree(node.children, language, routes, posts) }];
		if (!node.href) return [{ ...node, label: translate(node.label, language) }];
		const href = links.get(node);
		if (seen.has(href)) return [];
		seen.add(href);
		const sourcePath = stripLanguagePrefix(node.href);
		const translationAvailable = node.external || !sourcePath.startsWith('/docs') || Boolean(docVariantRoute(routeFromHref(sourcePath), language, routes));
		return [{ ...node, href, translationAvailable, label: translate(choices.get(href) ?? node.label, language) }];
	});
}
