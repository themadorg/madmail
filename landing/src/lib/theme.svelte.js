import { browser } from '$app/environment';

function readMode() {
	if (!browser) return 'system';
	try {
		const saved = localStorage.getItem('madmail-theme');
		if (['light', 'dark', 'system'].includes(saved)) return saved;
		return localStorage.getItem('madmail-light') === 'true' ? 'light' : 'system';
	} catch { return 'system'; }
}

function resolveLight(mode) {
	return mode === 'light' || (mode === 'system' && browser && window.matchMedia('(prefers-color-scheme: light)').matches);
}

const initialMode = readMode();
export const theme = $state({ mode: initialMode, light: resolveLight(initialMode) });

export function setTheme(mode) {
	if (!['light', 'dark', 'system'].includes(mode)) return;
	theme.mode = mode;
	theme.light = resolveLight(mode);
	if (browser) {
		try { localStorage.setItem('madmail-theme', mode); } catch { /* Storage may be unavailable. */ }
	}
}

export function toggleLightMode() {
	setTheme(theme.light ? 'dark' : 'light');
}

export function watchSystemTheme() {
	const preference = window.matchMedia('(prefers-color-scheme: light)');
	const update = () => { if (theme.mode === 'system') theme.light = preference.matches; };
	preference.addEventListener('change', update);
	update();
	return () => preference.removeEventListener('change', update);
}
