<script>
	import { releases } from '$lib/releases.js';
	import Button from '$lib/components/Button.svelte';
	import MadGlitchText from '$lib/components/MadGlitchText.svelte';
	import MadmailLogo from '$lib/components/MadmailLogo.svelte';
	import NavLinks from '$lib/components/NavLinks.svelte';
	import { heroNav, repo } from '$lib/nav.js';

	const latest = releases[0] ?? null;

	let { onDeltachat = () => {} } = $props();
</script>

<section class="hero">
	<MadmailLogo class="hero-logo" interactive transitionName="madmail-logo" />
	<h1><MadGlitchText text="madmail v2" /></h1>
	<p class="subtitle">
		Bringing the madness to mail delivery.
		<button type="button" class="hashtag" onclick={onDeltachat}>#deltachat</button>
	</p>
	<p class="tagline">
		Rust chatmail relay for
		<button type="button" class="link" onclick={onDeltachat}>Delta Chat</button>, encrypted,
		federated, one binary.
	</p>
	<NavLinks links={heroNav} class="nav" ariaLabel="Site navigation" />
	<div class="actions">
		<Button href="{repo}/releases" variant="primary">Download</Button>
		<Button href="/docs/quick-setup">Quick Start</Button>
	</div>
	{#if latest}
		<a class="latest-release" href="/releases">{latest.tag_name}</a>
	{/if}
</section>

<style>
	.hero {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 100vh;
		min-height: 100dvh;
		padding: 2rem 1.5rem;
		text-align: center;
	}

	:global(.hero-logo) {
		margin-bottom: 0.5rem;
	}

	h1 {
		margin: 0 0 2rem;
		font-size: clamp(2rem, 6vw, 2.75rem);
		font-weight: 700;
		letter-spacing: 0;
		text-transform: lowercase;
		color: var(--color-text);
	}

	.subtitle {
		max-width: 34rem;
		margin: 0 0 1rem;
		font-size: 1.15rem;
		line-height: 1.5;
		color: var(--color-text);
	}

	.hashtag {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: var(--color-text);
		cursor: pointer;
	}

	.hashtag:hover {
		color: var(--color-text);
		text-decoration: underline;
	}

	.tagline {
		max-width: 34rem;
		margin: 0 0 1.75rem;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--color-text-muted);
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}

	.link:hover {
		color: var(--color-text);
		text-decoration: underline;
	}

	:global(.nav) {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 0.35rem 1.25rem;
		margin-bottom: 2rem;
	}

	:global(.nav a) {
		font-size: 0.95rem;
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	:global(.nav a:hover) {
		color: var(--color-text);
		text-decoration: underline;
	}

	.latest-release {
		margin-top: 1.25rem;
		font-size: 0.9rem;
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	.latest-release:hover {
		color: var(--color-text);
		text-decoration: underline;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 0.75rem;
	}
</style>
