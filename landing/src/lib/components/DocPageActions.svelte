<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
	import CopyPageButton from '$lib/components/CopyPageButton.svelte';
	import EditPageButton from '$lib/components/EditPageButton.svelte';

	/** @type {{ currentHref?: string, variant?: 'header' | 'footer' }} */
	let { currentHref = '', variant = 'header' } = $props();
</script>

<div class="doc-page-actions" class:footer={variant === 'footer'} aria-label={t("Page actions")}>
	<EditPageButton {currentHref} />
	<CopyPageButton {currentHref} />
</div>

<style>
	.doc-page-actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.doc-page-actions.footer {
		display: none;
		max-width: 48rem;
		margin: 2rem auto 0;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
		justify-content: flex-end;
	}

	@media (max-width: 640px) {
		.doc-page-actions:not(.footer) {
			display: none;
		}

		.doc-page-actions.footer {
			display: flex;
		}
	}
</style>
