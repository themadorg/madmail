<script>
	import { useI18n } from '$lib/i18n/use.svelte.js';

	const i18n = useI18n();

	/**
	 * Each service is one pigeonhole in the relay's sorting rack. `mail` is the
	 * post waiting in it: envelope heights (in the 64px-tall drawing), and which
	 * one carries the airmail edge.
	 */
	const holes = [
		{ id: 'mail', mail: [48, 42, 54], air: 2 },
		{ id: 'pgp', mail: [46], air: 0 },
		{ id: 'jit', mail: [44, 50], air: -1 },
		{ id: 'fed', mail: [54, 48, 44], air: 0 },
		{ id: 'nolog', mail: [42], air: -1 },
		{ id: 'calls', mail: [50, 44], air: 1 },
		{ id: 'tls', mail: [48], air: 0 },
		{ id: 'admin', mail: [40, 52, 46], air: 1 }
	];
</script>

<section class="office" id="features" aria-labelledby="office-title">
	<div class="wrap layout">
		<header class="head">
			<h2 id="office-title">{i18n.t('sort_title')}</h2>
			<p>{i18n.t('sort_lede')}</p>
		</header>

		<div class="rack sheet">
			<p class="sign" aria-hidden="true">{i18n.t('sort_sign')}</p>
			<ul class="holes" aria-label={i18n.t('sort_rack_label')}>
				{#each holes as hole (hole.id)}
					<li class="hole">
						<h3 class="plate">{i18n.t(`f_${hole.id}`)}</h3>
						<p>{i18n.t(`f_${hole.id}_body`)}</p>
						<svg class="post" viewBox="0 0 200 64" aria-hidden="true">
							{#each hole.mail as h, n (n)}
								{@const x = 4 + n * 34}
								{@const w = 72}
								{@const y = 64 - h}
								<g transform="rotate({n % 2 ? 2.5 : -2} {x + w / 2} 64)">
									<rect x={x} y={y} width={w} height={h} fill="var(--paper)" stroke="currentColor" stroke-width="2" />
									{#if n === hole.air}
										<rect x={x + 3} y={y + 3} width={w - 6} height={h - 6} fill="none" stroke="#d7262e" stroke-width="3" stroke-dasharray="5 5" />
										<rect x={x + 3} y={y + 3} width={w - 6} height={h - 6} fill="none" stroke="#1d3a8a" stroke-width="3" stroke-dasharray="5 5" stroke-dashoffset="5" />
									{/if}
									<path d="M{x} {y} L{x + w / 2} {y + h * 0.48} L{x + w} {y}" fill="none" stroke="currentColor" stroke-width="1.5" />
									<rect x={x + w - 16} y={y + 6} width="10" height="12" fill={n % 2 ? '#1d3a8a' : '#c41f28'} />
								</g>
							{/each}
						</svg>
					</li>
				{/each}
			</ul>
		</div>
	</div>
</section>

<style>
	.office {
		padding-block: var(--section-gap) 0;
	}

	.layout {
		display: grid;
		gap: clamp(2.5rem, 5vw, 4rem);
	}

	.head {
		display: grid;
		grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
		gap: 1.25rem clamp(2rem, 5vw, 5rem);
		align-items: end;
	}

	h2 {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--step-h2);
		font-weight: 800;
		line-height: 0.92;
		letter-spacing: -0.01em;
		text-transform: uppercase;
		text-wrap: balance;
	}

	:lang(fa) h2 {
		line-height: 1.25;
		letter-spacing: 0;
	}

	.head p {
		max-width: 34rem;
		margin: 0;
		font-size: var(--step-lead);
		text-wrap: pretty;
	}

	/* The rack: a wooden-less, ink-ruled set of pigeonholes printed on paper.
	   The shelves are real rules because the rack is a real object. */
	.rack {
		border: 3px solid var(--ink);
		outline: 0.375rem solid var(--paper);
	}

	.sign {
		margin: 0;
		padding: 0.6rem 1.25rem 0.55rem;
		border-bottom: 3px solid var(--ink);
		background: var(--blue-field);
		color: var(--blue-field-ink);
		font-family: var(--font-display);
		font-size: var(--step-h3);
		font-weight: 800;
		line-height: 1;
		text-align: center;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	:lang(fa) .sign {
		letter-spacing: 0;
	}

	.holes {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.hole {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		min-height: 15rem;
		padding: 1.1rem 1.1rem 0;
		background: var(--paper-2);
		/* Each hole draws its own inline-start and block-end partitions. */
		border-inline-start: 2px solid var(--ink);
		border-block-end: 2px solid var(--ink);
	}

	.hole:nth-child(4n + 1) {
		border-inline-start: 0;
	}

	.hole:nth-last-child(-n + 4) {
		border-block-end: 0;
	}

	/* The label plate on the front of each pigeonhole. */
	.plate {
		align-self: start;
		margin: 0;
		padding: 0.3rem 0.55rem 0.25rem;
		background: var(--paper);
		border: 2px solid var(--ink);
		font-family: var(--font-display);
		font-size: var(--step-lead);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
	}

	:lang(fa) .plate {
		line-height: 1.4;
	}

	.hole p {
		margin: 0;
		font-size: var(--step-small);
		line-height: 1.5;
		text-wrap: pretty;
	}

	.post {
		width: 200px;
		height: 64px;
		margin-top: auto;
		color: var(--ink);
		overflow: visible;
	}

	@media (max-width: 1100px) {
		.holes {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}

		.hole {
			border-inline-start: 2px solid var(--ink);
			border-block-end: 2px solid var(--ink);
		}

		.hole:nth-child(4n + 1) {
			border-inline-start: 2px solid var(--ink);
		}

		.hole:nth-child(2n + 1) {
			border-inline-start: 0;
		}

		.hole:nth-last-child(-n + 4) {
			border-block-end: 2px solid var(--ink);
		}

		.hole:nth-last-child(-n + 2) {
			border-block-end: 0;
		}
	}

	@media (max-width: 860px) {
		.head {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	@media (max-width: 560px) {
		.holes {
			grid-template-columns: minmax(0, 1fr);
		}

		.hole,
		.hole:nth-child(2n + 1) {
			min-height: 0;
			padding-bottom: 0;
			border-inline-start: 0;
			border-block-end: 2px solid var(--ink);
		}

		.hole:last-child {
			border-block-end: 0;
		}

		.post {
			width: 160px;
			height: 51px;
		}
	}
</style>
