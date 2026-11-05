<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
	import { browser } from '$app/environment';
	import Button from '$lib/components/Button.svelte';
	import MadGlitchText from '$lib/components/MadGlitchText.svelte';
	import MadmailLogo from '$lib/components/MadmailLogo.svelte';
	import chevronLeft from '$lib/icons/chevron-left.svg?raw';

	/** @type {{ error: App.Error, status: number }} */
	let { error, status } = $props();

	const is404 = $derived(status === 404);
	const title = $derived(is404 ? '404' : String(status));
	const headline = $derived(is404 ? 'Page not found' : 'Something went wrong');
	const detail = $derived(
		is404
			? "This page doesn't exist or was moved."
			: (error?.message ?? 'An unexpected error occurred.')
	);

	function goBack() {
		if (!browser) return;
		if (history.length > 1) {
			history.back();
		} else {
			window.location.href = '/';
		}
	}
</script>

<div class="error-page">
	<header class="error-top">
		<div class="error-top__inner">
			<button type="button" class="back" aria-label={t("Back")} onclick={goBack}>
				<span class="icon" aria-hidden="true">{@html chevronLeft}</span>
			</button>
			<MadmailLogo href="/" size="2rem" class="error-logo" transitionName="madmail-logo" />
		</div>
	</header>

	<main class="error-main">
		<h1 class="code">
			<MadGlitchText text={title} />
		</h1>
		<p class="headline">{t(headline)}</p>
		<p class="detail">{t(detail)}</p>
		<div class="actions">
			<Button href="/" variant="primary">{t('Back to home')}</Button>
			<Button href="/docs">{t('Documentation')}</Button>
		</div>
	</main>
</div>

<style>
	.error-page {
		display: flex;
		flex-direction: column;
		min-height: 100vh;
		min-height: 100dvh;
		background: var(--color-bg);
		color: var(--color-text);
		transition: var(--transition-theme);
	}

	.error-top {
		flex-shrink: 0;
		height: 5rem;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg);
		transition: var(--transition-theme);
	}

	.error-top__inner {
		display: grid;
		grid-template-columns: 1fr auto 1fr;
		align-items: center;
		height: 100%;
		max-width: 48rem;
		margin: 0 auto;
		padding: 0 1.5rem;
	}

	.back {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 2.25rem;
		padding: 0;
		border: none;
		border-radius: 0.5rem;
		background: transparent;
		color: var(--color-text-subtle);
		cursor: pointer;
		grid-column: 1;
		justify-self: start;
	}

	.back:hover {
		background: var(--color-hover);
		color: var(--color-text);
	}

	.error-top :global(.error-logo) {
		grid-column: 2;
		justify-self: center;
	}

	.icon :global(svg) {
		display: block;
		width: 1.25rem;
		height: 1.25rem;
	}

	.error-main {
		display: flex;
		flex: 1;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 2rem 1.5rem 3rem;
		text-align: center;
	}

	.code {
		margin: 0 0 1rem;
		font-size: clamp(4rem, 18vw, 7rem);
		font-weight: 700;
		line-height: 1;
		letter-spacing: -0.04em;
	}

	.headline {
		margin: 0 0 0.5rem;
		font-size: clamp(1.25rem, 4vw, 1.5rem);
		font-weight: 600;
	}

	.detail {
		margin: 0 0 2rem;
		max-width: 28rem;
		font-size: 1rem;
		line-height: 1.6;
		color: var(--color-text-muted);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
	}
</style>
