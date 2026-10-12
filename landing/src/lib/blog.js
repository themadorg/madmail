import { parseBlogSource, sortPosts } from './blogSource.js';

const sources = import.meta.glob('../content/blog/*.md', {
	query: '?raw',
	import: 'default',
	eager: true
});

export const posts = sortPosts(
	Object.entries(sources).map(([path, source]) => parseBlogSource(path, source))
);

/** @param {string} slug */
export function postBySlug(slug) {
	return posts.find((post) => post.slug === slug) ?? null;
}
