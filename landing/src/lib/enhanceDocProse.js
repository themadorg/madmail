import { initPostgresOptions } from './postgresOptions.js';
import { initCopyOnlyCodeBlock, initEditableCodeBlock, isShellCodeBlock } from './codeBlock.js';
import { slugifyHeading, uniqueSlug } from './slugify.js';

const TOC_TITLE = 'table of contents';

/** @param {HTMLElement} root */
function addHeadingIds(root) {
	const used = new Set();

	for (const heading of root.querySelectorAll('h1, h2, h3, h4, h5, h6')) {
		if (heading.id) {
			used.add(heading.id);
			continue;
		}

		const text = heading.textContent ?? '';
		heading.id = uniqueSlug(text, used);
	}
}

/** @param {HTMLElement} root */
function syncTocAnchors(root) {
	const headings = new Map();
	for (const heading of root.querySelectorAll('h1, h2, h3, h4, h5, h6')) {
		if (heading.id) headings.set(heading.id, heading);
	}

	for (const link of root.querySelectorAll('.doc-toc a[href^="#"]')) {
		const hash = link.getAttribute('href')?.slice(1);
		if (!hash) continue;

		if (headings.has(hash)) continue;

		const label = link.textContent ?? '';
		const match = [...headings.entries()].find(([id]) => slugifyHeading(label) === id);
		if (match) link.setAttribute('href', `#${match[0]}`);
	}
}

/** @param {string} id @param {HTMLAnchorElement} link */
async function copyHeadingLink(id, link) {
	const url = `${window.location.origin}${window.location.pathname}#${id}`;

	try {
		await navigator.clipboard.writeText(url);
		link.dataset.copied = '';
		setTimeout(() => {
			delete link.dataset.copied;
		}, 1500);
	} catch {
		window.location.hash = id;
	}
}

/** @param {HTMLElement} root */
function addHeadingAnchors(root) {
	for (const heading of root.querySelectorAll('h1, h2, h3, h4, h5, h6')) {
		if (!heading.id) continue;
		if (heading.closest('.doc-toc')) continue;
		if (heading.parentElement?.classList.contains('heading-wrap')) continue;

		const wrap = document.createElement('div');
		wrap.className = 'heading-wrap';

		const link = document.createElement('a');
		link.className = 'heading-link';
		link.href = `#${heading.id}`;
		link.setAttribute('aria-label', `Copy link to ${heading.textContent?.trim() ?? 'section'}`);
		link.textContent = '#';
		link.addEventListener('click', (event) => {
			event.preventDefault();
			copyHeadingLink(heading.id, link);
		});
		link.addEventListener('mousedown', (event) => event.preventDefault());

		heading.replaceWith(wrap);
		wrap.append(link, heading);
	}
}

/** @param {HTMLElement} root */
function addCodeBlocks(root) {
	for (const pre of root.querySelectorAll('pre')) {
		if (pre.dataset.codeEnhanced === 'true') continue;
		if (pre.classList.contains('rfc-text')) continue;
		if (pre.parentElement?.classList.contains('code-block')) continue;

		pre.dataset.codeEnhanced = 'true';

		const wrap = document.createElement('div');
		const editable = isShellCodeBlock(pre);
		wrap.className = editable ? 'code-block code-block--wrap' : 'code-block';

		pre.replaceWith(wrap);
		wrap.append(pre);

		if (editable) {
			initEditableCodeBlock(wrap, pre);
		} else {
			initCopyOnlyCodeBlock(wrap, pre);
		}
	}
}

/** @param {HTMLElement} root */
function wrapTableOfContents(root) {
	for (const heading of root.querySelectorAll('h2')) {
		if (heading.closest('.doc-toc')) continue;
		if (heading.textContent?.trim().toLowerCase() !== TOC_TITLE) continue;

		const list = heading.nextElementSibling;
		if (!list || list.tagName !== 'UL') continue;

		const details = document.createElement('details');
		details.className = 'doc-toc';
		details.open = true;

		const summary = document.createElement('summary');
		summary.className = 'doc-toc__summary';
		summary.textContent = heading.textContent?.trim() ?? 'Table of contents';

		list.classList.add('doc-toc__list');
		heading.replaceWith(details);
		details.append(summary, list);
	}
}

/** @param {HTMLElement} node */
function enhance(node) {
	initPostgresOptions(node);
	wrapTableOfContents(node);
	addHeadingIds(node);
	syncTocAnchors(node);
	addHeadingAnchors(node);
	addCodeBlocks(node);
}

/** @param {HTMLElement} node */
export function enhanceDocProse(node) {
	let scheduled = false;

	const schedule = () => {
		if (scheduled) return;
		scheduled = true;
		requestAnimationFrame(() => {
			scheduled = false;
			enhance(node);
		});
	};

	schedule();
	const observer = new MutationObserver(schedule);
	observer.observe(node, { childList: true, subtree: true });

	return {
		destroy() {
			observer.disconnect();
		}
	};
}
