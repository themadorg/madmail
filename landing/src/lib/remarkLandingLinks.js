import { visit } from 'unist-util-visit';
import { resolveHref, sourcePathFromFile } from './docLinks.js';

/** Prism's powershell lexer treats `\` as an escape, which deletes Windows paths. */
const PLAIN_CODE_LANGS = new Set(['powershell', 'ps1', 'pwsh', 'cmd']);

/** @returns {import('unified').Plugin} */
export function remarkPlainPowershell() {
	return (tree) => {
		visit(tree, 'code', (node) => {
			const lang = node.lang?.toLowerCase();
			if (!lang || !PLAIN_CODE_LANGS.has(lang)) return;
			node.lang = 'text';
		});
	};
}

/** @returns {import('unified').Plugin} */
export function remarkLandingLinks() {
	return (tree, file) => {
		const sourcePath = sourcePathFromFile(file);

		visit(tree, 'link', (node) => {
			let url = node.url;
			if (!url || url.startsWith('http') || url.startsWith('mailto:') || url.startsWith('#')) {
				return;
			}

			if (/^\/\d{2}-/.test(url)) {
				url = `/docs/project/user-guide${url}`;
			}

			const resolved = resolveHref(url, sourcePath);
			if (resolved) {
				node.url = resolved;
			}
		});
	};
}
