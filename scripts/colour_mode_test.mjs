// Colour-mode bootstrap behaviour. Slices the IIFE out of www-src/main.js and runs
// it against stub globals, so what gets exercised is the shipped source rather than
// a copy of it that could drift.
//
//   node --test scripts/colour_mode_test.mjs

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createContext, runInContext } from 'node:vm';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..');
const file = join(repo, 'crates/chatmail-www/www-src/main.js');
const source = readFileSync(file, 'utf8');

const MARKER = '/* Theme bootstrap.';
const OPEN = '(function () {\n';
const CLOSE = '\n})();\n';

// The substring `(function () {` also occurs in callbacks further down main.js, so
// the block is found by the comment documenting it and confirmed by its contents
// rather than by position alone.
function slice() {
  const marker = source.indexOf(MARKER);
  assert.notEqual(marker, -1, 'theme bootstrap comment not found in main.js');
  assert.equal(source.indexOf(MARKER, marker + 1), -1, 'more than one bootstrap comment');
  const start = source.indexOf(OPEN, marker);
  assert.notEqual(start, -1, 'no IIFE follows the bootstrap comment');
  const end = source.indexOf(CLOSE, start);
  assert.notEqual(end, -1, 'bootstrap IIFE never closes');
  const block = source.slice(start, end + CLOSE.length);
  assert.match(block, /window\.cycleTheme/, 'the sliced block is not the bootstrap');
  assert.match(block, /madmail-theme/, 'the sliced block is not the bootstrap');
  return block;
}

const BOOTSTRAP = slice();

// A deliberately small DOM: only the surface the bootstrap actually touches.
function page({ storage = {}, declared = '', prefersDark = false } = {}) {
  const classes = new Set();
  const onChange = [];
  const button = {
    title: null,
    label: null,
    setAttribute(name, value) {
      if (name === 'aria-label') this.label = value;
    },
  };
  const root = {
    classList: {
      toggle(name, on) {
        if (on) classes.add(name);
        else classes.delete(name);
      },
    },
    dataset: {},
    style: {},
  };
  const media = {
    matches: prefersDark,
    addEventListener(type, fn) {
      if (type === 'change') onChange.push(fn);
    },
  };

  const context = createContext({
    document: {
      documentElement: root,
      querySelector: (sel) => (sel === '.mode-button' ? button : null),
      addEventListener() {},
    },
    window: { matchMedia: () => media },
    localStorage: {
      getItem: (k) => (Object.prototype.hasOwnProperty.call(storage, k) ? storage[k] : null),
      setItem: (k, v) => {
        storage[k] = String(v);
      },
    },
    getComputedStyle: () => ({ getPropertyValue: () => declared }),
  });

  runInContext(BOOTSTRAP, context);

  return {
    storage,
    button,
    get mode() {
      return root.dataset.mode;
    },
    get dark() {
      return classes.has('dark');
    },
    get light() {
      return classes.has('light');
    },
    get colorScheme() {
      return root.style.colorScheme;
    },
    cycle: () => context.window.cycleTheme(),
    prefer(dark) {
      media.matches = dark;
      for (const fn of onChange) fn();
    },
  };
}

test('resolution: with nothing stored or declared, automatic follows the preference', () => {
  const dark = page({ prefersDark: true });
  assert.equal(dark.mode, 'system');
  assert.equal(dark.dark, true);
  assert.equal(dark.light, false);
  assert.equal(dark.colorScheme, 'dark');

  const light = page({ prefersDark: false });
  assert.equal(light.mode, 'system');
  assert.equal(light.light, true);
  assert.equal(light.dark, false);
  assert.equal(light.colorScheme, 'light');
});

test('resolution: a stored choice outranks the preference', () => {
  const p = page({ storage: { 'madmail-theme': 'light' }, prefersDark: true });
  assert.equal(p.mode, 'light');
  assert.equal(p.light, true);
  assert.equal(p.dark, false);
});

test('resolution: --default-mode applies when nothing is stored', () => {
  const p = page({ declared: 'dark', prefersDark: false });
  assert.equal(p.mode, 'dark');
  assert.equal(p.dark, true);
});

test('resolution: a stored automatic outranks --default-mode', () => {
  const p = page({
    storage: { 'madmail-theme': 'system' },
    declared: 'dark',
    prefersDark: false,
  });
  assert.equal(p.mode, 'system');
  assert.equal(p.light, true);
});

test('resolution: the legacy key maps only true and otherwise falls through', () => {
  assert.equal(page({ storage: { 'madmail-light': 'true' }, declared: 'dark' }).mode, 'light');
  assert.equal(
    page({ storage: { 'madmail-light': 'false' }, declared: 'dark' }).mode,
    'dark',
    'any value but true must leave the choice unset so --default-mode applies',
  );
  assert.equal(page({ declared: 'dark' }).mode, 'dark', 'an absent key must fall through');
});

test('cycling: automatic to dark to light and back', () => {
  const p = page({ prefersDark: false });
  assert.equal(p.mode, 'system');
  p.cycle();
  assert.equal(p.mode, 'dark');
  assert.equal(p.dark, true);
  p.cycle();
  assert.equal(p.mode, 'light');
  assert.equal(p.light, true);
  p.cycle();
  assert.equal(p.mode, 'system');
});

test('persistence: cycling writes the chosen mode', () => {
  const p = page();
  p.cycle();
  assert.equal(p.storage['madmail-theme'], 'dark');
  p.cycle();
  assert.equal(p.storage['madmail-theme'], 'light');
  p.cycle();
  assert.equal(p.storage['madmail-theme'], 'system');
});

test('a live preference change applies while automatic and is ignored when pinned', () => {
  const auto = page({ prefersDark: false });
  assert.equal(auto.dark, false);
  auto.prefer(true);
  assert.equal(auto.dark, true, 'automatic must follow a change while the page is open');
  assert.equal(auto.mode, 'system');

  const pinned = page({ storage: { 'madmail-theme': 'light' }, prefersDark: false });
  pinned.prefer(true);
  assert.equal(pinned.light, true, 'a pinned choice must not follow the preference');
  assert.equal(pinned.dark, false);
});

test('the control is labelled for the current mode', () => {
  const p = page();
  assert.equal(p.button.title, 'Auto');
  assert.equal(p.button.label, 'Color theme: Auto');
  p.cycle();
  assert.equal(p.button.title, 'Dark');
  assert.equal(p.button.label, 'Color theme: Dark');
});
