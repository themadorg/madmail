<script>
	import { useI18n } from '$lib/i18n/use.svelte.js';

	const i18n = useI18n();

	/* Five stops; the middle two are Madmail's own work. */
	const stops = [
		{ name: 'route_app', note: 'route_app_note', ours: false },
		{ name: 'route_relay', note: 'route_relay_note', ours: true },
		{ name: 'route_fed', note: 'route_fed_note', ours: true },
		{ name: 'route_relay2', note: 'route_relay2_note', ours: false },
		{ name: 'route_app2', note: 'route_app2_note', ours: false }
	];

	/* The map key. Each symbol is drawn from the route's own vocabulary. */
	const key = [
		{ text: 'route_key_admin', sym: 'relay' },
		{ text: 'route_key_mobile', sym: 'phone' },
		{ text: 'route_key_calls', sym: 'ping' },
		{ text: 'route_key_nolog', sym: 'sheet' }
	];

	/* Airmail band, drawn at the route line's 20px scale. */
	const bands = [-10, 0, 10, 20, 30, 40, 50];

	/** @type {HTMLElement | undefined} */
	let trackEl = $state();
	/** @type {SVGSVGElement | undefined} */
	let letterEl = $state();
	let flying = $state(false);
	/* How many stops the letter has passed. Each one is cancelled as it goes by. */
	let passed = $state(0);

	function follow() {
		if (!trackEl || !letterEl) return;
		const nodes = Array.from(trackEl.querySelectorAll('.node'));
		const vertical = nodes.length > 1 && nodes[1].getBoundingClientRect().top - nodes[0].getBoundingClientRect().top > 8;
		const rtl = getComputedStyle(trackEl).direction === 'rtl';
		const started = performance.now();

		const tick = () => {
			if (!letterEl) return;
			const l = letterEl.getBoundingClientRect();
			const lx = l.left + l.width / 2;
			const ly = l.top + l.height / 2;
			let n = 0;
			for (const node of nodes) {
				const r = node.getBoundingClientRect();
				const nx = r.left + r.width / 2;
				const ny = r.top + r.height / 2;
				const past = vertical ? ly >= ny - 2 : rtl ? lx <= nx + 2 : lx >= nx - 2;
				if (!past) break;
				n++;
			}
			if (n > passed) passed = n;
			if (passed < nodes.length && performance.now() - started < 4000) requestAnimationFrame(tick);
			else passed = nodes.length;
		};
		requestAnimationFrame(tick);
	}

	$effect(() => {
		if (!trackEl || typeof IntersectionObserver === 'undefined') return;
		if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
			passed = stops.length;
			return;
		}
		const io = new IntersectionObserver(
			([entry]) => {
				if (entry.isIntersecting) {
					io.disconnect();
					flying = true;
					follow();
				}
			},
			{ rootMargin: '0px 0px -38% 0px', threshold: 0 }
		);
		io.observe(trackEl);
		return () => io.disconnect();
	});
</script>

