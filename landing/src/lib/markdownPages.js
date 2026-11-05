import { resolveHref, splitHash } from './docLinks.js';
import { hrefFromRoute, isTxtRoute, labelFromRoute, sourcePathForRoute } from './docs.js';
import { getAllDocRoutes, loadRawDoc } from './rawDocModules.js';
import { siteOrigin } from './siteMeta.js';

/**
 * Markdown copies of HTML pages, served at the page URL plus `.md`
 * (the llms.txt convention), e.g. /docs/project/user-guide/02-quick-start.md.
 */

/** Doc routes that have a Markdown source (RFC/draft plain-text routes do not). */
export function getMarkdownDocRoutes() {
	return getAllDocRoutes().filter((route) => !isTxtRoute(route));
}

/** @param {string} route */
export function isMarkdownDocRoute(route) {
	return !isTxtRoute(route) && getAllDocRoutes().includes(route);
}

/** @param {string} route */
export function markdownUrlForRoute(route) {
	return `${hrefFromRoute(route)}.md`;
}

/** @param {string} href site path such as /docs/guide/docker */
function markdownTargetForHref(href) {
	const { path, hash } = splitHash(href);
	if (path === '/docs') return `${siteOrigin}/docs.md${hash}`;
	if (path.startsWith('/docs/')) {
		const route = path.slice('/docs/'.length);
		if (isMarkdownDocRoute(route)) return `${siteOrigin}${path}.md${hash}`;
	}
	return `${siteOrigin}${href}`;
}

/**
 * Rewrite relative links to absolute madmail.chat URLs so the file stands on its own
 * when an LLM or a crawler reads it out of context. Fenced code is left alone.
 * @param {string} markdown
 * @param {string} route
 */
export function absolutizeDocLinks(markdown, route) {
	const sourcePath = sourcePathForRoute(route);
	return markdown
		.split(/(^(?:```|~~~)[^\n]*\n[\s\S]*?^(?:```|~~~)[ \t]*$)/m)
		.map((chunk, index) => {
			if (index % 2 === 1) return chunk;
			return chunk.replace(/(!?\[[^\]]*\]\()([^)\s]+)((?:\s+"[^"]*")?\))/g, (match, open, href, close) => {
				if (/^(?:[a-z][a-z0-9+.-]*:|#)/i.test(href)) return match;
				const resolved = resolveHref(href, sourcePath);
				if (resolved?.startsWith('http')) return `${open}${resolved}${close}`;
				if (resolved?.startsWith('/')) return `${open}${markdownTargetForHref(resolved)}${close}`;
				if (href.startsWith('/')) return `${open}${siteOrigin}${href}${close}`;
				return match;
			});
		})
		.join('');
}

/** @param {string} route */
export async function docMarkdown(route) {
	const raw = await loadRawDoc(route);
	if (raw === null || raw === undefined) return null;
	const body = absolutizeDocLinks(raw.trim(), route);
	const heading = /^#\s/m.test(body.split('\n').find((line) => line.trim()) ?? '') ? '' : `# ${labelFromRoute(route)}\n\n`;
	return `${heading}${body}\n`;
}

/**
 * @param {{ slug: string, title: string, date: string, author?: string, description?: string, body: string }} post
 */
export function blogMarkdown(post) {
	const byline = [post.date, post.author ? `by ${post.author}` : ''].filter(Boolean).join(' · ');
	const body = post.body.replace(/(\]\()\/(?!\/)/g, `$1${siteOrigin}/`);
	return `# ${post.title}\n\n${byline}\n\n${post.description ? `> ${post.description}\n\n` : ''}${body}\n`;
}

/** @param {string} body */
export function markdownResponse(body) {
	return new Response(body, {
		headers: { 'Content-Type': 'text/markdown; charset=utf-8' }
	});
}
