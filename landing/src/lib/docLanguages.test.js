import test from 'node:test';
import assert from 'node:assert/strict';
import { docLanguagesForRoute } from './docLanguages.js';

const routes = ['guide/postgres', 'guide/postgres-fa', 'guide/docker', 'guide/standalone-fa'];
test('both translations link to one another and identify the current language', () => {
	for (const [route, expected] of [['guide/postgres', 'en'], ['guide/postgres-fa', 'fa']]) {
		const result = docLanguagesForRoute(route, routes);
		assert.equal(result.language, expected);
		assert.deepEqual(result.languages.map((variant) => variant.label), ['English', 'فارسی']);
		assert.deepEqual(result.languages.filter((variant) => variant.current).map((variant) => variant.href), [`/docs/${route}`]);
		assert.deepEqual(result.alternates.map((alternate) => alternate.hreflang), ['en', 'fa']);
	}
});
test('pages without translations have no switcher or alternate links', () => {
	assert.deepEqual(docLanguagesForRoute('guide/docker', routes), { language: 'en', languages: [], alternates: [] });
});
test('a standalone Persian document still identifies its language', () => {
	assert.deepEqual(docLanguagesForRoute('guide/standalone-fa', routes), { language: 'fa', languages: [], alternates: [] });
});

test('existing underscore translation names are supported', () => {
	const result = docLanguagesForRoute('guide/setup_fa', ['guide/setup', 'guide/setup_fa']);
	assert.equal(result.language, 'fa');
	assert.equal(result.languages[1].href, '/docs/guide/setup_fa');
});

test('explicit English and Persian filename suffixes form a translation family', () => {
	const routes = ['guide/recipes/server-domains_en', 'guide/recipes/server-domains_fa'];
	for (const [route, language] of [[routes[0], 'en'], [routes[1], 'fa']]) {
		const result = docLanguagesForRoute(route, routes);
		assert.equal(result.language, language);
		assert.deepEqual(result.languages.map(({ href }) => href), routes.map((route) => `/docs/${route}`));
		assert.equal(result.languages.filter(({ current }) => current).length, 1);
	}
});
