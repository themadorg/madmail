<script>
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import { faqKeys } from '$lib/i18n/faq.js';

	const i18n = useI18n();
</script>

<!-- Questions as a counter's enquiry cards. The same text feeds the FAQPage JSON-LD. -->
<section class="faq" id="faq" aria-labelledby="faq-title">
	<div class="wrap grid">
		<header class="head">
			<p class="counter" aria-hidden="true">
				<span class="window">{i18n.t('faq_counter')}</span>
			</p>
			<h2 id="faq-title">{i18n.t('faq_title')}</h2>
		</header>

		<ol class="cards">
			{#each faqKeys as key, index (key)}
				<li class="card sheet">
					<details open={index < 2}>
						<summary>
							<span class="no" aria-hidden="true" lang="en" dir="ltr">{String(index + 1).padStart(2, '0')}</span>
							<h3>{i18n.t(`faq_${key}_q`)}</h3>
							<span class="toggle" aria-hidden="true"></span>
						</summary>
						<p>{i18n.t(`faq_${key}_a`)}</p>
					</details>
				</li>
			{/each}
		</ol>
	</div>
</section>

<style>
	.faq {
		padding-block: var(--section-gap) 0;
	}

	.grid {
		display: grid;
		grid-template-columns: minmax(0, 4fr) minmax(0, 8fr);
		gap: 2rem clamp(2rem, 5vw, 5rem);
		align-items: start;
	}

	.head {
		position: sticky;
		top: 6rem;
	}

	.counter {
		margin-bottom: 1.25rem;
	}

	.window {
		display: inline-block;
		padding: 0.35rem 0.75rem 0.4rem;
		background: var(--blue-field);
		color: var(--blue-field-ink);
		font-family: var(--font-display);
		font-size: 0.9375rem;
		font-weight: 800;
		letter-spacing: 0.16em;
		line-height: 1;
		text-transform: uppercase;
	}

	h2 {
		font-size: var(--step-h2);
	}

	.cards {
		display: grid;
		gap: 0;
		margin: 0;
		padding: 0;
		list-style: none;
		border-block-start: 3px solid var(--ink);
	}

	/* Filed cards: a ruled index card each, the number punched at the head. */
	.card {
		--paper: var(--sheet);
		border-block-end: 1px solid color-mix(in oklab, var(--ink) 22%, transparent);
		background-image: repeating-linear-gradient(
			180deg,
			transparent 0 calc(2rem - 1px),
			color-mix(in oklab, var(--blue) 14%, transparent) calc(2rem - 1px) 2rem
		);
		background-position: 0 4.5rem;
		background-repeat: no-repeat;
		background-size: 100% calc(100% - 4.5rem);
	}

	summary {
		display: grid;
		grid-template-columns: 3rem minmax(0, 1fr) 1.5rem;
		gap: 1rem;
		align-items: center;
		min-height: 4.5rem;
		padding: 1rem clamp(1rem, 2vw, 1.5rem);
		cursor: pointer;
		list-style: none;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	.no {
		color: var(--red);
		font-family: var(--font-display);
		font-size: 1.5rem;
		font-weight: 800;
		line-height: 1;
	}

	h3 {
		font-family: var(--font-text);
		font-size: var(--step-lead);
		font-weight: 700;
		letter-spacing: 0;
		line-height: 1.25;
		text-transform: none;
	}

	.toggle {
		position: relative;
		width: 1.25rem;
		height: 1.25rem;
	}

	.toggle::before,
	.toggle::after {
		content: '';
		position: absolute;
		inset: 50% 0 auto;
		height: 3px;
		margin-top: -1.5px;
		background: var(--ink);
		transition: rotate var(--dur-ui) var(--ease-out);
	}

	.toggle::after {
		rotate: 90deg;
	}

	details[open] .toggle::after {
		rotate: 0deg;
	}

	details p {
		padding-block: 0 1.5rem;
		padding-inline: calc(clamp(1rem, 2vw, 1.5rem) + 4rem) clamp(1rem, 2vw, 1.5rem);
		max-width: 46rem;
		line-height: 2rem;
	}

	:lang(fa) details p {
		line-height: 2rem;
	}

	summary:focus-visible {
		outline-offset: -3px;
	}

	@media (hover: hover) and (pointer: fine) {
		summary:hover h3 {
			text-decoration: underline;
			text-decoration-color: var(--red);
			text-decoration-thickness: 2px;
			text-underline-offset: 0.3em;
		}
	}

	@media (max-width: 900px) {
		.grid {
			grid-template-columns: minmax(0, 1fr);
		}

		.head {
			position: static;
		}
	}

	@media (max-width: 560px) {
		summary {
			grid-template-columns: 2.25rem minmax(0, 1fr) 1.25rem;
			gap: 0.75rem;
		}

		details p {
			padding-inline: 1rem;
		}
	}
</style>
