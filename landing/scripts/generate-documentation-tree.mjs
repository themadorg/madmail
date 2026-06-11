import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { hrefFromRoute, labelFromSlug } from '../src/lib/docs.js';
import { isCompiledDocRoute } from '../src/lib/publishedDocs.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const DOCS_ROOT = join(__dirname, '../../docs');
const OUT_FILE = join(__dirname, '../src/lib/assets/documentation.json');
const SEARCH_OUT_FILE = join(__dirname, '../src/lib/assets/search-index.json');

/** @type {Record<string, string>} */
const DIR_LABELS = {
	cli: 'CLI',
	guide: 'Guide',
	plans: 'Plans',
	problems: 'Problems',
	project: 'Project',
	TDD: 'TDD',
	RFC: 'RFC',
	'user-guide': 'User & Operator Guide'
};

/** @param {string} name */
function dirLabel(name) {
	if (DIR_LABELS[name]) return DIR_LABELS[name];
	return name
		.split('-')
		.map((part) => part.charAt(0).toUpperCase() + part.slice(1))
		.join(' ');
}

/** @param {string} route */
function fileLabel(route) {
	if (route === 'install-simple-ip-acme') return 'IP + ACME';
	if (route === 'project/user-guide/README') return 'Documentation';
	if (route.endsWith('/README')) {
		const parent = route.split('/').at(-2) ?? 'Docs';
		return `${dirLabel(parent)} Overview`;
	}
	return labelFromSlug(route.split('/').pop() ?? route);
}

/** @param {string} name */
function isDocFile(name) {
	return name.endsWith('.md') || name.endsWith('.txt');
}

/** @param {string} name */
function routeFromFile(name) {
	return name.replace(/\.(md|txt)$/, '');
}

/** @param {string} text */
function extractRfcHeadings(text) {
	/** @type {string[]} */
	const headings = [];

	for (const line of text.split('\n').slice(0, 250)) {
		const trimmed = line.trim();
		if (!trimmed || trimmed.length > 72) continue;

		if (/^\d+(?:\.\d+)*\.?\s+[A-Z]/.test(trimmed)) {
			headings.push(trimmed);
			continue;
		}

		if (/^[A-Z][A-Za-z0-9 ,.'()-]+$/.test(trimmed) && !/  /.test(trimmed)) {
			headings.push(trimmed);
		}
	}

	return headings.slice(0, 24);
}

/** @param {string} route */
function hrefForRoute(route) {
	return {
		href: hrefFromRoute(route),
		compiled: isCompiledDocRoute(route)
	};
}

/** @param {string} markdown */
function extractHeadings(markdown) {
	/** @type {string[]} */
	const headings = [];

	for (const line of markdown.split('\n')) {
		const match = line.match(/^#{1,3}\s+(.+?)\s*#*\s*$/);
		if (!match) continue;
		headings.push(match[1].replace(/[*_`[\]()]/g, '').trim());
	}

	return headings;
}

/** @param {string} markdown */
function markdownToPlainText(markdown) {
	return markdown
		.replace(/```[\s\S]*?```/g, ' ')
		.replace(/`[^`]+`/g, ' ')
		.replace(/!\[[^\]]*]\([^)]+\)/g, ' ')
		.replace(/\[([^\]]+)]\([^)]+\)/g, '$1')
		.replace(/^#{1,6}\s+/gm, '')
		.replace(/^\s*[-*+]\s+/gm, '')
		.replace(/^\s*\d+\.\s+/gm, '')
		.replace(/[*_~>|]/g, ' ')
		.replace(/\s+/g, ' ')
		.trim();
}

/** @param {string} a @param {string} b */
function compareNames(a, b) {
	return a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });
}

/**
 * @param {import('node:fs').Dirent} a
 * @param {import('node:fs').Dirent} b
 */
function compareEntries(a, b) {
	if (a.isDirectory() !== b.isDirectory()) {
		return a.isDirectory() ? -1 : 1;
	}
	return compareNames(a.name, b.name);
}

/** @type {import('../src/lib/commandSearch.js').SearchPage[]} */
const searchPages = [];

/**
 * @param {string} dir
 * @param {string[]} groups
 */
async function buildTree(dir, groups = []) {
	const entries = await readdir(dir, { withFileTypes: true });
	const nodes = [];

	for (const entry of entries.sort(compareEntries)) {
		const fullPath = join(dir, entry.name);

		if (entry.isDirectory()) {
			const label = dirLabel(entry.name);
			const children = await buildTree(fullPath, [...groups, label]);
			if (!children.length) continue;

			const rel = relative(DOCS_ROOT, fullPath).replace(/\\/g, '/');
			nodes.push({
				id: rel,
				type: 'dir',
				label,
				children
			});
			continue;
		}

		if (!isDocFile(entry.name)) continue;

		const rel = relative(DOCS_ROOT, fullPath).replace(/\\/g, '/');
		const route = routeFromFile(rel);
		const link = hrefForRoute(route);
		const content = await readFile(fullPath, 'utf8');
		const headings = entry.name.endsWith('.txt')
			? extractRfcHeadings(content)
			: extractHeadings(content);
		const text = entry.name.endsWith('.txt')
			? content.replace(/\s+/g, ' ').trim()
			: markdownToPlainText(content);

		searchPages.push({
			label: fileLabel(route),
			href: link.href,
			group: groups.length ? groups.join(' › ') : 'Documentation',
			route,
			headings,
			text
		});

		nodes.push({
			id: route,
			type: 'file',
			label: fileLabel(route),
			route,
			href: link.href,
			compiled: link.compiled
		});
	}

	return nodes;
}

async function main() {
	const generatedAt = new Date().toISOString();
	const tree = await buildTree(DOCS_ROOT);
	const payload = {
		generatedAt,
		index: {
			id: '__index__',
			label: 'Documentation',
			href: '/docs',
			compiled: true
		},
		tree
	};

	await mkdir(dirname(OUT_FILE), { recursive: true });
	await writeFile(OUT_FILE, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
	console.log(`Wrote ${OUT_FILE} (${tree.length} top-level entries)`);

	const searchPayload = {
		generatedAt,
		pages: searchPages.sort((a, b) => a.label.localeCompare(b.label))
	};

	await writeFile(SEARCH_OUT_FILE, `${JSON.stringify(searchPayload)}\n`, 'utf8');
	console.log(`Wrote ${SEARCH_OUT_FILE} (${searchPages.length} pages)`);
}

main().catch((error) => {
	console.error(error);
	process.exit(1);
});
