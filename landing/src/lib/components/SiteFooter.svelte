<script>
	import { homePath, languages } from '$lib/i18n/index.js';
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import { savePreference } from '$lib/locale.svelte.js';
	import logo from '$lib/logoUrl.js';
	import { repo } from '$lib/nav.js';

	/** `spacious` is the docs and releases variant: it sits under the page and is revealed on scroll. */
	let { spacious = false } = $props();
	const i18n = useI18n();
	const year = new Date().getUTCFullYear();

	const product = $derived([
		{ label: i18n.t('nav_docs'), href: '/docs' },
		{ label: i18n.t('footer_quick_setup'), href: '/docs/quick-setup' },
		{ label: i18n.t('nav_releases'), href: '/releases' },
		{ label: i18n.t('nav_mobile'), href: '/phone' },
		{ label: i18n.t('footer_admin'), href: 'https://admin.madmail.chat' },
		{ label: i18n.t('nav_blog'), href: '/blog' },
		{ label: i18n.t('footer_rss'), href: '/blog/rss.xml' }
	]);

	const community = $derived([
		{ label: 'GitHub', href: repo },
		{ label: i18n.t('footer_discussions'), href: `${repo}/discussions` },
		{ label: i18n.t('footer_telegram'), href: 'https://t.me/the_madmail' },
		{ label: i18n.t('footer_deltachat'), href: 'https://delta.chat' },
		{ label: i18n.t('footer_apps'), href: 'https://delta.chat/en/download' }
	]);

	/** @param {string} href */
	const external = (href) => (href.startsWith('http') ? { target: '_blank', rel: 'noopener noreferrer' } : {});

	/** @param {HTMLElement} node */
	function clipReveal(node) {
		if (!spacious) return;
		let frame = 0;
		function update() {
			frame = 0;
			const content = node.previousElementSibling;
			if (!content) return;
			const footerBounds = node.getBoundingClientRect();
			const covered = Math.max(0, Math.min(footerBounds.height, content.getBoundingClientRect().bottom - footerBounds.top));
			node.style.clipPath = `inset(${covered}px 0 0)`;
		}
		function schedule() {
			if (!frame) frame = requestAnimationFrame(update);
		}
		const observer = new ResizeObserver(schedule);
		observer.observe(document.body);
		window.addEventListener('scroll', schedule, { passive: true });
		window.addEventListener('resize', schedule);
		update();
		return {
			destroy() {
				observer.disconnect();
				window.removeEventListener('scroll', schedule);
				window.removeEventListener('resize', schedule);
				cancelAnimationFrame(frame);
			}
		};
	}
</script>

