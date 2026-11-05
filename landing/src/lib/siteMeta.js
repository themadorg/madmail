export const siteOrigin = 'https://madmail.chat';
export const siteName = 'Madmail';
export const defaultTitle = 'Madmail · Chatmail relay for Delta Chat';
export const defaultDescription =
	'Madmail is a free, open-source chatmail relay for Delta Chat, written in Rust: encrypted mail only, federated, one binary to install. We care about secure communication.';
export const ogImagePath = '/og.png';
export const ogImageUrl = `${siteOrigin}${ogImagePath}`;
export const ogImageWidth = 1200;
export const ogImageHeight = 630;
export const repoUrl = 'https://github.com/themadorg/madmail';
/** TheMadOrg publishes Madmail; its own site owns the Organization node. */
export const organizationName = 'TheMadOrg';
export const organizationUrl = 'https://themadorg.net';
export const organizationId = 'https://themadorg.net/#organization';
export const organizationGithub = 'https://github.com/themadorg';
/** Where Madmail itself lives besides this site. */
export const sameAs = [repoUrl, 'https://t.me/the_madmail'];

/** Open Graph locale for each page language. */
/** @type {Record<string, string>} */
export const ogLocales = { en: 'en_US', fa: 'fa_IR', es: 'es_ES', ru: 'ru_RU' };
