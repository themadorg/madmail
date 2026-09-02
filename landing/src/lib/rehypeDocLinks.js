import { visit } from 'unist-util-visit';
import { resolveHref, sourcePathFromFile } from './docLinks.js';

/** @returns {import('unified').Plugin} */
export function rehypeDocLinks() {
	return (tree, file) => {
		const sourcePath = sourcePathFromFile(file);

		visit(tree, 'element', (node) => {
			if (node.tagName !== 'a' || !node.properties?.href) return;

			let href = String(node.properties.href);
			if (href.startsWith('http') || href.startsWith('mailto:') || href.startsWith('#')) {
				return;
			}

			if (/^\/\d{2}-/.test(href)) {
				href = `/docs/project/user-guide${href}`;
			}

			const resolved = resolveHref(href, sourcePath);
			if (resolved) {
				node.properties.href = resolved;
			}
		});
	};
}
