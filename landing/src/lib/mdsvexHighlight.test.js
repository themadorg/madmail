import assert from 'node:assert/strict';
import test from 'node:test';
import { highlightMdsvex } from './mdsvexHighlight.js';

test('powershell current-directory prefix keeps its backslash', () => {
	const html = highlightMdsvex('.\\madmail.exe install', 'powershell');
	assert.match(html, /class="language-powershell"/);
	assert.equal(html.includes('.\\madmail.exe install'), true);
});

test('Windows Program Files paths keep backslashes', () => {
	const html = highlightMdsvex('C:\\Program Files\\Madmail\\madmail.exe', 'powershell');
	assert.equal(html.includes('C:\\Program Files\\Madmail\\madmail.exe'), true);
});
