import test from 'node:test';
import assert from 'node:assert/strict';
import { localizedHref, localizedDocTree, translate, languageFromPath, stripLanguagePrefix } from './localization.js';

const routes = ['guide/postgres', 'guide/postgres-fa', 'guide/docker'];
test('Persian UI messages and untranslated content have explicit fallbacks', () => {
	assert.equal(translate('Copy page', 'fa'), 'کپی صفحه');
	assert.equal(translate('Copy page', 'en'), 'Copy page');
	assert.equal(translate('RFC 5321', 'fa'), 'RFC 5321');
	assert.equal(translate('sudo systemctl start madmail', 'fa'), 'sudo systemctl start madmail');
});
test('documentation navigation prefers existing translations and preserves query/hash', () => {
	assert.equal(localizedHref('/docs/guide/postgres?focus=true#backup', 'fa', routes), '/fa/docs/guide/postgres?focus=true#backup');
	assert.equal(localizedHref('/docs/guide/postgres-fa', 'en', routes), '/docs/guide/postgres');
	assert.equal(localizedHref('/docs/guide/docker', 'fa', routes), '/fa/docs/guide/docker');
	assert.equal(localizedHref('https://github.com/themadorg/madmail', 'fa', routes), 'https://github.com/themadorg/madmail');
});
test('blog links keep the preference and select matching article translations', () => {
	const posts = [{ slug: 'hello', translation: 'hello', language: 'en' }, { slug: 'hello-fa', translation: 'hello', language: 'fa' }];
	assert.equal(localizedHref('/blog', 'fa'), '/fa/blog');
	assert.equal(localizedHref('/blog?lang=fa', 'en'), '/blog');
	assert.equal(localizedHref('/blog/hello', 'fa', routes, posts), '/fa/blog/hello');
});

test('existing underscore translation routes are preferred too', () => {
	assert.equal(localizedHref('/docs/guide/setup', 'fa', ['guide/setup', 'guide/setup_fa']), '/fa/docs/guide/setup');
});

test('explicit _en and _fa documentation routes switch in both directions', () => {
	const routes = ['guide/recipes/server-domains_en', 'guide/recipes/server-domains_fa'];
	assert.equal(localizedHref(`/docs/${routes[0]}#example`, 'fa', routes), `/fa/docs/${routes[0]}#example`);
	assert.equal(localizedHref(`/docs/${routes[1]}`, 'en', routes), `/docs/${routes[0]}`);
});

test('the URL selects English by default and Persian only under /fa', () => {
	assert.equal(languageFromPath('/docs/project/user-guide/02-quick-start'), 'en');
	assert.equal(languageFromPath('/fa/docs/project/user-guide/02-quick-start'), 'fa');
	assert.equal(languageFromPath('/faq'), 'en');
	assert.equal(stripLanguagePrefix('/fa'), '/');
	assert.equal(localizedHref('/docs/project/user-guide/02-quick-start?focus=true#install', 'fa'), '/fa/docs/project/user-guide/02-quick-start?focus=true#install');
	assert.equal(localizedHref('/fa/docs/project/user-guide/02-quick-start?focus=true#install', 'en'), '/docs/project/user-guide/02-quick-start?focus=true#install');
	assert.equal(localizedHref('/fa/phone', 'fa'), '/fa/phone');
	assert.equal(localizedHref('/phone', 'fa'), '/fa/phone');
	assert.equal(localizedHref('/releases/v2.41.0', 'fa'), '/fa/releases/v2.41.0');
	assert.equal(localizedHref('/fa', 'en'), '/');
	assert.equal(localizedHref('/fonts/shabnam/Shabnam.woff2', 'fa'), '/fonts/shabnam/Shabnam.woff2');
	assert.equal(localizedHref('#installation', 'fa'), '#installation');
	assert.equal(localizedHref('/blog/rss.xml', 'fa'), '/blog/rss.xml');
});

test('navigation labels follow the selected language even when Persian files come first', () => {
	const nodes = [
		{ href: '/docs/guide/setup_fa', label: 'راهنمای نصب' },
		{ href: '/docs/guide/setup', label: 'Setup guide' },
		{ href: '/docs/guide/postgres-fa', label: 'راهنمای پایگاه داده' },
		{ href: '/docs/guide/postgres', label: 'PostgreSQL guide' }
	];
	const routes = nodes.map(({ href }) => href.slice('/docs/'.length));
	for (const entries of [nodes, [...nodes].reverse()]) {
		const english = localizedDocTree(entries, 'en', routes);
		const persian = localizedDocTree(entries, 'fa', routes);
		assert.equal(english.length, 2);
		assert.equal(persian.length, 2);
		assert.equal(english.find(({ href }) => href.endsWith('/setup')).label, 'Setup guide');
		assert.equal(english.find(({ href }) => href.endsWith('/postgres')).label, 'PostgreSQL guide');
		assert.equal(persian.find(({ href }) => href.endsWith('/setup')).label, 'راهنمای نصب');
		assert.equal(persian.find(({ href }) => href.endsWith('/postgres')).label, 'راهنمای پایگاه داده');
	}
});

test('documentation availability depends on actual articles, not translated UI titles', () => {
	const nodes = [{ label: 'Guide', children: [
		{ href: '/docs/guide/setup', label: 'Setup' },
		{ href: '/docs/guide/setup_fa', label: 'نصب' },
		{ href: '/docs/guide/docker', label: 'Docker' }
	]}];
	const routes = ['guide/setup', 'guide/setup_fa', 'guide/docker'];
	const persian = localizedDocTree(nodes, 'fa', routes)[0].children;
	assert.equal(persian.find(({ href }) => href.endsWith('/setup')).translationAvailable, true);
	assert.equal(persian.find(({ href }) => href.endsWith('/docker')).translationAvailable, false);
	assert.ok(localizedDocTree(nodes, 'en', routes)[0].children.every(({ translationAvailable }) => translationAvailable));
});
