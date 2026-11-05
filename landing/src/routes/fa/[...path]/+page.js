import { error, redirect } from '@sveltejs/kit';
import { getAllDocRoutes } from '$lib/rawDocModules.js';
import { getAllDirRoutes } from '$lib/docTreeData.js';
import { posts } from '$lib/blog.js';
import { releases } from '$lib/releases.js';

export const prerender = true;

const pages = {
	'': () => Promise.all([import('../../+page.js'), import('../../+page.svelte')]),
	docs: () => Promise.all([import('../../docs/+page.js'), import('../../docs/+page.svelte')]),
	'docs/quick-setup': () => Promise.all([import('../../docs/quick-setup/+page.js'), import('../../docs/quick-setup/+page.svelte')]),
	blog: () => Promise.all([import('../../blog/+page.js'), import('../../blog/+page.svelte')]),
	releases: () => Promise.all([import('../../releases/+page.js'), import('../../releases/+page.svelte')]),
	phone: () => Promise.all([import('../../phone/+page.js'), import('../../phone/+page.svelte')]),
	'phone/install': () => Promise.all([import('../../phone/install/+page.js'), import('../../phone/install/+page.svelte')]),
	'phone/faq': () => Promise.all([import('../../phone/faq/+page.js'), import('../../phone/faq/+page.svelte')])
};

export function entries() {
	return [...new Set([
		...Object.keys(pages),
		...getAllDocRoutes().map((route) => `docs/${route}`),
		...getAllDirRoutes().map((route) => `docs/${route}`),
		...posts.map((post) => `blog/${post.slug}`),
		...releases.map((release) => `releases/${release.tag_name}`)
	])].map((path) => ({ path }));
}

export async function load(event) {
	const path = event.params.path ?? '';
	let modules;
	const params = { ...event.params, locale: 'fa' };
	if (path === 'mobile') redirect(301, '/fa/phone');
	if (path === 'docs/features') redirect(301, '/fa/docs/project/user-guide/01-what-is-chatmail');
	if (path === 'docs/deployment') redirect(301, '/fa/docs/project/user-guide/11-deployment-ip-domain-certs');
	if (pages[path]) modules = await pages[path]();
	else if (path.startsWith('docs/')) {
		params.path = path.slice(5);
		modules = await Promise.all([import('../../docs/[...path]/+page.js'), import('../../docs/[...path]/+page.svelte')]);
	} else if (path.startsWith('blog/')) {
		const slug = path.slice(5);
		const post = posts.find((post) => post.slug === slug);
		params.slug = posts.find((other) => post && other.translation === post.translation && other.language === 'fa')?.slug ?? slug;
		modules = await Promise.all([import('../../blog/[slug]/+page.js'), import('../../blog/[slug]/+page.svelte')]);
	} else if (path.startsWith('releases/')) {
		params.tag = path.slice(9);
		modules = await Promise.all([import('../../releases/[tag]/+page.js'), import('../../releases/[tag]/+page.svelte')]);
	} else error(404, 'Page not found');
	const [loader, component] = modules;
	const data = await loader.load({ ...event, params });
	return { ...data, canonical: `/fa${path ? `/${path}` : ''}`, LocalizedPage: component.default };
}
