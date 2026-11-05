import { error, redirect } from '@sveltejs/kit';
import { MD_REDIRECTS, mdRedirectTarget } from '$lib/mdRedirects.js';
import { match as isShortLink } from '../../../params/mdalias.js';

export const prerender = true;

export function entries() {
	return Object.keys(MD_REDIRECTS)
		.filter((file) => file !== 'README.md' && isShortLink(file))
		.map((file) => ({ file }));
}

/** @type {import('./$types').PageLoad} */
export function load({ params }) {
	const target = mdRedirectTarget(params.file);
	if (target) redirect(301, target);
	error(404, 'Not found');
}
