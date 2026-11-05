import { redirect } from '@sveltejs/kit';

// Madmail Mobile's onboarding draws a QR code for https://madmail.chat/mobile.
export function load() {
	redirect(301, '/phone');
}
