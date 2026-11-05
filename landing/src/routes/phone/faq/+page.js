import { faqJsonLd } from '$lib/structuredData.js';

/** Plain-text answers: rendered on the page and reused verbatim as FAQPage JSON-LD. */
const faq = [
	{
		question: 'Is Madmail Mobile a chat app?',
		answer:
			'No. It runs and manages a Madmail server. You and your guests chat in Delta Chat, which connects to that server like any other chatmail relay.'
	},
	{
		question: 'Who can join a server on my phone?',
		answer:
			"Anyone who can reach the phone over the network and scans an invite you create. Usually that's people on the same Wi-Fi or on your phone's hotspot. Each account QR code is for one guest, and invitation links can have a use limit and an expiry."
	},
	{
		question: 'Why are the addresses numbers instead of a domain?',
		answer:
			"The phone has no domain name, so the mail domain is the phone's IP address, as in name@[192.168.43.1]. That's why there's nothing to register. If the phone moves to a different network, stop and start hosting so the server picks up the new address."
	},
	{
		question: 'Can guests reach my phone over mobile data?',
		answer:
			"Often they can't. Many carriers put phones behind shared addresses that other people can't connect to. The overview turns orange on mobile data and offers Wi-Fi and hotspot shortcuts."
	},
	{
		question: 'Does it keep running when the screen is off?',
		answer:
			'Hosting runs as a foreground service with a notification you can stop it from. Allow background access during setup, so Android doesn\'t pause the server to save battery. Running a server does use more battery than leaving the phone idle.'
	},
	{
		question: 'Do I need to root my phone?',
		answer:
			'No. The server listens on ports 11465 for SMTP, 11993 for IMAP and 18443 for HTTPS, which any app may use. Delta Chat gets these ports from the invite.'
	},
	{
		question: 'Is there an iPhone version?',
		answer:
			'No. Madmail Mobile runs on Android 8.0 or later with a 64-bit ARM processor. Guests can use Delta Chat on any platform, iPhone included.'
	},
	{
		question: 'Can I manage a Madmail server on my VPS with it?',
		answer:
			'Yes. Over SSH it can install Madmail on a Linux server (ARM64 or x86-64, with root or passwordless sudo), start and stop it, update it with a backup first, and show its statistics. You can also connect to a server you already run with its admin key, over an SSH tunnel or HTTPS.'
	},
	{
		question: 'How are my passwords and keys stored?',
		answer:
			"Saved SSH keys and admin tokens are encrypted with Android Keystore. SSH passwords stay in memory and aren't saved. The app asks you to confirm each server's SSH host key before it connects. App lock can hide the controls behind your fingerprint or screen lock."
	},
	{
		question: 'What happens to my server if I uninstall the app?',
		answer:
			"Android deletes the app's private data, which includes the server's accounts and mail. To get a newer version, install it over the old one instead of uninstalling."
	},
	{
		question: 'Is it finished?',
		answer:
			'Not yet. Every version so far is a pre-release. Hosting, invites, settings and remote servers work, while calls and attachments between guests, offline networks and some admin settings still need work and testing.'
	}
];

const description = 'Answers about Madmail Mobile: who can join, addresses, mobile data, battery, root, iPhone, VPS management and security.';

export function load() {
	return {
		title: 'Madmail Mobile questions',
		description,
		faq,
		jsonLd: faqJsonLd(faq)
	};
}
