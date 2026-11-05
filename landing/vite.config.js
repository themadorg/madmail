import { writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import { mdsvex } from 'mdsvex';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { rehypeDocLinks } from './src/lib/rehypeDocLinks.js';
import { rehypeDocToc } from './src/lib/rehypeDocToc.js';
import { rehypeHeadingIds } from './src/lib/rehypeHeadingIds.js';
import { highlightMdsvex } from './src/lib/mdsvexHighlight.js';
import { remarkLandingLinks, remarkPlainPowershell } from './src/lib/remarkLandingLinks.js';
import { Features } from 'lightningcss';
import { defineConfig } from 'vite';

function generateDocumentationTree() {
	const result = spawnSync('node', ['scripts/generate-documentation-tree.mjs'], {
		stdio: 'inherit'
	});

	if (result.status !== 0) {
		throw new Error('Failed to generate documentation tree');
	}
}

// Two fetches per build; a token (CI has one) lifts GitHub's 60/hour anonymous limit.
const githubHeaders = { Accept: 'application/vnd.github+json' };
const githubToken = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
if (githubToken) githubHeaders.Authorization = `Bearer ${githubToken}`;

/** @param {string} repo */
async function fetchReleases(repo) {
	const response = await fetch(`https://api.github.com/repos/${repo}/releases?per_page=20`, { headers: githubHeaders });
	if (!response.ok) throw new Error(`Unable to generate ${repo} releases: GitHub returned ${response.status}`);
	return (await response.json()).filter((release) => !release.draft);
}

// Generate one snapshot shared by static pages and the homepage.
const releases = (await fetchReleases('themadorg/madmail')).filter((release) => !release.prerelease);
await writeFile(new URL('./src/lib/assets/releases.json', import.meta.url), JSON.stringify(releases));

// Madmail Mobile ships every 0.x build as a prerelease, so keep them.
const phoneReleases = (await fetchReleases('themadorg/madmail-phone')).map((release) => ({
	tag_name: release.tag_name,
	name: release.name,
	html_url: release.html_url,
	published_at: release.published_at,
	prerelease: release.prerelease,
	body: release.body ?? '',
	assets: release.assets.map(({ name, size, browser_download_url }) => ({ name, size, browser_download_url }))
}));
await writeFile(new URL('./src/lib/assets/phoneReleases.json', import.meta.url), JSON.stringify(phoneReleases));

export default defineConfig({
	css: {
		lightningcss: {
			// Keep native light-dark(). The minifier's polyfill resolves it once on :root,
			// so tokens inherited into a `.sheet` (color-scheme: light) would keep their
			// dark values and text would vanish on paper. Chrome 123, Safari 17.5, Firefox 120.
			exclude: Features.LightDark
		}
	},
	build: {
		chunkSizeWarningLimit: 10000,
		rolldownOptions: {
			checks: {
				pluginTimings: false
			}
		}
	},
	server: {
		fs: {
			allow: ['..']
		}
	},
	plugins: [
		{
			name: 'generate-documentation-tree',
			buildStart() {
				generateDocumentationTree();
			}
		},
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) => filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({ fallback: '404.html' }),
			prerender: {
				// Links to missing pages are tolerated, but a page that crashes while
				// prerendering must fail the build instead of silently 404ing in production.
				handleHttpError: ({ status, message }) => {
					if (status === 404) return;
					throw new Error(message);
				},
				handleMissingId: 'ignore',
				// An empty blog has no /blog/[slug] pages yet. Once a post exists,
				// entries() prerenders it and this route is no longer unseen.
				handleUnseenRoutes: ({ routes }) => {
					const unexpected = routes.filter((id) => id !== '/blog/[slug]');
					if (unexpected.length === 0) return;
					throw new Error(
						`Routes were not prerendered:\n${unexpected.map((id) => `  - ${id}`).join('\n')}`
					);
				}
			},
			preprocess: [
				mdsvex({
					extensions: ['.svx', '.md'],
					highlight: { highlighter: highlightMdsvex },
					remarkPlugins: [remarkPlainPowershell, remarkLandingLinks],
					rehypePlugins: [rehypeHeadingIds, rehypeDocToc, rehypeDocLinks]
				})
			],
			extensions: ['.svelte', '.svx', '.md']
		})
	]
});
