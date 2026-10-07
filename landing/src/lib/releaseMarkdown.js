import { Marked } from 'marked';

function escape(value) {
	return value.replace(/[&<>"']/g, (character) => ({
		'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
	})[character]);
}

function safeUrl(href) {
	try {
		const url = new URL(href, 'https://github.com/themadorg/madmail/');
		return ['https:', 'http:', 'mailto:'].includes(url.protocol) ? url.href : null;
	} catch {
		return null;
	}
}

const markdown = new Marked({
	renderer: {
		html({ text }) { return escape(text); },
		link({ href, tokens }) {
			const text = this.parser.parseInline(tokens);
			const url = safeUrl(href);
			if (!url) return text;
			const external = /^https?:/.test(url);
			const icon = external ? '<svg class="external-link-icon" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" d="M13.5 6H5.25A2.25 2.25 0 0 0 3 8.25v10.5A2.25 2.25 0 0 0 5.25 21h10.5A2.25 2.25 0 0 0 18 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" /></svg>' : '';
			return `<a href="${escape(url)}"${external ? ' target="_blank" title="Opens in a new tab"' : ''} rel="noopener noreferrer">${text}${icon}</a>`;
		},
		image({ href, text }) {
			const url = safeUrl(href);
			return url && !url.startsWith('mailto:')
				? `<img src="${escape(url)}" alt="${escape(text)}" loading="lazy">`
				: escape(text);
		}
	}
});

export function renderReleaseMarkdown(body) {
	const tokens = markdown.lexer(body);
	function linkReferences(value) {
		if (Array.isArray(value)) {
			value.forEach(linkReferences);
			return;
		}
		if (!value || typeof value !== 'object') return;
		// Keep existing links, images, raw HTML, and code untouched.
		if (['link', 'image', 'html', 'code', 'codespan'].includes(value.type)) return;
		if (value.type === 'text' && !value.tokens) {
			const pattern = /(?<![\w/@&#])(@[a-z\d](?:[a-z\d-]{0,38})(?![\w-])|#[1-9]\d*\b)/gi;
			const inline = [];
			let offset = 0;
			for (const match of value.text.matchAll(pattern)) {
				inline.push({ type: 'text', raw: value.text.slice(offset, match.index), text: value.text.slice(offset, match.index) });
				const label = match[0];
				inline.push({
					type: 'link', raw: label, title: null,
					href: label.startsWith('@') ? `https://github.com/${label.slice(1)}` : `https://github.com/themadorg/madmail/issues/${label.slice(1)}`,
					tokens: [{ type: 'text', raw: label, text: label }]
				});
				offset = match.index + label.length;
			}
			if (inline.length) {
				inline.push({ type: 'text', raw: value.text.slice(offset), text: value.text.slice(offset) });
				value.tokens = inline;
			}
			return;
		}
		for (const child of Object.values(value)) linkReferences(child);
	}
	linkReferences(tokens);
	return markdown.parser(tokens);
}
