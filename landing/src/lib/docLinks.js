import { hrefFromRoute } from './docs.js';

const REPO_BLOB = 'https://github.com/themadorg/madmail/blob/main';
const REPO_ROOT_PREFIXES = new Set([
	'assets',
	'context',
	'crates',
	'external',
	'packaging',
	'scripts',
	'tests'
]);

const DOCS_SUBTREES = [
	'TDD',
	'guide',
	'project',
	'plans',
	'problems',
	'madmail-admin-web',
	'man',
	'prompts'
];

const ROOT_DOC_SLUGS = new Set([
	'ai-assisted-development',
	'context-references',
	'how-we-built-it',
	'install-simple-ip-acme',
	'local-dev',
	'what-is-missing'
]);

/** @param {unknown} file */
export function sourcePathFromFile(file) {
	if (!file || typeof file !== 'object') return '';
	const record = /** @type {{ path?: string, filename?: string, history?: string[] }} */ (file);
	return record.path || record.filename || record.history?.[0] || '';
}

/** @param {string} href */
export function splitHash(href) {
	const index = href.indexOf('#');
	if (index === -1) return { path: href, hash: '' };
	return { path: href.slice(0, index), hash: href.slice(index) };
}

/** @param {string | null} resolved @param {string} hash */
function withHash(resolved, hash) {
	if (!resolved) return null;
	return `${resolved}${hash}`;
}

/** @param {string} path */
function isDocFile(path) {
	return /\.(md|txt)$/i.test(path);
}

/** @param {string} path */
function stripDocExt(path) {
	return path.replace(/\.(md|txt)$/i, '');
}

