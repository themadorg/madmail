import { docNav } from '$lib/nav.js';
import { getPaletteCommands } from '$lib/commandItems.js';

/** @typedef {{ label: string, href: string, group: string, route?: string, headings?: string[], text?: string, hint?: string }} SearchPage */
/** @typedef {{ kind: 'page', id: string, label: string, href: string, group: string, hint?: string }} PalettePageItem */
/** @typedef {{ kind: 'command', id: string, label: string, group: string, hint?: string, run: () => void }} PaletteCommandItem */
/** @typedef {PalettePageItem | PaletteCommandItem} PaletteItem */

/** @type {SearchPage[]} */
export const sitePages = [
	{
		label: 'Home',
		href: '/',
		group: 'Site',
		text: 'madmail landing home chatmail relay delta chat install features about'
	},
	...docNav.map((page) => ({
		label: page.label,
		href: page.href,
		group: 'Site',
		text: `${page.label} ${page.href} site`
	}))
];

/** @type {SearchPage[] | null} */
let cachedPages = null;

/** @returns {Promise<SearchPage[]>} */
export async function loadSearchPages() {
	if (cachedPages) return cachedPages;

	const { default: index } = await import('$lib/assets/search-index.json');
	cachedPages = [...sitePages, ...index.pages];
	return cachedPages;
}

/** @param {SearchPage} page @param {string} token */
function tokenMatches(page, token) {
	const label = page.label.toLowerCase();
	const route = page.route?.toLowerCase() ?? '';
	const group = page.group.toLowerCase();
	const href = page.href.toLowerCase();
	const headings = (page.headings ?? []).join(' ').toLowerCase();
	const text = (page.text ?? '').toLowerCase();

	return (
		label.includes(token) ||
		route.includes(token) ||
		group.includes(token) ||
		href.includes(token) ||
		headings.includes(token) ||
		text.includes(token)
	);
}

/** @param {SearchPage} page @param {string} token */
function scoreToken(page, token) {
	const label = page.label.toLowerCase();
	const route = page.route?.toLowerCase() ?? '';
	const headings = page.headings ?? [];
	let score = 0;

	if (label === token) score += 120;
	else if (label.startsWith(token)) score += 80;
	else if (label.includes(token)) score += 40;

	if (route.includes(token)) score += 20;

	for (const heading of headings) {
		const lower = heading.toLowerCase();
		if (lower === token) score += 35;
		else if (lower.startsWith(token)) score += 25;
		else if (lower.includes(token)) score += 12;
	}

	if ((page.text ?? '').toLowerCase().includes(token)) score += 4;

	return score;
}

/** @param {SearchPage} page @param {string[]} tokens */
function matchHint(page, tokens) {
	for (const heading of page.headings ?? []) {
		const lower = heading.toLowerCase();
		if (tokens.some((token) => lower.includes(token))) return heading;
	}

	const text = page.text ?? '';
	const lower = text.toLowerCase();
	for (const token of tokens) {
		const index = lower.indexOf(token);
		if (index === -1) continue;

		const start = Math.max(0, index - 36);
		const end = Math.min(text.length, index + token.length + 48);
		const snippet = text.slice(start, end).trim();
		return `${start > 0 ? '…' : ''}${snippet}${end < text.length ? '…' : ''}`;
	}

	return page.group;
}

/** @param {import('$lib/commandItems.js').PaletteCommand} command @param {string} token */
function commandTokenMatches(command, token) {
	const label = (command.getLabel?.() ?? command.label).toLowerCase();
	const searchText = [label, command.keywords, command.group, command.hint ?? '']
		.join(' ')
		.toLowerCase();

	return searchText.includes(token);
}

/** @param {import('$lib/commandItems.js').PaletteCommand} command @param {string} token */
function commandScoreToken(command, token) {
	const label = (command.getLabel?.() ?? command.label).toLowerCase();
	let score = 0;

	if (label === token) score += 160;
	else if (label.startsWith(token)) score += 120;
	else if (label.includes(token)) score += 80;

	if (command.keywords.includes(token)) score += 50;
	if ((command.hint ?? '').toLowerCase().includes(token)) score += 20;

	return score;
}

/** @param {import('$lib/commandItems.js').PaletteCommand[]} commands @param {string[]} tokens */
function searchCommands(commands, tokens) {
	return commands
		.filter((command) => tokens.every((token) => commandTokenMatches(command, token)))
		.map((command) => ({
			kind: /** @type {const} */ ('command'),
			id: command.id,
			label: command.getLabel?.() ?? command.label,
			group: command.group,
			hint: command.hint,
			run: command.run,
			score:
				200 + tokens.reduce((total, token) => total + commandScoreToken(command, token), 0)
		}));
}

/** @param {SearchPage[]} pages @param {string} query @returns {PaletteItem[]} */
export function searchPalette(pages, query) {
	const commands = getPaletteCommands();
	const cliPrefix = /^\s*cli\s+/i.exec(query);
	if (cliPrefix) {
		const cliPages = pages.filter((page) => page.href.startsWith('/docs/guide/cli/'));
		const cliQuery = query.slice(cliPrefix[0].length).trim();
		const matches = cliQuery ? searchPages(cliPages, cliQuery) : cliPages;
		return matches.map((page) => ({
			kind: /** @type {const} */ ('page'),
			id: page.href,
			label: page.label,
			href: page.href,
			group: page.group,
			hint: page.hint
		}));
	}
	const trimmed = query.trim().toLowerCase();

	if (!trimmed) {
		return [
			...commands.map((command) => ({
				kind: /** @type {const} */ ('command'),
				id: command.id,
				label: command.getLabel?.() ?? command.label,
				group: command.group,
				hint: command.hint,
				run: command.run
			})),
			...sitePages.map((page) => ({
				kind: /** @type {const} */ ('page'),
				id: page.href,
				label: page.label,
				href: page.href,
				group: page.group
			}))
		];
	}

	const tokens = trimmed.split(/\s+/).filter(Boolean);
	if (!tokens.length) return searchPalette(pages, '');

	const commandResults = searchCommands(commands, tokens);
	const pageResults = searchPages(pages, query).map((page) => ({
		kind: /** @type {const} */ ('page'),
		id: page.href,
		label: page.label,
		href: page.href,
		group: page.group,
		hint: page.hint,
		score: tokens.reduce((total, token) => total + scoreToken(page, token), 0)
	}));

	return [...commandResults, ...pageResults]
		.sort((a, b) => {
			const scoreA = 'score' in a ? a.score : 0;
			const scoreB = 'score' in b ? b.score : 0;
			return scoreB - scoreA || a.label.localeCompare(b.label);
		})
		.slice(0, 40)
		.map(({ score: _score, ...item }) => item);
}

/**
 * @param {SearchPage[]} pages
 * @param {string} query
 * @returns {SearchPage[]}
 */
export function searchPages(pages, query) {
	const trimmed = query.trim().toLowerCase();

	if (!trimmed) return sitePages;

	const tokens = trimmed.split(/\s+/).filter(Boolean);
	if (!tokens.length) return sitePages;

	return pages
		.map((page) => {
			if (!tokens.every((token) => tokenMatches(page, token))) return null;

			const score = tokens.reduce((total, token) => total + scoreToken(page, token), 0);
			return {
				page: { ...page, hint: matchHint(page, tokens) },
				score
			};
		})
		.filter((entry) => entry !== null)
		.sort((a, b) => b.score - a.score || a.page.label.localeCompare(b.page.label))
		.slice(0, 40)
		.map((entry) => entry.page);
}
