import en from './en.json';
import es from './es.json';
import fa from './fa.json';
import ru from './ru.json';

/**
 * Locales of the landing page. English lives at the site root; the others are
 * URL-prefixed (/fa, /es, /ru) and prerendered, so every edition is a real page.
 * @type {{ code: 'en' | 'fa' | 'es' | 'ru', name: string, dir: 'ltr' | 'rtl', og: string, home: string }[]}
 */
export const languages = [
	{ code: 'en', name: 'English', dir: 'ltr', og: 'en_US', home: '/' },
	{ code: 'fa', name: 'فارسی', dir: 'rtl', og: 'fa_IR', home: '/fa' },
	{ code: 'es', name: 'Español', dir: 'ltr', og: 'es_ES', home: '/es' },
	{ code: 'ru', name: 'Русский', dir: 'ltr', og: 'ru_RU', home: '/ru' }
];

export const defaultLang = 'en';

/** Locales that carry a URL prefix. */
export const prefixedLangs = languages.filter((l) => l.code !== defaultLang).map((l) => l.code);

/** @type {Record<string, Record<string, string>>} */
export const messages = { en, fa, es, ru };

/** @param {unknown} code @returns {code is 'en' | 'fa' | 'es' | 'ru'} */
export function isLang(code) {
	return typeof code === 'string' && languages.some((l) => l.code === code);
}

/** @param {string} code */
export function langDir(code) {
	return languages.find((l) => l.code === code)?.dir ?? 'ltr';
}

/** @param {string} code */
export function homePath(code) {
	return languages.find((l) => l.code === code)?.home ?? '/';
}

/**
 * Look up a string, falling back to English, then to the key itself.
 * `{name}` placeholders are filled from `vars`.
 * @param {string} lang
 * @param {string} key
 * @param {Record<string, string | number>} [vars]
 */
export function translate(lang, key, vars) {
	let value = messages[lang]?.[key] ?? en[/** @type {keyof typeof en} */ (key)] ?? key;
	if (vars) for (const [k, v] of Object.entries(vars)) value = value.replaceAll(`{${k}}`, String(v));
	return value;
}

/** @param {string} lang */
export function translator(lang) {
	/** @param {string} key @param {Record<string, string | number>} [vars] */
	return (key, vars) => translate(lang, key, vars);
}
