import { posts } from './blog.js';
import { docMetaForRoute } from './docPageMeta.js';
import { docMarkdown, getMarkdownDocRoutes, markdownUrlForRoute } from './markdownPages.js';
import { releases } from './releases.js';
import { repoUrl, siteOrigin } from './siteMeta.js';

/**
 * /llms.txt and /llms-full.txt, following https://llmstxt.org.
 * Links point at the Markdown copies of each page so agents can read them without HTML.
 */

const USER_GUIDE_README = 'project/user-guide/README';

/** Translated copies (`_fa`, `_ru`, …) duplicate the English page; keep only English here. */
/** @param {string} route */
function isTranslation(route) {
	return /_(?:fa|ru|es|de|fr|zh)$/i.test(route);
}

/** @param {string} route */
const isCliCommandPage = (route) => route.startsWith('guide/cli/') && route !== 'guide/cli/README';

/**
 * `full`: included in llms-full.txt. `optional`: listed under the spec's "Optional" heading,
 * which agents may skip when context is short.
 * @type {{ title: string, test: (route: string) => boolean, full: boolean, optional?: boolean }[]}
 */
const SECTIONS = [
	{
		title: 'User and operator guide',
		test: (route) => route.startsWith('project/user-guide/') && route !== USER_GUIDE_README,
		full: true
	},
	{
		title: 'Installation and operation guides',
		test: (route) => route === 'install-simple-ip-acme' || (route.startsWith('guide/') && !isCliCommandPage(route)),
		full: true
	},
	{ title: 'Project internals', test: (route) => route.startsWith('project/'), full: true },
	{ title: 'Admin web panel', test: (route) => route.startsWith('madmail-admin-web/'), full: false },
	{ title: 'CLI command reference', test: isCliCommandPage, full: true, optional: true },
	{ title: 'Design notes', test: () => true, full: false, optional: true }
];

/** Work-in-progress plans and agent prompts describe intent, not shipped behavior. */
/** @param {string} route */
const isExcluded = (route) => route.startsWith('plans/') || route.startsWith('prompts/');

function groupedRoutes() {
	/** @type {Map<string, string[]>} */
	const groups = new Map(SECTIONS.map((section) => [section.title, []]));
	for (const route of getMarkdownDocRoutes()) {
		if (route === USER_GUIDE_README || isTranslation(route) || isExcluded(route)) continue;
		const section = SECTIONS.find((candidate) => candidate.test(route));
		if (section) groups.get(section.title)?.push(route);
	}
	return groups;
}

/** @param {string} text */
function oneLine(text) {
	return text.replace(/\s+/g, ' ').trim();
}

/** @param {string} route */
async function docLink(route) {
	const meta = await docMetaForRoute(route);
	const name = meta.heading || meta.title;
	return `- [${name}](${siteOrigin}${markdownUrlForRoute(route)}): ${oneLine(meta.description)}`;
}

const englishPosts = () => posts.filter((post) => post.language === 'en');

const summary = `> Madmail is a free, open-source (AGPL-3.0-or-later) chatmail relay written in Rust: a purpose-built email server for Delta Chat that ships SMTP, IMAP, HTTP federation, an admin API, and TURN/Iroh call relays as a single binary. It is made by TheMadOrg (https://themadorg.net). We care about secure communication.`;

const facts = `Key facts:

- Madmail is a chatmail relay, not a general-purpose mail server. It only accepts end-to-end encrypted (OpenPGP) mail and rejects plaintext messages.
- Accounts are created just in time (JIT registration) the first time a Delta Chat client logs in; there is no separate sign-up flow.
- Relays federate over HTTPS (\`/mxdeliv\`) and fall back to SMTP, so Madmail interoperates with other chatmail relays and regular mail servers.
- Install on Linux (amd64/arm64) with one binary: download from ${repoUrl}/releases/latest, then run \`sudo ./madmail install --simple --domain mail.example.org --acme-email you@example.com\`, or \`--ip YOUR_IP --auto-ip-cert\` without a domain. Windows 10 and 11 have an installer, and Docker images are also available.
- Madmail Admin (https://admin.madmail.chat) is the web panel for a running relay; Madmail Mobile (${siteOrigin}/phone) hosts a relay on an Android phone.
- TLS certificates come from ACME (Let's Encrypt) automatically, including for bare-IP deployments.
- Licence: AGPL-3.0-or-later. Source code: ${repoUrl}. Website: ${siteOrigin}/. Latest release: ${releases[0]?.tag_name ?? 'see GitHub'}.
- The home page is published in English (${siteOrigin}/), Persian (${siteOrigin}/fa), Spanish (${siteOrigin}/es) and Russian (${siteOrigin}/ru); the documentation is in English.`;

