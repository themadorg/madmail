<script>
	import adminShot from '$lib/assets/madmail-admin.webp';
	import mobileShot from '$lib/assets/phone/overview.webp';
	import { posts } from '$lib/blog.js';
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import { releases } from '$lib/releases.js';

	const i18n = useI18n();
	const latestRelease = releases[0] ?? null;
	/* The newest post in the page's language when there is one, else the newest English post. */
	const latestPost = $derived(
		posts.find((post) => post.language === i18n.lang) ?? posts.find((post) => post.language === 'en') ?? posts[0] ?? null
	);
	const releaseDate = latestRelease?.published_at?.slice(0, 10) ?? '';

	/* One printed sheet: every stamp gets a red or blue tone for its face. */
	const stamps = $derived([
		{
			id: 'mobile',
			area: 'mobile',
			href: '/phone',
			source: 'https://github.com/themadorg/madmail-phone',
			group: 'set_group_app',
			shot: mobileShot,
			shotW: 720,
			shotH: 1600,
			denomination: 'Android',
			extra: ''
		},
		{
			id: 'admin',
			area: 'admin',
			href: 'https://admin.madmail.chat',
			source: 'https://github.com/themadorg/madmail-admin-web',
			group: 'set_group_panel',
			shot: adminShot,
			shotW: 880,
			shotH: 660,
			denomination: 'Web',
			extra: ''
		},
		{
			id: 'blog',
			area: 'blog',
			href: latestPost ? `/blog/${latestPost.slug}` : '/blog',
			source: '/blog/rss.xml',
			group: 'set_group_notes',
			tone: 'red',
			denomination: 'RSS',
			extra: latestPost ? i18n.t('set_latest_post', { title: latestPost.title }) : '',
			extraLang: latestPost?.language
		},
		{
			id: 'releases',
			area: 'releases',
			href: '/releases',
			source: 'https://github.com/themadorg/madmail/releases',
			group: 'set_group_downloads',
			tone: 'blue',
			denomination: latestRelease?.tag_name ?? '',
			extra: latestRelease ? i18n.t('set_latest_release', { tag: latestRelease.tag_name, date: releaseDate }) : ''
		}
	]);

	/** @param {string} href */
	const external = (href) => (href.startsWith('http') ? { target: '_blank', rel: 'noopener noreferrer' } : {});
</script>

