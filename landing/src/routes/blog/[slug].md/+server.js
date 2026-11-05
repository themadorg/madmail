import { error } from '@sveltejs/kit';
import { postBySlug, posts } from '$lib/blog.js';
import { blogMarkdown, markdownResponse } from '$lib/markdownPages.js';

export const prerender = true;

export function entries() {
	return posts.map(({ slug }) => ({ slug }));
}

/** @type {import('./$types').RequestHandler} */
export function GET({ params }) {
	const post = postBySlug(params.slug);
	if (!post) error(404, 'Post not found');
	return markdownResponse(blogMarkdown(post));
}