export async function buildLlmsTxt() {
	const groups = groupedRoutes();
	const parts = [`# Madmail\n\n${summary}\n\n${facts}`];

	parts.push(
		`## Start here\n\n- [Documentation index](${siteOrigin}/docs.md): Overview of the Madmail user and operator guide.\n- [Quick start](${siteOrigin}${markdownUrlForRoute('project/user-guide/02-quick-start')}): Install and run a chatmail relay, from local testing to public deployment.\n- [What is chatmail?](${siteOrigin}${markdownUrlForRoute('project/user-guide/01-what-is-chatmail')}): How chatmail relays work and what Madmail implements.\n- [Full documentation in one file](${siteOrigin}/llms-full.txt): The guides below concatenated as Markdown.\n- [Madmail in Persian](${siteOrigin}/fa), [in Spanish](${siteOrigin}/es), [in Russian](${siteOrigin}/ru): The home page, translated.`
	);

	for (const section of SECTIONS.filter((candidate) => !candidate.optional)) {
		const routes = groups.get(section.title) ?? [];
		if (!routes.length) continue;
		parts.push(`## ${section.title}\n\n${(await Promise.all(routes.map(docLink))).join('\n')}`);
	}

	const blog = englishPosts().map(
		(post) => `- [${post.title}](${siteOrigin}/blog/${post.slug}.md): ${oneLine(post.description || post.title)} (${post.date})`
	);
	if (blog.length) parts.push(`## Blog\n\n${blog.join('\n')}`);

	parts.push(
		`## Madmail Mobile\n\n- [Madmail Mobile](${siteOrigin}/phone): Android app (ARM64, Android 8+) that hosts a Madmail server on the phone for Wi-Fi and hotspot guests, or manages remote Madmail servers over SSH or the admin API. It is not a chat client; guests use Delta Chat.\n- [Install Madmail Mobile](${siteOrigin}/phone/install): Signed APK download, signature check and setup steps.\n- [Madmail Mobile questions](${siteOrigin}/phone/faq): Addresses, mobile data, battery, root, iPhone and security.`
	);

	parts.push(
		`## Releases\n\n- [Release notes](${siteOrigin}/releases): Changelog and downloads for every version.\n- [GitHub releases](${repoUrl}/releases): Prebuilt Linux binaries and checksums.`
	);

	const optional = SECTIONS.filter((section) => section.optional).flatMap((section) => groups.get(section.title) ?? []);
	if (optional.length) {
		parts.push(`## Optional\n\n${(await Promise.all(optional.map(docLink))).join('\n')}`);
	}

	return `${parts.join('\n\n')}\n`;
}

export async function buildLlmsFullTxt() {
	const groups = groupedRoutes();
	const routes = [
		USER_GUIDE_README,
		...SECTIONS.filter((section) => section.full).flatMap((section) => groups.get(section.title) ?? [])
	];

	const docs = await Promise.all(
		routes.map(async (route) => {
			const markdown = await docMarkdown(route);
			const url = route === USER_GUIDE_README ? `${siteOrigin}/docs` : `${siteOrigin}/docs/${route}`;
			return markdown ? `<!-- Source: ${url} -->\n\n${markdown.trim()}` : null;
		})
	);
	const blog = englishPosts().map(
		(post) => `<!-- Source: ${siteOrigin}/blog/${post.slug} -->\n\n# ${post.title}\n\n${post.date}\n\n${post.body.trim()}`
	);

	return `# Madmail\n\n${summary}\n\n${facts}\n\n${[...docs.filter(Boolean), ...blog].join('\n\n---\n\n')}\n`;
}
