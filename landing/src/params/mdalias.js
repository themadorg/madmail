import { match as isDocMarkdown } from './docmd.js';

/** `.md` short links under /docs that are not the Markdown copy of a real page. */
/** @param {string} param */
export function match(param) {
	return /\.md$/i.test(param) && !isDocMarkdown(param);
}
