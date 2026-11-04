import { isMarkdownDocRoute } from '$lib/markdownPages.js';

/** `<doc route>.md`: the Markdown copy of a documentation page. */
/** @param {string} param */
export function match(param) {
	return /\.md$/i.test(param) && isMarkdownDocRoute(param.slice(0, -'.md'.length));
}
