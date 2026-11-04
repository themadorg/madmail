import test from 'node:test';
import assert from 'node:assert/strict';
import { unzipSync, strFromU8 } from 'fflate';
import { createArticleWebxdc } from './articleWebxdc.js';
import { parseBlogSource } from './blogSource.js';
import { readdirSync, readFileSync } from 'node:fs';

test('each article produces a self-contained webxdc with its title, language and content', () => {
	const folder = new URL('../content/blog/', import.meta.url);
	for (const filename of readdirSync(folder).filter((name) => name.endsWith('.md'))) {
		const post = parseBlogSource(filename, readFileSync(new URL(filename, folder), 'utf8'));
		const files = unzipSync(createArticleWebxdc(post, `https://madmail.example/blog/${post.slug}`));
		assert.deepEqual(Object.keys(files).sort(), ['icon.png', 'index.html', 'manifest.toml']);
		assert.equal(strFromU8(files['manifest.toml']), `name = ${JSON.stringify(post.title)}\n`);
		assert.deepEqual(files['icon.png'], new Uint8Array(readFileSync(new URL('../../static/favicon.png', import.meta.url))));
		const html = strFromU8(files['index.html']);
		assert.ok(html.includes(`lang="${post.language}"`));
		assert.ok(html.includes(`dir="${post.language === 'fa' ? 'rtl' : 'ltr'}"`));
		assert.ok(html.includes(post.date));
		assert.ok(html.includes('<article>'));
		assert.ok(!/<script|<link\b/.test(html));
		assert.ok(!html.includes('Share page'));
		if (post.slug.startsWith('imap-smtp-port-443')) {
			assert.ok(html.includes('<table>'));
			assert.ok(html.includes('madmail shared-port enable both'));
		}
	}
});

test('article text and titles cannot inject executable HTML into the archive', () => {
	const files = unzipSync(createArticleWebxdc({ title: '<script>alert(1)</script>', date: '2026-10-12', language: 'en', body: '<script>alert(2)</script>\n\n[bad](javascript:alert%283%29)' }, 'https://madmail.example/blog/test'));
	const html = strFromU8(files['index.html']);
	assert.ok(!html.includes('<script>'));
	assert.ok(!html.includes('href="javascript:'));
	assert.ok(html.includes('&lt;script&gt;'));
});
