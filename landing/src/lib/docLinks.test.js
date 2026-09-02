import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveHref, sourcePathFromFile, splitHash } from './docLinks.js';

const USER_GUIDE = 'content/docs/project/user-guide/02-quick-start.md';
const IP_ACME = 'content/docs/install-simple-ip-acme.md';

test('splitHash keeps fragment separate from the path', () => {
	assert.deepEqual(splitHash('./17-customizing-html-pages.md#windows'), {
		path: './17-customizing-html-pages.md',
		hash: '#windows'
	});
});

test('sourcePathFromFile reads mdsvex filename fallbacks', () => {
	assert.equal(sourcePathFromFile({ filename: USER_GUIDE }), USER_GUIDE);
	assert.equal(sourcePathFromFile({ history: [USER_GUIDE] }), USER_GUIDE);
	assert.equal(sourcePathFromFile({}), '');
});

test('Windows packaging links become GitHub blob URLs', () => {
	const blob =
		'https://github.com/themadorg/madmail/blob/main/packaging/windows/README.md';

	assert.equal(resolveHref('../../../packaging/windows/README.md', USER_GUIDE), blob);
	assert.equal(
		resolveHref(
			'../../../packaging/windows/README.md#windows-defender-smartscreen-and-uac',
			USER_GUIDE
		),
		`${blob}#windows-defender-smartscreen-and-uac`
	);
	assert.equal(resolveHref('../packaging/windows/README.md', IP_ACME), blob);
	assert.equal(resolveHref('../../../packaging/windows/README.md', ''), blob);
});

test('user-guide links keep hashes and drop the .md suffix', () => {
	assert.equal(
		resolveHref('./17-customizing-html-pages.md#windows', USER_GUIDE),
		'/docs/project/user-guide/17-customizing-html-pages#windows'
	);
	assert.equal(
		resolveHref('./12-advanced-deployment.md', USER_GUIDE),
		'/docs/project/user-guide/12-advanced-deployment'
	);
});

test('relative docs outside user-guide resolve on the landing site', () => {
	assert.equal(resolveHref('../../local-dev.md', USER_GUIDE), '/docs/local-dev');
	assert.equal(
		resolveHref('../15-development-workflow.md', USER_GUIDE),
		'/docs/project/15-development-workflow'
	);
	assert.equal(resolveHref('../../TDD/07-federation.md', USER_GUIDE), '/docs/TDD/07-federation');
	assert.equal(
		resolveHref('../../guide/docker.md#turn-calls', USER_GUIDE),
		'/docs/guide/docker#turn-calls'
	);
	assert.equal(
		resolveHref('../../install-simple-ip-acme.md', USER_GUIDE),
		'/docs/install-simple-ip-acme'
	);
	assert.equal(
		resolveHref('../install-simple-ip-acme.md', USER_GUIDE),
		'/docs/install-simple-ip-acme'
	);
});

test('heuristics still work when mdsvex omits file.path', () => {
	assert.equal(resolveHref('../../local-dev.md', ''), '/docs/local-dev');
	assert.equal(
		resolveHref('../15-development-workflow.md', ''),
		'/docs/project/15-development-workflow'
	);
	assert.equal(resolveHref('../../TDD/07-federation.md', ''), '/docs/TDD/07-federation');
	assert.equal(
		resolveHref('./12-dns-mail-auth.md', ''),
		'/docs/project/user-guide/12-dns-mail-auth'
	);
});
