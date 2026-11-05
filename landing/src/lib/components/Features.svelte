<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
	import { features } from '$lib/data.js';
</script>

<section class="features">
	<h2>{t("Features")}</h2>
	<ul>
		{#each features as f}
			<li>
				<strong>{t(f.title)}</strong>
				<span>{t(f.desc)}</span>
			</li>
		{/each}
	</ul>
</section>

<style>
	.features {
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

	strong {
		color: var(--color-text);
	}

	ul {
		display: grid;
		gap: 1.25rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	li {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		line-height: 1.5;
	}

	strong {
		font-size: 0.95rem;
	}

	span {
		font-size: 0.9rem;
		color: var(--color-text-subtle);
	}

	@media (min-width: 640px) {
		ul {
			grid-template-columns: 1fr 1fr;
		}
	}
</style>
