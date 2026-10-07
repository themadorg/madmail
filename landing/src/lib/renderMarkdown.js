import { marked } from 'marked';
import { resolveHref } from './docLinks.js';
import { sourcePathForRoute } from './docs.js';
import { wrapTableOfContentsHtml } from './enhanceDocHtml.js';
import { uniqueSlug } from './slugify.js';

/** @param {string} markdown @param {string} route */
export function renderMarkdown(markdown, route) {
	const sourcePath = sourcePathForRoute(route);
	const usedSlugs = new Set();

	marked.use({
		renderer: {
			heading({ text, depth }) {
				const id = uniqueSlug(text, usedSlugs);
				return `<h${depth} id="${id}">${text}</h${depth}>\n`;
			},
			link({ href, title, text }) {
				const resolved = resolveHref(href, sourcePath) ?? href;
				const titleAttr = title ? ` title="${title}"` : '';
				const external = /^https?:\/\//i.test(resolved);
				const icon = external ? '<svg class="external-link-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="M13.5 6H5.25A2.25 2.25 0 0 0 3 8.25v10.5A2.25 2.25 0 0 0 5.25 21h10.5A2.25 2.25 0 0 0 18 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" /></svg>' : '';
				return `<a href="${resolved}"${titleAttr}${external ? ' target="_blank" rel="noopener noreferrer"' : ''}>${text}${icon}</a>`;
			}
		}
	});

	return wrapTableOfContentsHtml(marked.parse(markdown, { async: false }));
}
