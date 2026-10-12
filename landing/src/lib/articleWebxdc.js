import { madmailIconBase64 } from './assets/webxdcIcon.js';
import { strToU8, zipSync } from 'fflate';
import { renderReleaseMarkdown } from './releaseMarkdown.js';

const escape = (value) => String(value).replace(/[&<>"']/g, (character) => ({
	'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
})[character]);

// Everything the reader needs is in the archive; no site scripts or fonts are loaded.
export function createArticleWebxdc(post, url) {
	const body = renderReleaseMarkdown(post.body, { baseUrl: url, wrapTables: true });
	const language = post.language || 'en';
	const html = `<!doctype html>
<html lang="${escape(language)}" dir="${language === 'fa' ? 'rtl' : 'ltr'}">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:;">
<title>${escape(post.title)}</title>
<style>
:root { color-scheme: light dark; font-family: system-ui, sans-serif; }
body { margin: 0; padding: 1.25rem; background: light-dark(#fff, #101318); color: light-dark(#20242b, #edf0f5); }
article { max-width: 46rem; margin: auto; line-height: 1.75; overflow-wrap: anywhere; }
h1 { font-size: clamp(1.65rem, 5vw, 2.5rem); line-height: 1.2; } h2,h3 { line-height: 1.3; margin-top: 2rem; }
.meta { opacity: .7; } a { color: light-dark(#245abe, #a4c5ff); }
pre,code { background: light-dark(#eef1f5, #202630); border-radius: .3rem; } code { padding: .15em .3em; font-size: .9em; }
pre { padding: 1rem; overflow-x: auto; } pre code { padding: 0; }
.table-scroll { overflow-x: auto; margin: 1.5rem 0; border: 1px solid light-dark(#d5dae1, #39414d); border-radius: .6rem; }
table { width: 100%; border-collapse: collapse; } th,td { padding: .8rem 1rem; text-align: start; min-width: 8rem; }
th { background: light-dark(#eef1f5, #202630); } tbody tr { border-top: 1px solid light-dark(#d5dae1, #39414d); }
blockquote { margin-inline: 0; padding-inline-start: 1rem; border-inline-start: 3px solid #7e8999; }
.external-link-icon { width: .75em; height: .75em; margin-inline-start: .2em; vertical-align: super; }
img { max-width: 100%; } footer { margin-top: 2rem; font-size: .85rem; }
</style></head>
<body><article><h1>${escape(post.title)}</h1>
<p class="meta">${escape(post.date)}${post.author ? ` · ${escape(post.author)}` : ''}</p>
${body}<footer><a href="${escape(url)}">Original article · Madmail</a></footer></article></body></html>`;
	return zipSync({
		'icon.png': Uint8Array.from(atob(madmailIconBase64), (character) => character.charCodeAt(0)),
		'index.html': strToU8(html),
		'manifest.toml': strToU8(`name = ${JSON.stringify(post.title)}\n`)
	});
}

export function downloadArticleWebxdc(post, url) {
	const archive = createArticleWebxdc(post, url);
	const blobUrl = URL.createObjectURL(new Blob([archive], { type: 'application/octet-stream' }));
	const link = document.createElement('a');
	link.href = blobUrl;
	link.download = `${post.slug}.xdc`;
	document.body.append(link);
	link.click();
	link.remove();
	setTimeout(() => URL.revokeObjectURL(blobUrl), 10000);
}
