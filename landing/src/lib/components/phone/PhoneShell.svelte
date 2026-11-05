<script>
	import MadmailLogo from '$lib/components/MadmailLogo.svelte';
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import { latestPhone, phonePages, phoneReleasesUrl } from '$lib/phone.js';

	/** @type {{ current: string, children: import('svelte').Snippet }} */
	let { current, children } = $props();

	const downloadHref = latestPhone?.apk?.browser_download_url ?? phoneReleasesUrl;
</script>

<div class="phone-shell">
	<header class="bar">
		<a class="brand" href="/phone">
			<MadmailLogo size="1.75rem" alt="" />
			<span>Madmail Mobile</span>
		</a>
		<nav aria-label="Madmail Mobile">
			{#each phonePages as page}
				<a href={page.href} aria-current={current === page.href ? 'page' : undefined}>{page.label}</a>
			{/each}
			<a href="/">Server</a>
		</nav>
		<a class="pill pill--small" href={downloadHref}>Download</a>
	</header>

	<main>
		{@render children()}
	</main>

	<SiteFooter />
</div>

<style>
	.phone-shell {
		--pm-red: #e5383b;
		--pm-red-hover: #f04a4d;
		--pm-panel: #1e1b1a;
		--pm-group: #2b2726;
		--pm-divider: rgb(255 255 255 / 0.07);
		--pm-ink: #f4efed;
		--pm-ink-soft: #b3aaa6;
		min-height: 100dvh;
		background: var(--color-bg);
		color: var(--color-text);
		transition: var(--transition-theme);
	}

	:global(:root[data-light]) .phone-shell {
		--pm-panel: #f3f0ef;
		--pm-group: #ffffff;
		--pm-divider: rgb(0 0 0 / 0.07);
		--pm-ink: #1c1918;
		--pm-ink-soft: #6f6763;
	}

	.bar {
		position: sticky;
		top: 0;
		z-index: 10;
		display: flex;
		align-items: center;
		gap: 1.5rem;
		max-width: 72rem;
		margin: 0 auto;
		padding: 0.85rem 1.5rem;
		background: color-mix(in srgb, var(--color-bg) 82%, transparent);
		backdrop-filter: blur(14px);
		-webkit-backdrop-filter: blur(14px);
	}

	.brand {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-weight: 650;
		font-size: 0.95rem;
		color: var(--color-text);
		text-decoration: none;
		white-space: nowrap;
	}

	nav {
		display: flex;
		gap: 1.25rem;
		margin-left: auto;
	}

	nav a {
		font-size: 0.9rem;
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	nav a:hover,
	nav a[aria-current='page'] {
		color: var(--color-text);
	}

	nav a[aria-current='page'] {
		text-decoration: underline;
		text-underline-offset: 0.35em;
		text-decoration-thickness: 1.5px;
	}

	/* The red pill is the app's one primary action; the site reuses it for Download. */
	.phone-shell :global(.pill) {
		display: inline-flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.8rem 1.4rem;
		border-radius: 999px;
		background: var(--pm-red);
		color: #fff;
		font-weight: 600;
		font-size: 0.98rem;
		text-decoration: none;
		white-space: nowrap;
		transition: background 0.15s;
	}

	.phone-shell :global(.pill:hover) {
		background: var(--pm-red-hover);
	}

	.phone-shell :global(.pill--small) {
		padding: 0.45rem 1rem;
		font-size: 0.85rem;
	}

	.phone-shell :global(.pill--soft) {
		background: color-mix(in srgb, var(--pm-red) 14%, transparent);
		color: var(--color-text);
	}

	.phone-shell :global(.pill--soft:hover) {
		background: color-mix(in srgb, var(--pm-red) 24%, transparent);
	}

	/* Shared layout for the Install and Questions pages. */
	.phone-shell :global(.doc) {
		max-width: 46rem;
		margin: 0 auto;
		padding: 3.5rem 1.5rem 2rem;
	}

	.phone-shell :global(.doc h1) {
		margin: 0 0 1rem;
		font-size: clamp(2.2rem, 5vw, 3.2rem);
		font-weight: 800;
		line-height: 1.02;
		letter-spacing: -0.04em;
	}

	.phone-shell :global(:is(h2, details)) {
		scroll-margin-top: 5rem;
	}

	.phone-shell :global(.doc h2) {
		margin: 3.5rem 0 1rem;
		font-size: 1.45rem;
		font-weight: 700;
		letter-spacing: -0.02em;
	}

	.phone-shell :global(.doc p),
	.phone-shell :global(.doc li) {
		line-height: 1.65;
		color: var(--color-text-muted);
	}

	.phone-shell :global(.doc .lede) {
		margin: 0;
		font-size: 1.1rem;
	}

	.phone-shell :global(.doc a:not(.pill)) {
		color: var(--color-text);
		text-underline-offset: 0.2em;
	}

	.phone-shell :global(.doc strong) {
		color: var(--color-text);
	}

	.phone-shell :global(a:focus-visible),
	.phone-shell :global(button:focus-visible),
	.phone-shell :global(summary:focus-visible) {
		outline: 2px solid var(--pm-red);
		outline-offset: 3px;
	}

	@media (max-width: 640px) {
		.bar {
			flex-wrap: wrap;
			gap: 0.6rem 1rem;
			padding: 0.75rem 1rem;
		}

		nav {
			order: 3;
			width: 100%;
			margin-left: 0;
			gap: 1.1rem;
			overflow-x: auto;
		}

		.pill--small {
			margin-left: auto;
		}
	}
</style>
