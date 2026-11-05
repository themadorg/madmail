import { latestPhone } from '$lib/phone.js';
import { phoneJsonLd } from '$lib/structuredData.js';

const description =
	'Madmail Mobile runs a Madmail chatmail server on an Android phone, or manages one on your VPS. Guests on your Wi-Fi or hotspot scan a QR code and chat in Delta Chat.';

export function load() {
	return {
		metaTitle: 'Madmail Mobile, a mail server on your Android phone',
		description,
		jsonLd: phoneJsonLd(latestPhone, description)
	};
}
