import { error } from '@sveltejs/kit';
import { releases } from '$lib/releases.js';

export function entries() {
	return releases.map((release) => ({ tag: release.tag_name }));
}

export function load({ params }) {
	const release = releases.find((release) => release.tag_name === params.tag);
	if (!release) error(404, 'Release not found');
	return { release };
}
