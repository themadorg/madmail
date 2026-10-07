import { goto } from '$app/navigation';

export function setDocFocus(enabled) {
	const url = new URL(window.location.href);
	if (enabled) url.searchParams.set('focus', 'true');
	else url.searchParams.delete('focus');
	return goto(url, { replaceState: true, noScroll: true, keepFocus: true });
}
