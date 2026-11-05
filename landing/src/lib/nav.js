export const docPages = [
	{ label: 'Quick Setup', href: '/docs/quick-setup' },
	{ label: 'Features', href: '/docs/project/user-guide/01-what-is-chatmail' },
	{ label: 'Documentation', href: '/docs' }
];

export const docNav = docPages;

/** @param {string} href */
export function getDocNeighbors(href) {
	const index = docPages.findIndex((page) => page.href === href);
	return {
		prev: index > 0 ? docPages[index - 1] : null,
		next: index >= 0 && index < docPages.length - 1 ? docPages[index + 1] : null
	};
}

export const repo = 'https://github.com/themadorg/madmail';