/** @param {string} sourcePath */
function docsRootFromSource(sourcePath) {
	const match = sourcePath.replace(/\\/g, '/').match(/(.*\/docs)\//);
	if (match) return match[1];
	const normalized = sourcePath.replace(/\\/g, '/');
	if (normalized === 'docs' || normalized.endsWith('/docs')) return normalized;
	return null;
}

/** @param {string} sourcePath */
function sourceDocDir(sourcePath) {
	const normalized = sourcePath.replace(/\\/g, '/');
	const nested = normalized.match(/docs\/(.+)\/[^/]+$/);
	if (nested) return nested[1];
	if (/docs\/[^/]+$/.test(normalized)) return '';
	return '';
}

/** @param {string[]} baseParts @param {string} linkPath */
function resolveRelativeParts(baseParts, linkPath) {
	/** @type {string[]} */
	const stack = [...baseParts];

	for (const part of linkPath.split('/')) {
		if (!part || part === '.') continue;
		if (part === '..') {
			stack.pop();
			continue;
		}
		stack.push(part);
	}

	return stack;
}

/** @param {string} baseDir @param {string} linkPath */
function resolveRelative(baseDir, linkPath) {
	return resolveRelativeParts(baseDir.split('/').filter(Boolean), linkPath).join('/');
}

/** @param {string} docsRoot @param {string} targetPath */
function routeFromDocsPath(docsRoot, targetPath) {
	const prefix = `${docsRoot}/`;
	if (!targetPath.startsWith(prefix)) return null;
	return stripDocExt(targetPath.slice(prefix.length));
}

/** @param {string} linkUrl @param {string} sourcePath */
export function resolveDocLink(linkUrl, sourcePath) {
	if (!isDocFile(linkUrl)) return null;

	const docsRoot = docsRootFromSource(sourcePath);
	if (!docsRoot) return null;

	const sourceDir = sourcePath.slice(0, sourcePath.lastIndexOf('/'));
	const targetPath = resolveRelative(sourceDir, linkUrl);
	const route = routeFromDocsPath(docsRoot, targetPath);

	if (!route) return null;
	if (route === 'project/install-simple-ip-acme' || route.endsWith('/install-simple-ip-acme')) {
		return '/docs/install-simple-ip-acme';
	}
	return hrefFromRoute(route);
}

/** @param {string} href */
function normalizeMarkdownHref(href) {
	return href.replace(/^\.\//, '').replace(/^\//, '').replace(/\.(md|txt)$/i, '');
}

/** @param {string} href */
function repoPathFromHref(href) {
	const cleaned = href.replace(/\\/g, '/');
	const match = cleaned.match(
		/(?:^|\/)((?:assets|context|crates|external|packaging|scripts|tests)\/\S+)$/
	);
	return match?.[1] ?? null;
}

/** @param {string} href @param {string} sourcePath */
export function resolveRepoBlobHref(href, sourcePath) {
	if (!href || href.startsWith('http') || href.startsWith('mailto:') || href.startsWith('#')) {
		return null;
	}

	const fromHref = repoPathFromHref(href);
	if (fromHref && REPO_ROOT_PREFIXES.has(fromHref.split('/')[0])) {
		return `${REPO_BLOB}/${fromHref}`;
	}

	if (!docsRootFromSource(sourcePath)) return null;

	const parts = href.startsWith('/')
		? href.slice(1).split('/').filter(Boolean)
		: resolveRelativeParts(sourceDocDir(sourcePath).split('/').filter(Boolean), href);

	if (!parts.length) return null;

	const repoPath = parts.join('/');
	if (repoPath.startsWith('docs/') || repoPath === 'docs') return null;
	if (!REPO_ROOT_PREFIXES.has(parts[0])) return null;

	return `${REPO_BLOB}/${repoPath}`;
}

/** @param {string} path */
function resolveDocsSubtreeHref(path) {
	const cleaned = path.replace(/\\/g, '/');
	for (const tree of DOCS_SUBTREES) {
		const match = cleaned.match(new RegExp(`(?:^|/)(${tree}/\\S+\\.(?:md|txt))$`, 'i'));
		if (match) return hrefFromRoute(stripDocExt(match[1]));
	}
	return null;
}

/** @param {string} path @param {string} sourcePath */
function resolveHeuristicHref(path, sourcePath) {
	const fromSubtree = resolveDocsSubtreeHref(path);
	if (fromSubtree) return fromSubtree;

	const base = stripDocExt(path.split('/').pop() ?? path);
	if (ROOT_DOC_SLUGS.has(base)) {
		return `/docs/${base}`;
	}

	const parentProject = path.match(/^\.\.\/(\d{2}-[^/]+\.(?:md|txt))$/i);
	if (parentProject && (!sourcePath || sourcePath.includes('user-guide'))) {
		return `/docs/project/${stripDocExt(parentProject[1])}`;
	}

	const normalized = normalizeMarkdownHref(path);
	if (/^\d{2}-/i.test(normalized) && !normalized.includes('/')) {
		return `/docs/project/user-guide/${normalized}`;
	}

	return null;
}

/** @param {string} href @param {string} [sourcePath] */
export function resolveHref(href, sourcePath = '') {
	if (!href || href.startsWith('http') || href.startsWith('mailto:') || href.startsWith('#')) {
		return null;
	}

	const { path, hash } = splitHash(href);

	if (path.startsWith('/docs/')) {
		return withHash(stripDocExt(path), hash);
	}

	if (/^\/\d{2}-.+\.(md|txt)$/i.test(path)) {
		return withHash(`/docs/project/user-guide/${path.slice(1).replace(/\.(md|txt)$/i, '')}`, hash);
	}

	if (path.includes('install-simple-ip-acme')) {
		return withHash('/docs/install-simple-ip-acme', hash);
	}

	const fromSource = resolveDocLink(path, sourcePath);
	if (fromSource) return withHash(fromSource, hash);

	const heuristic = resolveHeuristicHref(path, sourcePath);
	if (heuristic) return withHash(heuristic, hash);

	return withHash(resolveRepoBlobHref(path, sourcePath), hash);
}
