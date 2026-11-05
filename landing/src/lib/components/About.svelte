<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
</script>
<section class="about">
	<h2>{t("What is Chatmail?")}</h2>
	<p><strong>Chatmail</strong> {t('is a protocol for secure, private chat over email. Delta Chat turns IMAP and SMTP into fast, encrypted messaging and works best when both sides use a chatmail relay.')}</p>
	<p>
		{t("Madmail is one implementation of that relay: a Rust rewrite of Madmail v1 with SMTP, IMAP, encryption enforcement, and real-time services built in. It is not a general-purpose mail server It is specialized for how Delta Chat works.")}
	</p>
</section>

<style>
	.about {
		max-width: 42rem;
		margin: 0 auto;
		padding: 3.5rem 1.5rem 0;
	}

	h2 {
		margin: 0 0 1.25rem;
		font-size: 1.35rem;
		font-weight: 600;
		letter-spacing: -0.02em;
		color: var(--color-text);
	}

	p {
		margin: 0 0 1rem;
		line-height: 1.65;
		color: var(--color-text);
	}

	p:last-child {
		margin-bottom: 0;
	}
</style>
