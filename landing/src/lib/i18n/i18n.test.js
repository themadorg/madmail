import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { faqKeys } from './faq.js';

const load = (/** @type {string} */ lang) => JSON.parse(readFileSync(new URL(`./${lang}.json`, import.meta.url), 'utf8'));
const locales = ['en', 'fa', 'es', 'ru'];
const en = load('en');
/** @param {string} value */
const placeholders = (value) => [...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

test('every locale has exactly the English keys', () => {
	for (const lang of locales) {
		assert.deepEqual(Object.keys(load(lang)).sort(), Object.keys(en).sort(), lang);
	}
});

test('translations keep the same {placeholders} and are never empty', () => {
	for (const lang of locales) {
		const messages = load(lang);
		for (const [key, value] of Object.entries(messages)) {
			assert.ok(String(value).trim(), `${lang}.${key} is empty`);
			assert.deepEqual(placeholders(value), placeholders(en[key]), `${lang}.${key}`);
		}
	}
});

test('every visible FAQ entry has a question and an answer', () => {
	for (const key of faqKeys) {
		assert.ok(en[`faq_${key}_q`], `faq_${key}_q`);
		assert.ok(en[`faq_${key}_a`], `faq_${key}_a`);
	}
});
