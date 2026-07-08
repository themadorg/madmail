/** Routes compiled with mdsvex (must match docModules glob). */
export const COMPILED_DOC_ROUTES = [
	'project/user-guide/01-what-is-chatmail',
	'project/user-guide/02-quick-start',
	'project/user-guide/03-accounts-and-registration',
	'project/user-guide/04-privacy-and-security',
	'project/user-guide/05-sending-receiving-and-federation',
	'project/user-guide/06-calls-and-real-time',
	'project/user-guide/07-admin-and-cli',
	'project/user-guide/08-quota-and-maintenance',
	'project/user-guide/09-browser-and-web-access',
	'project/user-guide/10-troubleshooting',
	'project/user-guide/11-deployment-ip-domain-certs',
	'project/user-guide/12-dns-mail-auth',
	'project/user-guide/15-endpoint-rewrite',
	'project/user-guide/16-exchangers',
	'project/user-guide/17-customizing-html-pages',
	'install-simple-ip-acme'
];

/** @param {string} route */
export function isCompiledDocRoute(route) {
	return COMPILED_DOC_ROUTES.includes(route);
}

/** @deprecated Use isCompiledDocRoute */
export function isPublishedDocRoute(route) {
	return isCompiledDocRoute(route);
}

/** @deprecated Use COMPILED_DOC_ROUTES */
export const PUBLISHED_DOC_ROUTES = COMPILED_DOC_ROUTES;
