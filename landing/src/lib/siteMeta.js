export const siteOrigin = 'https://madmail.chat';
export const siteName = 'Madmail';
export const defaultTitle = 'Madmail · Chatmail relay for Delta Chat';
export const defaultDescription =
	'A Rust-based mail relay server for Delta Chat. Encrypted, federated, one binary. SMTP, IMAP, PGP-only policy, JIT registration, and built-in TURN.';
export const ogImagePath = '/og-image.jpg';
export const ogImageUrl = `${siteOrigin}${ogImagePath}`;
export const ogImageWidth = 1200;
export const ogImageHeight = 630;
export const repoUrl = 'https://github.com/themadorg/madmail';
export const organizationName = 'The Mad Org';
export const organizationUrl = 'https://github.com/themadorg';
export const sameAs = [repoUrl, organizationUrl, 'https://t.me/the_madmail'];

/** Open Graph locale for each blog language. */
/** @type {Record<string, string>} */
export const ogLocales = { en: 'en_US', fa: 'fa_IR', ru: 'ru_RU' };
