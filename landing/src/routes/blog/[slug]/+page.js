import { error } from '@sveltejs/kit';
import { postBySlug, posts } from '$lib/blog.js';
import { blogPostJsonLd } from '$lib/structuredData.js';

export function entries() {
	return posts.map((post) => ({ slug: post.slug }));
}

/** @param {{ params: { slug: string } }} event */
export function load({ params }) {
	const post = postBySlug(params.slug);
	if (!post) error(404, 'Post not found');
	const translations = posts.filter((other) => other.translation === post.translation);
	const original = translations.find((other) => other.slug === post.translation) ?? translations[0];
	const alternates =
		translations.length > 1
			? [
					...translations.map((other) => ({ hreflang: other.language, href: `/blog/${other.slug}` })),
					{ hreflang: 'x-default', href: `/blog/${original.slug}` }
				]
			: [];
	return {
		post,
		title: post.title,
		description: post.description,
		article: true,
		language: post.language,
		published: post.date,
		author: post.author,
		alternates,
		markdownHref: `/blog/${post.slug}.md`,
		jsonLd: blogPostJsonLd(post)
	};
}