<!-- The back of the envelope: return address, the flap, and where else to write to. -->
<footer use:clipReveal class="footer" class:spacious data-doc-footer={spacious || undefined}>
	<div class="wrap">
		<div class="back sheet">
			<div class="flap" aria-hidden="true"></div>

			<div class="grid">
				<div class="return">
					<p class="label">{i18n.t('footer_return')}</p>
					<a class="sender" href={homePath(i18n.lang)}>
						<img src={logo} alt="" width="56" height="56" loading="lazy" />
						<span>
							<span class="name" lang="en">Madmail</span>
							<span class="by">{i18n.t('footer_by')}</span>
						</span>
					</a>
					<p class="motto">{i18n.t('motto')}</p>
					<p class="org">
						<a class="textlink" href="https://themadorg.net" target="_blank" rel="noopener noreferrer" lang="en">themadorg.net</a>
					</p>
				</div>

				<nav class="col" aria-labelledby="footer-product">
					<h2 id="footer-product" class="col-title">{i18n.t('footer_product')}</h2>
					<ul>
						{#each product as link (link.href)}
							<li><a href={link.href} {...external(link.href)}>{link.label}</a></li>
						{/each}
					</ul>
				</nav>

				<nav class="col" aria-labelledby="footer-community">
					<h2 id="footer-community" class="col-title">{i18n.t('footer_community')}</h2>
					<ul>
						{#each community as link (link.href)}
							<li><a href={link.href} {...external(link.href)}>{link.label}</a></li>
						{/each}
					</ul>
				</nav>

				<nav class="col" aria-labelledby="footer-languages">
					<h2 id="footer-languages" class="col-title">{i18n.t('footer_languages')}</h2>
					<ul>
						{#each languages as language (language.code)}
							<li>
								<a
									href={language.home}
									hreflang={language.code}
									lang={language.code}
									dir={language.dir}
									aria-current={language.code === i18n.lang ? 'true' : undefined}
									onclick={() => savePreference(language.code)}>{language.name}</a
								>
							</li>
						{/each}
					</ul>
				</nav>
			</div>

			<div class="foot">
				<p class="note">
					{i18n.t('footer_note')}
					<a href="{repo}/blob/main/LICENCE" target="_blank" rel="noopener noreferrer">{i18n.t('install_license')}</a>
				</p>
				<p class="year" lang="en" dir="ltr">Madmail · {year}</p>
				<a class="top" href="#top">{i18n.t('footer_top')}</a>
			</div>
		</div>
	</div>
</footer>

<style>
	.footer {
		padding-block: var(--section-gap) clamp(1.5rem, 3vw, 2.5rem);
		background: var(--paper);
		color: var(--ink);
	}

	.footer.spacious {
		position: sticky;
		bottom: 0;
		z-index: 0;
		padding-block: 4rem 2rem;
		background: var(--docs-navigation-bg, var(--paper));
	}

	@media (max-height: 600px), (prefers-reduced-motion: reduce) {
		.footer.spacious {
			position: relative;
		}
	}

	/* The envelope back: the airmail chevron runs round the edge. */
	.back {
		position: relative;
		padding: clamp(2rem, 4vw, 3.5rem);
		padding-top: clamp(5rem, 9vw, 7.5rem);
		border: 10px solid transparent;
		border-image: var(--airmail) 10 / 10px round;
		background-clip: padding-box;
	}

	/* The closed flap: two folds meeting at the seal line. */
	.flap {
		position: absolute;
		inset: 0 0 auto;
		height: clamp(3.5rem, 7vw, 5.5rem);
		background: var(--paper-2);
		clip-path: polygon(0 0, 100% 0, 50% 100%);
		pointer-events: none;
	}

	.grid {
		position: relative;
		display: grid;
		grid-template-columns: minmax(0, 5fr) repeat(3, minmax(0, 2fr));
		gap: 2.5rem clamp(1.5rem, 3vw, 3rem);
	}

	.label,
	.col-title {
		margin: 0 0 1rem;
		color: var(--red);
		font-family: var(--font-display);
		font-size: 1rem;
		font-weight: 800;
		letter-spacing: 0.14em;
		line-height: 1;
		text-transform: uppercase;
	}

	:lang(fa) .label,
	:lang(fa) .col-title {
		letter-spacing: 0;
	}

	.sender {
		display: inline-flex;
		align-items: center;
		gap: 1rem;
		color: inherit;
		text-decoration: none;
	}

	.sender img {
		width: 56px;
		height: 56px;
	}

	.name {
		display: block;
		font-family: var(--font-display);
		font-size: clamp(2.5rem, 2rem + 2vw, 3.5rem);
		font-weight: 800;
		line-height: 0.9;
		text-transform: uppercase;
	}

	.by {
		display: block;
		margin-top: 0.25rem;
		color: var(--ink-2);
		font-size: var(--step-small);
		font-weight: 700;
	}

	.motto {
		margin-top: 1.5rem;
		max-width: 22rem;
		font-family: var(--font-display);
		font-size: var(--step-h3);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
	}

	:lang(fa) .motto {
		line-height: 1.4;
	}

	.org {
		margin-top: 1rem;
	}

	ul {
		display: grid;
		gap: 0.15rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.col a {
		display: inline-flex;
		align-items: center;
		min-height: 2.25rem;
		color: var(--ink);
		font-weight: 600;
		text-decoration: none;
	}

	.col a[aria-current] {
		text-decoration: underline;
		text-decoration-color: var(--red);
		text-decoration-thickness: 2px;
		text-underline-offset: 0.3em;
	}

	.foot {
		position: relative;
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.75rem 2rem;
		margin-top: clamp(2.5rem, 5vw, 4rem);
		padding-top: 1.25rem;
		border-top: 1px solid color-mix(in oklab, var(--ink) 24%, transparent);
		color: var(--ink-2);
		font-size: var(--step-small);
	}

	.note {
		flex: 1 1 28rem;
		max-width: 46rem;
	}

	.note a {
		color: var(--ink);
		font-weight: 700;
	}

	.year {
		font-weight: 700;
	}

	.top {
		color: var(--ink);
		font-weight: 700;
		text-decoration-color: var(--red);
		text-decoration-thickness: 2px;
		text-underline-offset: 0.3em;
	}

	@media (hover: hover) and (pointer: fine) {
		.col a:hover {
			text-decoration: underline;
			text-decoration-color: var(--red);
			text-decoration-thickness: 2px;
			text-underline-offset: 0.3em;
		}
	}

	@media (max-width: 1000px) {
		.grid {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}

		.return {
			grid-column: 1 / -1;
		}
	}

	@media (max-width: 560px) {
		.grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.back {
			padding-inline: 1.25rem;
		}
	}
</style>
