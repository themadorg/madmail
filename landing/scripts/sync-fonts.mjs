import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const staticFonts = join(root, 'static', 'fonts');

/** @type {const} */
const fonts = [
	{
		package: '@fontsource/inter',
		files: [
			'inter-latin-400-normal.woff2',
			'inter-latin-500-normal.woff2',
			'inter-latin-600-normal.woff2',
			'inter-latin-700-normal.woff2'
		]
	},
	{
		package: '@fontsource/jetbrains-mono',
		files: [
			'jetbrains-mono-latin-400-normal.woff2',
			'jetbrains-mono-latin-500-normal.woff2',
			'jetbrains-mono-latin-600-normal.woff2',
			'jetbrains-mono-latin-700-normal.woff2'
		]
	}
];

mkdirSync(staticFonts, { recursive: true });

for (const { package: pkg, files } of fonts) {
	const sourceDir = join(root, 'node_modules', pkg, 'files');

	for (const file of files) {
		copyFileSync(join(sourceDir, file), join(staticFonts, file));
	}
}

console.log(`Synced ${fonts.flatMap((font) => font.files).length} font files to static/fonts/`);
