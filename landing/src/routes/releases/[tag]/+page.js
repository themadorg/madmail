import { error } from '@sveltejs/kit';
import { descriptionFromMarkdown } from '$lib/docDescription.js';
import { releases } from '$lib/releases.js';
import { releaseJsonLd } from '$lib/structuredData.js';

export function entries() {
	return releases.map((release) => ({ tag: release.tag_name }));
}

export function load({ params }) {
	const release = releases.find((release) => release.tag_name === params.tag);
	if (!release) error(404, 'Release not found');
	const description =
		descriptionFromMarkdown(`# ${release.tag_name}\n\n${release.body ?? ''}`) ||
		`Release notes and Linux downloads for Madmail ${release.tag_name}.`;
	return {
		release,
		title: `${release.tag_name} release notes`,
		description,
		jsonLd: releaseJsonLd(release, description)
	};
}