<section class="route" id="route" aria-labelledby="route-title">
	<div class="wrap map">
		<header class="head">
			<h2 id="route-title">{i18n.t('route_title')}</h2>
			<p>{i18n.t('route_lede')}</p>
		</header>

		<div class="track" class:flying bind:this={trackEl}>
			<div class="line" aria-hidden="true"></div>
			<ol class="stops">
				{#each stops as stop, i (stop.name)}
					<li class:ours={stop.ours} class:hit={i < passed}>
						<span class="node" aria-hidden="true"></span>
						<svg class="cancel" viewBox="0 0 84 48" aria-hidden="true">
							<g fill="none" stroke="currentColor">
								<circle cx="24" cy="24" r="22" stroke-width="2.5" />
								<path
									stroke-width="2.5"
									d="M50 17 q 7 -4 14 0 t 14 0 M50 31 q 7 -4 14 0 t 14 0"
								/>
							</g>
						</svg>
						<h3>{i18n.t(stop.name)}</h3>
						<p>{i18n.t(stop.note)}</p>
					</li>
				{/each}
			</ol>
			<div class="flight" aria-hidden="true">
				<svg class="letter sheet" bind:this={letterEl} width="44" height="30" viewBox="0 0 44 30">
					<rect x="1.5" y="1.5" width="41" height="27" fill="var(--paper)" stroke="currentColor" stroke-width="3" />
					<path d="M2 3 22 17 42 3" fill="none" stroke="currentColor" stroke-width="3" />
				</svg>
			</div>
		</div>

		<aside class="key" aria-labelledby="route-key">
			<h3 id="route-key">{i18n.t('route_key_title')}</h3>
			<ul>
				{#each key as item (item.text)}
					<li>
						<svg class="sym" viewBox="0 0 48 24" aria-hidden="true">
							{#if item.sym === 'relay'}
								<g>
									{#each bands as x (x)}
										<polygon fill="#d7262e" points="{x},16 {x + 8},8 {x + 13},8 {x + 5},16" />
										<polygon fill="#1d3a8a" points="{x + 10},16 {x + 18},8 {x + 23},8 {x + 15},16" />
									{/each}
								</g>
								<circle cx="24" cy="12" r="9.5" fill="var(--red)" />
								<circle cx="24" cy="12" r="4" fill="var(--paper)" />
							{:else if item.sym === 'ping'}
								<g fill="var(--paper)" stroke="currentColor" stroke-width="3">
									<circle cx="7" cy="12" r="5" />
									<circle cx="41" cy="12" r="5" />
								</g>
								<path d="M15 12h18" stroke="currentColor" stroke-width="2.5" stroke-dasharray="3 3" />
							{:else if item.sym === 'phone'}
								<rect x="16" y="1.5" width="16" height="21" fill="var(--paper)" stroke="currentColor" stroke-width="2.5" />
								<circle cx="24" cy="12" r="4" fill="var(--red)" />
								<path d="M3 12h9M36 12h9" stroke="var(--blue)" stroke-width="2.5" stroke-dasharray="3 3" />
							{:else}
								<rect x="14" y="1.5" width="20" height="21" fill="var(--paper)" stroke="currentColor" stroke-width="2.5" />
								<path d="M18 8h12M18 12h12M18 16h8" stroke="currentColor" stroke-width="2" />
								<path d="M10 22 38 2" stroke="var(--red)" stroke-width="3" />
							{/if}
						</svg>
						<span>{i18n.t(item.text)}</span>
					</li>
				{/each}
			</ul>
		</aside>
	</div>
</section>

<style>
	.route {
		padding-block: var(--section-gap) 0;
	}

	/* A route map: title block at the start, the key in the end corner, the route across. */
	.map {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(19rem, 27rem);
		grid-template-areas:
			'head key'
			'track track';
		column-gap: clamp(2.5rem, 6vw, 6rem);
		row-gap: clamp(3rem, 5vw, 4.5rem);
		align-items: end;
	}

	.head {
		grid-area: head;
	}

	h2 {
		font-size: var(--step-h2);
	}

	.head p {
		max-width: 34rem;
		margin-top: 1.25rem;
		font-size: var(--step-lead);
	}

	/* ---- The key ---- */

	.key {
		grid-area: key;
		padding: 1.5rem 1.25rem 1.25rem;
		border: 2px solid var(--ink);
	}

	/* The caption sits in the key's top rule, as on a printed map. */
	.key h3 {
		width: fit-content;
		margin-block: calc(-1.5rem - 0.5em - 1px) 1rem;
		margin-inline-start: -0.5rem;
		padding-inline: 0.5rem;
		background: var(--paper);
		font-size: var(--step-lead);
		line-height: 1;
	}

	.key ul {
		display: grid;
		gap: 0.85rem;
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: var(--step-small);
		line-height: 1.45;
	}

	.key li {
		display: grid;
		grid-template-columns: 48px minmax(0, 1fr);
		gap: 0.9rem;
		align-items: center;
	}

	.sym {
		display: block;
		width: 48px;
		height: 24px;
		color: var(--ink);
	}

	/* ---- The route ---- */

	.track {
		grid-area: track;
		position: relative;
	}

	/* The route itself, in airmail bands. */
	.line,
	.flight {
		position: absolute;
		inset-inline: 0;
		top: 13px;
		height: 8px;
	}

	.line {
		background: var(--airmail);
		background-size: 20px 20px;
	}

	/* The letter's lane: same box as the line, above the stops, sized so the
	   glyph can travel in container units without measuring. */
	.flight {
		z-index: 2;
		container-type: size;
		pointer-events: none;
	}

	/* Delivered: where the letter rests without script or motion. */
	.letter {
		position: absolute;
		top: -11px;
		inset-inline-end: 0;
		display: block;
		max-width: none;
		background: none;
	}

	@media (prefers-reduced-motion: no-preference) {
		:global(.js) .letter {
			inset-inline-start: 0;
			inset-inline-end: auto;
			translate: 0 0;
			transition: translate 2.4s var(--ease-in-out);
		}

		:global(.js) .flying .letter {
			translate: calc(100cqi - 44px) 0;
		}

		:global(.js) .flying:dir(rtl) .letter {
			translate: calc(-100cqi + 44px) 0;
		}

		/* Each stop is cancelled as the letter passes it. */
		:global(.js) .cancel {
			opacity: 0;
			scale: 1.35;
			transition:
				opacity var(--dur-ui) var(--ease-out),
				scale var(--dur-ui) var(--ease-out);
		}

		:global(.js) .hit .cancel {
			opacity: 0.82;
			scale: 1;
		}
	}

	.stops {
		position: relative;
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: 1.5rem;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.stops li {
		position: relative;
	}

	.node {
		position: relative;
		display: block;
		width: 34px;
		height: 34px;
		margin-bottom: 1.5rem;
		border: 5px solid var(--ink);
		border-radius: 50%;
		background: var(--paper);
	}

	.ours .node {
		border-color: var(--red);
		background: var(--red);
		outline: 5px solid var(--paper);
		outline-offset: -10px;
	}

	/* A small hand cancellation: ring around the stop, waves trailing behind it. */
	.cancel {
		position: absolute;
		top: -7px;
		inset-inline-start: -7px;
		z-index: 1;
		width: 84px;
		height: 48px;
		color: var(--ink);
		opacity: 0.82;
		transform-origin: 24px 24px;
		pointer-events: none;
	}

	/* RTL: the ring sits at the other end and the waves trail the other way. */
	.cancel:dir(rtl) {
		transform-origin: 60px 24px;
	}

	.cancel:dir(rtl) g {
		transform: scaleX(-1);
		transform-box: view-box;
		transform-origin: 42px 24px;
	}

	.stops h3 {
		font-size: var(--step-h3);
	}

	.ours h3 {
		color: var(--red);
	}

	.stops p {
		margin-top: 0.5rem;
		font-size: var(--step-small);
		line-height: 1.45;
	}

	@media (max-width: 1100px) and (min-width: 861px) {
		.map {
			grid-template-columns: minmax(0, 1fr) minmax(17rem, 22rem);
			column-gap: 2.5rem;
		}
	}

	@media (max-width: 860px) {
		.map {
			grid-template-columns: minmax(0, 1fr);
			grid-template-areas:
				'head'
				'track'
				'key';
		}

		/* The line runs on past the last stop, so the delivered letter rests clear of it.
		   The extra 7px keeps the cancellation rings inside the page margin. */
		.track {
			padding-inline-start: calc(3.5rem + 7px);
			padding-block-end: 3.5rem;
		}

		.line,
		.flight {
			inset-inline: auto;
			inset-inline-start: 20px;
			top: 0;
			bottom: 0;
			width: 8px;
			height: auto;
		}

		/* Centred on the vertical line, delivered at its foot. */
		.letter {
			inset-inline: -18px auto;
			top: auto;
			bottom: 0;
		}

		.stops {
			grid-template-columns: 1fr;
			gap: 2rem;
		}

		.node {
			position: absolute;
			inset-inline-start: -3.5rem;
			margin: 0;
		}

		/* The waves turn to run down the line instead of across the stop name. */
		.cancel {
			inset-inline-start: calc(-3.5rem - 7px);
			rotate: 90deg;
		}

		.cancel:dir(rtl) {
			rotate: -90deg;
		}

		@media (prefers-reduced-motion: no-preference) {
			:global(.js) .letter {
				inset-inline: -18px auto;
				top: 0;
				bottom: auto;
			}

			:global(.js) .flying .letter,
			:global(.js) .flying:dir(rtl) .letter {
				translate: 0 calc(100cqb - 30px);
			}
		}
	}
</style>
