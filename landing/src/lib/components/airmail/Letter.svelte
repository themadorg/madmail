<script>
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import logo from '$lib/logoUrl.js';

	const i18n = useI18n();
	const principles = ['encrypted', 'jit', 'fed', 'one'];
</script>

<!--
	An aerogramme: one airmail sheet edged with chevrons on all four sides, as
	the printed form is. Its subject line is the section heading.
-->
<section class="letter" id="chatmail" aria-labelledby="letter-title">
	<div class="wrap">
		<article class="aerogramme sheet">
			<div class="leaf">
				<header class="letterhead">
					<img src={logo} alt="" width="52" height="52" loading="lazy" />
					<div class="lh-text">
						<span class="lh-name" lang="en">{i18n.t('letter_name')}</span>
						<span class="lh-meta">{i18n.t('letter_meta')}</span>
					</div>
				</header>

				<h2 id="letter-title">{i18n.t('letter_title')}</h2>

				<div class="body">
					<p class="greeting">{i18n.t('letter_greeting')}</p>
					<p class="intro">{i18n.t('letter_intro')}</p>
					{#each principles as p (p)}
						<p class="para">
							<strong>{i18n.t(`p_${p}`)}.</strong>
							{i18n.t(`p_${p}_body`)}
						</p>
					{/each}
					<div class="sign">
						<p>{i18n.t('letter_signoff')}</p>
						<p class="sign-name" lang="en">{i18n.t('letter_sign')}</p>
					</div>
				</div>
			</div>
		</article>
	</div>
</section>

<style>
	.letter {
		padding-block: var(--section-gap) 0;
	}

	.aerogramme {
		max-width: 58rem;
		margin-inline: auto;
		padding: clamp(8px, 1vw, 12px);
		background: var(--paper) var(--airmail);
		background-size: 24px 24px;
		font-size: var(--step-body);
		line-height: 1.6;
	}

	.leaf {
		padding: clamp(1.75rem, 4vw, 3.5rem) clamp(1.5rem, 5vw, 4.5rem) clamp(2rem, 4vw, 3.5rem);
		background: var(--paper);
	}

	.letterhead {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding-bottom: 1.25rem;
		border-bottom: 2px solid var(--ink);
	}

	.letterhead img {
		width: 52px;
		height: 52px;
	}

	.lh-text {
		display: grid;
		line-height: 1.25;
	}

	.lh-name {
		font-family: var(--font-display);
		font-size: var(--step-h3);
		font-weight: 800;
		text-transform: uppercase;
	}

	.lh-meta {
		color: var(--ink-2);
		font-size: var(--step-small);
	}

	h2 {
		margin: clamp(1.75rem, 4vw, 3rem) 0 0;
		font-family: var(--font-display);
		font-size: var(--step-h2);
		font-weight: 800;
		line-height: 0.92;
		letter-spacing: -0.01em;
		text-transform: uppercase;
		text-wrap: balance;
	}

	:lang(fa) h2 {
		line-height: 1.25;
		letter-spacing: 0;
	}

	.body {
		max-width: 44rem;
		margin-top: clamp(1.25rem, 3vw, 2rem);
	}

	.body p {
		margin: 0;
		text-wrap: pretty;
	}

	.body > * + * {
		margin-top: 1.15em;
	}

	.greeting {
		font-weight: 700;
	}

	.intro {
		font-size: var(--step-lead);
		line-height: 1.5;
	}

	.para strong {
		color: var(--red);
	}

	.sign {
		margin-top: 2.25em;
	}

	.sign-name {
		margin-top: 0.5rem !important;
		font-family: var(--font-display);
		font-size: var(--step-h3);
		font-weight: 800;
		text-transform: uppercase;
		rotate: -2deg;
		transform-origin: left center;
	}

	:dir(rtl) .sign-name {
		rotate: 2deg;
		transform-origin: right center;
	}
</style>
