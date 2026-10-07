import { visit } from 'unist-util-visit';
import { resolveHref, sourcePathFromFile } from './docLinks.js';

/** @returns {import('unified').Plugin} */
export function rehypeDocLinks() {
	return (tree, file) => {
		const sourcePath = sourcePathFromFile(file);

		visit(tree, 'element', (node) => {
			if (node.tagName !== 'a' || !node.properties?.href) return;

			let href = String(node.properties.href);
			if (href.startsWith('mailto:') || href.startsWith('#')) {
				return;
			}

			if (/^\/\d{2}-/.test(href)) {
				href = `/docs/project/user-guide${href}`;
			}

			const resolved = resolveHref(href, sourcePath);
			if (resolved) {
				node.properties.href = resolved;
			}
			if (/^https?:\/\//i.test(String(node.properties.href))) {
				node.properties.target = '_blank';
				node.properties.rel = ['noopener', 'noreferrer'];
				node.children.push({ type: 'element', tagName: 'svg', properties: { className: ['external-link-icon'], viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: '1.5', ariaHidden: 'true' }, children: [{ type: 'element', tagName: 'path', properties: { strokeLinecap: 'round', strokeLinejoin: 'round', d: 'M13.5 6H5.25A2.25 2.25 0 0 0 3 8.25v10.5A2.25 2.25 0 0 0 5.25 21h10.5A2.25 2.25 0 0 0 18 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25' }, children: [] }] });
			}
		});
	};
}
