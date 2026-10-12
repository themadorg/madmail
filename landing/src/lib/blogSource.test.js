import assert from 'node:assert/strict';
import test from 'node:test';
import { parseBlogSource, sortPosts } from './blogSource.js';

test('parseBlogSource reads title, date, description, and body', () => {
	const post = parseBlogSource(
		'shared-port.md',
		`---
title: Shared port
date: 2026-10-11
description: One listener on 443
---

The listener speaks first.
`
	);
	assert.equal(post.slug, 'shared-port');
	assert.equal(post.title, 'Shared port');
	assert.equal(post.date, '2026-10-11');
	assert.equal(post.description, 'One listener on 443');
	assert.equal(post.body, 'The listener speaks first.');
});

test('parseBlogSource rejects a bad slug, a missing title, and a bad date', () => {
	assert.throws(() => parseBlogSource('Shared Port.md', '---\ntitle: A\ndate: 2026-10-11\n---\n'), /slug/);
	assert.throws(() => parseBlogSource('post.md', '---\ndate: 2026-10-11\n---\n'), /title/);
	assert.throws(() => parseBlogSource('post.md', '---\ntitle: A\ndate: 11-10-2026\n---\n'), /date/);
	assert.throws(() => parseBlogSource('post.md', 'hello'), /frontmatter/);
});

test('sortPosts puts the newest date first', () => {
	const sorted = sortPosts([
		{ slug: 'b', date: '2026-01-01' },
		{ slug: 'a', date: '2026-02-01' },
		{ slug: 'c', date: '2026-02-01' }
	]);
	assert.deepEqual(
		sorted.map((post) => post.slug),
		['a', 'c', 'b']
	);
});
