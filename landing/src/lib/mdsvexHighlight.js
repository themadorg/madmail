import { escapeSvelte } from 'mdsvex';

/** @param {string} code */
function escapeCode(code) {
	return escapeSvelte(
		code.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
	);
}

/**
 * Fenced code for mdsvex/Svelte. Prism's powershell/bash lexers and the
 * generated html`...` template both eat `\\`, which broke Windows paths.
 *
 * @param {string} code
 * @param {string} [lang]
 */
export function highlightMdsvex(code, lang) {
	const language = (lang || 'text').toLowerCase().replace(/[^a-z0-9+-]/g, '') || 'text';
	const cls = `language-${language}`;
	return `<pre class="${cls}"><code class="${cls}">${escapeCode(code)}</code></pre>`;
}
