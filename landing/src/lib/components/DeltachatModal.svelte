<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t, localize } = useSiteLanguage();
	import Modal from '$lib/components/Modal.svelte';
	import { deltachatLinks } from '$lib/data.js';

	let { open = $bindable(false) } = $props();
</script>

<Modal
	bind:open
	logo="https://delta.chat/assets/logos/delta-chat.svg"
	label={t("Delta Chat")}
>
	<p>
		<strong>{t("Delta Chat")}</strong> {t("turns normal email (IMAP + SMTP) into a fast, encrypted messaging experience. No new protocol to learn, no closed network to join.")}
	</p>
	<p>
		{t("Delta Chat works best with a chatmail relay like Madmail: instant push notifications, reliable calls, strong privacy defaults, and easy setup.")}
	</p>
	<p class="modal-links">
		{#each deltachatLinks as link, i}
			{#if i > 0}<span class="sep" aria-hidden="true">·</span>{/if}
			<a href={localize(link.href)} target="_blank" rel="noopener noreferrer">{t(link.label)}</a>
		{/each}
	</p>
</Modal>
