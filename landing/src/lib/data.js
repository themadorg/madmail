import { repo } from '$lib/nav.js';

export const features = [
	{ title: 'SMTP + IMAP', desc: 'Submission, incoming, and retrieval, full stack in one process' },
	{ title: 'PGP-only policy', desc: 'Unencrypted mail rejected at the server' },
	{ title: 'JIT registration', desc: 'No sign-up flow; accounts created on first auth' },
	{ title: 'HTTP federation', desc: '/mxdeliv for fast inter-server delivery, SMTP as fallback' },
	{ title: 'No-Log mode', desc: 'Strip message metadata when strict privacy is required' },
	{ title: 'TURN / Iroh', desc: 'Audio/video calls and WebXDC relay, no extra services' },
	{ title: 'ACME / TLS', desc: 'Automatic certificate management' },
	{ title: 'One binary', desc: 'Admin API, WebIMAP, federation, batteries included' }
];

export const deltachatLinks = [
	{ label: 'Website', href: 'https://delta.chat' },
	{ label: 'Download', href: 'https://get.delta.chat' },
	{ label: 'Forum', href: 'https://support.delta.chat' },
	{ label: 'News', href: 'https://delta.chat/en/blog' }
];

export const resources = [
	{ label: 'Releases', href: '/releases' },
	{ label: 'Delta Chat', href: 'https://delta.chat' },
	{ label: 'Download Apps', href: 'https://delta.chat/en/download' },
	{ label: 'Telegram', href: 'https://t.me/the_madmail' },
	{ label: 'Discussions', href: `${repo}/discussions` }
];
