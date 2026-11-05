<script>
	/**
	 * A circular cancellation with wavy lines. Always Latin and LTR (decorative):
	 * RTL text on a circular path does not shape reliably. In RTL the ring moves
	 * to the other end of the waves; the SVG itself is never flipped.
	 * @type {{ top: string, date: string, mirrored?: boolean, class?: string }}
	 */
	let { top, date, mirrored = false, class: className = '' } = $props();

	const cx = $derived(mirrored ? 250 : 70);
	const waves = $derived(mirrored ? 'translate(-136 0)' : undefined);
	const uid = $props.id();
</script>

<svg class={['postmark', className]} viewBox="0 0 320 140" aria-hidden="true" lang="en">
	<defs>
		<path id="ring-{uid}" d="M {cx} 70 m -50 0 a 50 50 0 1 1 100 0 a 50 50 0 1 1 -100 0" />
	</defs>
	<g fill="none" stroke="currentColor" stroke-width="3">
		<circle {cx} cy="70" r="62" />
		<circle {cx} cy="70" r="38" stroke-width="2" />
		<path
			transform={waves}
			d="M140 44 q 22 -10 44 0 t 44 0 t 44 0 t 44 0 M140 70 q 22 -10 44 0 t 44 0 t 44 0 t 44 0 M140 96 q 22 -10 44 0 t 44 0 t 44 0 t 44 0"
		/>
	</g>
	<text class="ring-text" fill="currentColor">
		<textPath href="#ring-{uid}" startOffset="0">{top}</textPath>
	</text>
	<text class="date" x={cx} y="76" text-anchor="middle" fill="currentColor">{date}</text>
</svg>

<style>
	.postmark {
		direction: ltr;
		display: block;
		width: 100%;
		height: auto;
		overflow: visible;
	}

	.ring-text {
		font-family: var(--font-display);
		font-size: 15px;
		font-weight: 800;
		letter-spacing: 0.16em;
		text-transform: uppercase;
	}

	.date {
		font-family: var(--font-display);
		font-size: 17px;
		font-weight: 800;
		text-transform: uppercase;
	}
</style>