<section class="set" id="set" aria-labelledby="set-title">
	<div class="wrap">
		<header class="head">
			<h2 id="set-title">{i18n.t('set_title')}</h2>
			<p>{i18n.t('set_lede')}</p>
		</header>

		<div class="selvage sheet">
			<p class="plate-no plate-no-start" aria-hidden="true" lang="en">
				MM 2
				<span class="check check-red"></span>
				<span class="check check-blue"></span>
			</p>
			<p class="plate-no plate-no-end" aria-hidden="true" lang="en">MM 2</p>

			<div class="plate">
				{#each stamps as stamp (stamp.id)}
					<article class={['stamp', `stamp-${stamp.id}`]} style:grid-area={stamp.area}>
						<div class="paper">
							<div class={['picture', stamp.shot ? 'shot' : `face face-${stamp.tone}`]}>
								{#if stamp.shot}
									<img
										src={stamp.shot}
										alt={i18n.t(`set_${stamp.id}_alt`)}
										width={stamp.shotW}
										height={stamp.shotH}
										loading="lazy"
										decoding="async"
									/>
								{:else}
									<div class="vignette" aria-hidden="true">
										{#if stamp.id === 'blog'}
											<svg viewBox="0 0 200 140">
												<g fill="none" stroke="currentColor" stroke-width="3">
													<path d="M62 22h62l18 18v80H62z" />
													<path d="M124 22v18h18" />
												</g>
												<g stroke="currentColor" stroke-width="2.5">
													<path d="M74 52h56M74 64h56M74 76h56M74 88h40M74 100h48" />
												</g>
												<path d="M150 112 176 60l6 3-26 52-9 6z" fill="currentColor" />
											</svg>
										{:else}
											<svg viewBox="0 0 200 140">
												<g fill="none" stroke="currentColor" stroke-width="3">
													<path d="M52 56h96v62H52z" />
													<path d="M52 56l14-18h68l14 18" />
													<path d="M100 38v80M52 84h96" />
													<path d="M100 38c-10-14-26-12-24-2 2 8 16 6 24 2zm0 0c10-14 26-12 24-2-2 8-16 6-24 2z" />
													<path d="M148 92l20 10v18l-20-8z" />
												</g>
												<circle cx="160" cy="104" r="2.5" fill="currentColor" />
											</svg>
										{/if}
									</div>
									<span class="issuer" lang="en">Madmail</span>
								{/if}
								{#if stamp.denomination}
									<span class="denomination" lang="en" dir="ltr">{stamp.denomination}</span>
								{/if}
							</div>

							<div class="body">
								<h3><a class="main" href={stamp.href} {...external(stamp.href)}>{i18n.t(`set_${stamp.id}`)}</a></h3>
								<p class="blurb">{i18n.t(`set_${stamp.id}_body`)}</p>
								{#if stamp.extra}
									<p class="extra" lang={stamp.extraLang}>{stamp.extra}</p>
								{/if}
								<p class="meta">
									<span class="issue">{i18n.t(stamp.group)}</span>
									<a class="source" href={stamp.source} {...external(stamp.source)}>{i18n.t('set_source')}</a>
								</p>
							</div>
						</div>
					</article>
				{/each}
			</div>
		</div>
	</div>
</section>

<style>
	.set {
		padding-block: var(--section-gap) 0;
	}

	.head {
		display: grid;
		grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
		gap: 1.25rem clamp(2rem, 5vw, 5rem);
		align-items: end;
		margin-bottom: clamp(2.5rem, 5vw, 4rem);
	}

	h2 {
		font-size: var(--step-h2);
	}

	.head p {
		max-width: 34rem;
		font-size: var(--step-lead);
	}

	/* The sheet's margin, with the marks a printer leaves on it. */
	.selvage {
		--selvage: clamp(1.75rem, 3.2vw, 3rem);
		--paper: var(--sheet-2);
		position: relative;
		padding: var(--selvage);
	}

	.plate-no {
		position: absolute;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		color: var(--ink-2);
		font-family: var(--font-display);
		font-size: 0.8125rem;
		font-weight: 800;
		letter-spacing: 0.14em;
		line-height: 1;
	}

	.plate-no-start {
		inset-block-start: calc(var(--selvage) / 2 - 0.4rem);
		inset-inline-start: var(--selvage);
	}

	.plate-no-end {
		inset-block-end: calc(var(--selvage) / 2 - 0.4rem);
		inset-inline-end: var(--selvage);
	}

	.check {
		display: inline-block;
		width: 0.7rem;
		height: 0.7rem;
	}

	.check-red {
		background: var(--red-field);
	}

	.check-blue {
		background: var(--blue-field);
	}

	/*
	 * Stamps butt together: the perforation is the gutter. Each stamp punches its
	 * block-start row and inline-start column; the plate punches the outer end edges.
	 */
	.plate {
		position: relative;
		display: grid;
		grid-template-columns: minmax(0, 4fr) minmax(0, 4fr) minmax(0, 4fr);
		grid-template-areas:
			'mobile admin    admin'
			'mobile blog     releases';
	}

	.plate::after {
		content: '';
		position: absolute;
		inset: auto -5px -5px auto;
		inset-block: 0 -5px;
		inset-inline: 0 -5px;
		border-block-end: 10px dotted var(--sheet-2);
		border-inline-end: 10px dotted var(--sheet-2);
		pointer-events: none;
		z-index: 2;
	}

	.stamp {
		--pad: clamp(14px, 1.4vw, 18px);
		position: relative;
		z-index: 0;
		display: flex;
		min-width: 0;
		transition: transform var(--dur-ui) var(--ease-out);
	}

	.paper {
		position: relative;
		display: flex;
		flex-direction: column;
		width: 100%;
		padding: var(--pad);
		background: var(--sheet);
	}

	.paper::before,
	.paper::after {
		content: '';
		position: absolute;
		z-index: 2;
		pointer-events: none;
	}

	.paper::before {
		inset-block-start: -5px;
		inset-inline: -5px 0;
		border-block-start: 10px dotted var(--sheet-2);
	}

	.paper::after {
		inset-block: -5px 0;
		inset-inline-start: -5px;
		border-inline-start: 10px dotted var(--sheet-2);
	}

	.picture {
		position: relative;
		flex: 0 0 auto;
		aspect-ratio: 4 / 3;
		overflow: hidden;
		background: var(--picture-backdrop);
	}

	.picture img {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: contain;
		transition: scale var(--dur-ui) var(--ease-out);
	}

	/* The phone screen stands upright in its stamp and fills the double block. */
	.stamp-mobile .picture {
		flex-grow: 1;
		aspect-ratio: auto;
		min-height: 22rem;
		background: var(--sheet-3);
	}

	.stamp-mobile .picture img {
		inset: 6% 0 0;
		height: 94%;
		object-fit: contain;
		object-position: center top;
	}

	.face {
		display: grid;
		grid-template-rows: 1fr auto;
		padding: 12% 8% 5%;
	}

	.face-red {
		background: var(--red-field);
		color: var(--red-field-ink);
	}

	.face-blue {
		background: var(--blue-field);
		color: var(--blue-field-ink);
	}

	/* The engraved window: a frame and a keyline over a fine hatch. */
	.vignette {
		position: relative;
		display: grid;
		place-items: center;
		border: 1.5px solid currentColor;
		outline: 1px solid currentColor;
		outline-offset: 3px;
		background: repeating-linear-gradient(0deg, transparent 0 5px, color-mix(in oklab, currentColor 14%, transparent) 5px 6px);
		transform-origin: 50% 100%;
		transition: scale var(--dur-ui) var(--ease-out);
	}

	.vignette svg {
		width: 78%;
		height: auto;
	}

	.issuer {
		margin-top: 0.6rem;
		margin-inline-end: -0.24em;
		font-family: var(--font-display);
		font-size: clamp(0.8125rem, 1.1vw, 1.125rem);
		font-weight: 800;
		letter-spacing: 0.24em;
		line-height: 1;
		text-align: center;
		text-transform: uppercase;
	}

	.denomination {
		position: absolute;
		inset-block-start: 0;
		inset-inline-end: 0;
		padding: 0.3rem 0.6rem 0.35rem;
		background: var(--sheet);
		color: var(--ink);
		font-family: var(--font-display);
		font-size: var(--step-lead);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
	}

	.face .denomination {
		background: transparent;
		color: inherit;
		padding: 0.6rem 0.75rem;
	}

	.body {
		display: flex;
		flex: 1 0 auto;
		flex-direction: column;
		padding: 1rem 0.2rem 0.1rem;
	}

	.stamp-mobile .body {
		flex-grow: 0;
	}

	h3 {
		font-size: var(--step-h3);
	}

	.stamp-mobile h3,
	.stamp-admin h3 {
		font-size: var(--step-h2);
	}

	.main {
		color: inherit;
		text-decoration: none;
	}

	/* The whole stamp is the link; the source link sits above it. */
	.main::after {
		content: '';
		position: absolute;
		inset: 0;
		z-index: 1;
	}

	.blurb {
		margin-top: 0.6rem;
		max-width: 34rem;
		font-size: var(--step-small);
		line-height: 1.5;
	}

	.stamp-mobile .blurb,
	.stamp-admin .blurb {
		font-size: var(--step-body);
	}

	.extra {
		margin-top: 0.5rem;
		font-size: var(--step-small);
		font-weight: 700;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1rem;
		margin-top: auto;
		padding-top: 0.9rem;
		color: var(--ink-2);
		font-size: var(--step-small);
	}

	.issue {
		color: var(--red);
		font-weight: 700;
	}

	.source {
		position: relative;
		z-index: 3;
		color: var(--ink-2);
		font-weight: 700;
		text-decoration-color: var(--red);
		text-decoration-thickness: 2px;
		text-underline-offset: 0.3em;
	}

	/* Madmail Admin printed landscape on the wide sheet: picture beside the text. */
	@media (min-width: 1180px) {
		.stamp-admin .paper {
			flex-direction: row;
			gap: 1.25rem;
		}

		.stamp-admin .picture {
			flex: 0 0 56%;
		}

		.stamp-admin .body {
			flex: 1;
		}
	}

	.main:focus-visible {
		outline: none;
	}

	.stamp:has(.main:focus-visible) {
		z-index: 3;
	}

	.stamp:has(.main:focus-visible) .paper {
		outline: 3px solid var(--focus);
		outline-offset: 4px;
	}

	@media (prefers-reduced-motion: no-preference) {
		.stamp:active {
			transform: scale(0.985);
		}
	}

	@media (hover: hover) and (pointer: fine) {
		.source:hover {
			color: var(--ink);
		}
	}

	@media (hover: hover) and (pointer: fine) and (prefers-reduced-motion: no-preference) {
		.stamp:hover {
			z-index: 3;
			transform: translateY(-4px) rotate(-0.6deg);
		}

		.stamp:hover:dir(rtl) {
			transform: translateY(-4px) rotate(0.6deg);
		}

		.stamp:hover .picture img,
		.stamp:hover .vignette {
			scale: 1.04;
		}
	}

	@media (max-width: 1179px) {
		.head {
			grid-template-columns: minmax(0, 1fr);
		}

		.plate {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
			grid-template-areas:
				'mobile admin'
				'mobile admin'
				'blog   releases';
		}
	}

	@media (max-width: 699px) {
		.plate {
			grid-template-columns: minmax(0, 1fr);
			grid-template-areas: 'mobile' 'admin' 'blog' 'releases';
		}

		.stamp-mobile .picture {
			min-height: 26rem;
		}
	}
</style>
