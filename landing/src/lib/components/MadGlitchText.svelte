<script>
	import { runMatrixResolve } from '$lib/matrixScramble.js';

	/** @type {{ text: string, class?: string }} */
	let { text, class: className = '' } = $props();

	// Seeded from `text` so the letters are in the prerendered HTML, not only after hydration.
	// svelte-ignore state_referenced_locally
	let displayLetters = $state([...text]);
	let scrambling = $state(false);

	$effect(() => {
		displayLetters = [...text];
	});

	/** @type {(() => void) | undefined} */
	let stopScramble;

	function runMatrix() {
		if (scrambling) return;
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

		stopScramble?.();
		scrambling = true;

		stopScramble = runMatrixResolve(
			text,
			(letters) => {
				displayLetters = letters;
			},
			() => {
				scrambling = false;
				stopScramble = undefined;
			}
		);
	}
</script>

<button
	type="button"
	class="mad-glitch {className}"
	class:mad-glitch--scrambling={scrambling}
	aria-label={text}
	onclick={runMatrix}
>
	{#each displayLetters as letter, i}
		<span class="mad-letter" style="--mad-letter-i: {i}">
			<span class="mad-letter__ghost" aria-hidden="true">
				<span class="mad-letter__layer mad-letter__layer--r" data-letter={letter}></span>
				<span class="mad-letter__layer mad-letter__layer--g" data-letter={letter}></span>
			</span>
			<span class="mad-letter__main" aria-hidden="true">{letter}</span>
		</span>
	{/each}
</button>

<style>
	.mad-glitch {
		display: inline-flex;
		padding: 0;
		border: none;
		background: none;
		font-family: var(--font-mono);
		font-size: inherit;
		font-weight: inherit;
		line-height: inherit;
		letter-spacing: 0;
		color: inherit;
		text-transform: lowercase;
		cursor: pointer;
	}

	.mad-glitch--scrambling .mad-letter__main {
		color: var(--color-matrix);
	}

	.mad-letter {
		position: relative;
		display: inline-block;
		width: 1ch;
		min-width: 1ch;
		text-align: center;
	}

	.mad-letter__ghost {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.mad-letter__layer,
	.mad-letter__main {
		display: block;
		width: 100%;
		text-align: center;
	}

	/* Ghost copies come from CSS so crawlers read the word once, not three times. */
	.mad-letter__layer::before {
		content: attr(data-letter);
	}

	.mad-letter__layer {
		position: absolute;
		inset: 0;
		opacity: 0;
		pointer-events: none;
	}

	.mad-letter__layer--r {
		color: var(--color-glitch-r);
	}

	.mad-letter__layer--g {
		color: var(--color-glitch-g);
	}

	.mad-letter:hover .mad-letter__main {
		animation: mad-letter-jitter 0.42s steps(1);
	}

	.mad-glitch--scrambling .mad-letter:hover .mad-letter__main {
		animation: none;
	}

	.mad-letter:hover .mad-letter__layer--r {
		animation: mad-letter-rgb-r 0.42s steps(1);
	}

	.mad-letter:hover .mad-letter__layer--g {
		animation: mad-letter-rgb-g 0.42s steps(1);
	}

	.mad-glitch--scrambling .mad-letter:hover .mad-letter__layer--r,
	.mad-glitch--scrambling .mad-letter:hover .mad-letter__layer--g {
		animation: none;
	}

	@keyframes mad-letter-jitter {
		0%,
		100% {
			transform: translate(0);
		}

		12% {
			transform: translate(-2px, 1px) skewX(-4deg);
		}

		24% {
			transform: translate(2px, -1px) skewX(3deg);
		}

		36% {
			transform: translate(-1px, -2px);
		}

		48% {
			transform: translate(3px, 1px);
		}

		60% {
			transform: translate(-2px, 0);
		}

		72% {
			transform: translate(1px, 2px);
		}

		84% {
			transform: translate(-1px, -1px);
		}
	}

	@keyframes mad-letter-rgb-r {
		0%,
		100% {
			opacity: 0;
			transform: translate(0);
		}

		15% {
			opacity: 0.9;
			transform: translate(-3px, 0);
		}

		30% {
			opacity: 0.7;
			transform: translate(2px, 1px);
		}

		45% {
			opacity: 0.85;
			transform: translate(-2px, -1px);
		}

		60% {
			opacity: 0.5;
			transform: translate(3px, 0);
		}

		75% {
			opacity: 0;
		}
	}

	@keyframes mad-letter-rgb-g {
		0%,
		100% {
			opacity: 0;
			transform: translate(0);
		}

		18% {
			opacity: 0.85;
			transform: translate(3px, -1px);
		}

		33% {
			opacity: 0.65;
			transform: translate(-2px, 1px);
		}

		48% {
			opacity: 0.8;
			transform: translate(2px, 1px);
		}

		63% {
			opacity: 0.45;
			transform: translate(-3px, 0);
		}

		78% {
			opacity: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.mad-letter:hover .mad-letter__layer,
		.mad-letter:hover .mad-letter__main {
			animation: none;
		}
	}
</style>
