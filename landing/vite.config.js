import { spawnSync } from 'node:child_process';
import { mdsvex } from 'mdsvex';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { rehypeDocLinks } from './src/lib/rehypeDocLinks.js';
import { rehypeDocToc } from './src/lib/rehypeDocToc.js';
import { rehypeHeadingIds } from './src/lib/rehypeHeadingIds.js';
import { highlightMdsvex } from './src/lib/mdsvexHighlight.js';
import { remarkLandingLinks, remarkPlainPowershell } from './src/lib/remarkLandingLinks.js';
import { defineConfig } from 'vite';

function generateDocumentationTree() {
	const result = spawnSync('node', ['scripts/generate-documentation-tree.mjs'], {
		stdio: 'inherit'
	});

	if (result.status !== 0) {
		throw new Error('Failed to generate documentation tree');
	}
}

export default defineConfig({
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
			adapter: adapter(),
			prerender: {
				handleHttpError: 'ignore',
				handleMissingId: 'ignore'
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
