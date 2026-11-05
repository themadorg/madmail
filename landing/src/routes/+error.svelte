<script>
	import { page } from '$app/state';
	import SiteNav from '$lib/components/airmail/SiteNav.svelte';
	import { homePath } from '$lib/i18n/index.js';
	import { useI18n } from '$lib/i18n/use.svelte.js';

	const status = $derived(page.status);
	const error = $derived(page.error);
	const i18n = useI18n();
	const is404 = $derived(status === 404);
	const title = $derived(is404 ? i18n.t('notfound_title') : i18n.t('error_title'));
	const body = $derived(is404 ? i18n.t('notfound_body') : i18n.t('error_body'));
</script>

<div class="airmail error-page">
	<SiteNav />
	<main id="main" class="wrap">
		<!-- An undeliverable letter comes back with the post office's slip on it. -->
		<div class="slip sheet">
			<div class="stamp" aria-hidden="true">
				<span class="hand">☚</span>
				<span lang="en" dir="ltr">{status}</span>
			</div>
			<h1>{title}</h1>
			<p class="body">{body}</p>
			{#if !is404 && error?.message}
				<p class="detail" lang="en" dir="ltr">{error.message}</p>
			{/if}
			<p class="path" lang="en" dir="ltr">{page.url.pathname}</p>
			<div class="actions">
				<a class="action" href={homePath(i18n.lang)}>{i18n.t('notfound_home')}</a>
				<a class="textlink" href="/docs">{i18n.t('notfound_docs')}</a>
			</div>
		</div>
	</main>
</div>

<style>
	.error-page {
		min-height: 100dvh;
		background: var(--paper-2);
		color: var(--ink);
	}

	main {
		display: grid;
		place-items: center;
		padding-block: clamp(3rem, 8vw, 7rem);
	}

	.slip {
		position: relative;
		width: min(100%, 40rem);
		padding: clamp(2rem, 5vw, 3.5rem);
		border: 10px solid transparent;
		border-image: var(--airmail) 10 / 10px round;
		background-clip: padding-box;
	}

	/* The rubber stamp across the corner. */
	.stamp {
		position: absolute;
		inset-block-start: 1.5rem;
		inset-inline-end: 1.5rem;
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.35rem 0.7rem 0.4rem;
		border: 3px solid var(--red);
		color: var(--red);
		font-family: var(--font-display);
		font-size: 1.125rem;
		font-weight: 800;
		letter-spacing: 0.12em;
		line-height: 1;
		text-transform: uppercase;
		rotate: -6deg;
	}

	.stamp:dir(rtl) {
		rotate: 6deg;
	}

	.hand {
		font-size: 1.4em;
		letter-spacing: 0;
	}

	h1 {
		max-width: 12ch;
		padding-inline-end: 5rem;
		font-size: var(--step-h2);
	}

	.body {
		margin-top: 1.25rem;
		max-width: 32rem;
		font-size: var(--step-lead);
	}

	.detail,
	.path {
		margin-top: 1rem;
		color: var(--ink-2);
		font-family: var(--font-code);
		font-size: var(--step-small);
		overflow-wrap: anywhere;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 1rem 1.75rem;
		margin-top: 2rem;
	}

	@media (max-width: 560px) {
		.stamp {
			position: static;
			margin-bottom: 1.5rem;
		}
	}
</style>
