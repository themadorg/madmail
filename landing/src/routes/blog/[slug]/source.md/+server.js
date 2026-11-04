import { error } from '@sveltejs/kit';
import { posts } from '$lib/blog.js';

const sources = import.meta.glob('/src/content/blog/*.md', {
	query: '?raw', import: 'default', eager: true
});

export const prerender = true;
export function entries() {
	return posts.map(({ slug }) => ({ slug }));
}

export function GET({ params }) {
	const source = sources[`/src/content/blog/${params.slug}.md`];
	if (!source) error(404, 'Post not found');
	return new Response(source, {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' }
	});
}
