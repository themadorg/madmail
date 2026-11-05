/** Keep regular and generated article links in the current URL language. */
export function localizeSiteLinks(node, options) {
	let settings = options;
	let frame = 0;
	function updateLinks() {
		frame = 0;
		for (const link of node.querySelectorAll('a[href]:not([hreflang]):not([download])')) {
			const href = link.getAttribute('href');
			const localized = settings.localize(href);
			if (localized !== href) link.setAttribute('href', localized);
		}
	}
	function schedule() { if (!frame) frame = requestAnimationFrame(updateLinks); }
	const observer = new MutationObserver(schedule);
	observer.observe(node, { childList: true, subtree: true });
	updateLinks();
	return {
		update(value) { settings = value; updateLinks(); },
		destroy() { observer.disconnect(); cancelAnimationFrame(frame); }
	};
}
