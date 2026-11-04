/**
 * One blog post is `landing/src/content/blog/<slug>.md`.
 * The slug is the filename. Frontmatter needs `title` and `date` (`YYYY-MM-DD`).
 * `description` is optional and is used on the index, in the sitemap, and in RSS.
 */

/**
 * @param {string} filename
 * @param {string} source
 */
export function parseBlogSource(filename, source) {
	const slug = filename.replaceAll('\\', '/').split('/').pop().replace(/\.md$/, '');
	if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(slug)) {
		throw new Error(`blog slug must be lowercase words separated by hyphens: ${filename}`);
	}
	const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
	if (!match) {
		throw new Error(`blog post is missing frontmatter: ${filename}`);
	}
	/** @type {Record<string, string>} */
	const meta = {};
	for (const line of match[1].split(/\r?\n/)) {
		if (!line.trim()) continue;
		const sep = line.indexOf(':');
		if (sep === -1) {
			throw new Error(`blog frontmatter line is missing a colon: ${filename}`);
		}
		const key = line.slice(0, sep).trim();
		let value = line.slice(sep + 1).trim();
		if (
			(value.startsWith('"') && value.endsWith('"')) ||
			(value.startsWith("'") && value.endsWith("'"))
		) {
			value = value.slice(1, -1);
		}
		meta[key] = value;
	}
	if (!meta.title) {
		throw new Error(`blog post is missing a title: ${filename}`);
	}
	if (!/^\d{4}-\d{2}-\d{2}$/.test(meta.date ?? '')) {
		throw new Error(`blog post date must be YYYY-MM-DD: ${filename}`);
	}
	return {
		slug,
		title: meta.title,
		date: meta.date,
		author: meta.author ?? '',
		language: meta.language ?? 'en',
		translation: meta.translation ?? slug,
		description: meta.description ?? '',
		body: match[2].trim()
	};
}

/** @param {{ date: string, slug: string }[]} posts */
export function sortPosts(posts) {
	return [...posts].sort((a, b) => b.date.localeCompare(a.date) || a.slug.localeCompare(b.slug));
}
