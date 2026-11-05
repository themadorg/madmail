<script>
	import { openCommandPalette } from '$lib/commandPalette.svelte.js';
	import { homePath } from '$lib/i18n/index.js';
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import logo from '$lib/logoUrl.js';
	import { repo } from '$lib/nav.js';
	import LangSwitch from './LangSwitch.svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	const i18n = useI18n();
</script>

<a class="skip" href="#main">{i18n.t('skip')}</a>

<header class="nav">
	<div class="inner">
		<a href={homePath(i18n.lang)} class="brand" aria-label={i18n.t('nav_home')}>
			<img src={logo} alt="" width="34" height="34" />
			<span class="name" lang="en">Madmail</span>
		</a>

		<nav class="links" aria-label={i18n.t('nav_primary')}>
			<a href="/docs">{i18n.t('nav_docs')}</a>
			<a href="{homePath(i18n.lang)}#install">{i18n.t('nav_install')}</a>
			<a href="{homePath(i18n.lang)}#route">{i18n.t('nav_route')}</a>
			<a href="/releases">{i18n.t('nav_releases')}</a>
			<a href="/blog">{i18n.t('nav_blog')}</a>
			<a href="/phone">{i18n.t('nav_mobile')}</a>
		</nav>

		<div class="tools">
			<button type="button" class="tool" aria-label={i18n.t('nav_search')} title={i18n.t('nav_search')} onclick={() => openCommandPalette()}>
				<svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
					<circle cx="10.5" cy="10.5" r="6.5" stroke="currentColor" stroke-width="2" />
					<path d="m15.5 15.5 5 5" stroke="currentColor" stroke-width="2" />
				</svg>
			</button>
			<LangSwitch />
			<ThemeToggle />
			<a class="action download" href="{repo}/releases/latest" target="_blank" rel="noopener noreferrer">{i18n.t('nav_download')}</a>
		</div>
	</div>
</header>

<style>
	.skip {
		position: absolute;
		inset-inline-start: 1rem;
		top: -4rem;
		z-index: 100;
		padding: 0.75rem 1rem;
		background: var(--red);
		color: var(--on-red);
		font-weight: 700;
		text-decoration: none;
	}

	.skip:focus {
		top: 1rem;
	}

	.nav {
		position: sticky;
		top: 0;
		z-index: 40;
		background: var(--paper);
	}

	.inner {
		display: flex;
		align-items: center;
		gap: 1.5rem;
		max-width: calc(var(--content-max) + 2 * var(--margin));
		margin-inline: auto;
		padding: 0.75rem var(--margin);
	}

	.brand {
		display: inline-flex;
		align-items: center;
		gap: 0.6rem;
		color: var(--ink);
		text-decoration: none;
	}

	.brand img {
		width: 34px;
		height: 34px;
	}

	.name {
		font-family: var(--font-display);
		font-size: var(--step-h3);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
	}

	.links {
		display: flex;
		align-items: center;
		gap: 0.25rem 1.5rem;
		margin-inline-start: auto;
	}

	.links a {
		color: var(--ink);
		font-size: var(--step-small);
		font-weight: 600;
		text-decoration: underline;
		text-decoration-color: transparent;
		text-decoration-thickness: 2px;
		text-underline-offset: 0.35em;
		transition: text-decoration-color var(--dur-ui) var(--ease-out);
	}

	@media (hover: hover) and (pointer: fine) {
		.links a:hover {
			text-decoration-color: var(--red);
		}
	}

	.tools {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.tool {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2.5rem;
		height: 2.5rem;
		padding: 0;
		border: 0;
		background: var(--paper-2);
		color: var(--ink);
		cursor: pointer;
		transition:
			background-color var(--dur-ui) var(--ease-out),
			transform var(--dur-press) var(--ease-out);
	}

	.tool:active {
		transform: scale(0.94);
	}

	@media (hover: hover) and (pointer: fine) {
		.tool:hover {
			background: var(--paper-3);
		}
	}

	.download {
		min-height: 2.5rem;
		padding: 0.5rem 1rem;
		font-size: var(--step-small);
	}

	@media (max-width: 1100px) {
		.links {
			display: none;
		}

		.tools {
			margin-inline-start: auto;
		}
	}

	@media (max-width: 480px) {
		.name {
			display: none;
		}

		.inner {
			gap: 0.75rem;
		}

		.download {
			padding-inline: 0.75rem;
		}
	}
</style>
