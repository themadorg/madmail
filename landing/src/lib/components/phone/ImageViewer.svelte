<script>
	import { tick, onDestroy } from 'svelte';
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	import closeIcon from '$lib/icons/x-mark.svg?raw';
	import leftIcon from '$lib/icons/chevron-left.svg?raw';
	import rightIcon from '$lib/icons/chevron-right.svg?raw';
	const { t, siteLanguage } = useSiteLanguage();
	let { images } = $props();
	let dialog;
	let picture = $state();
	let index = $state(0);
	let opened = $state(false);
	let closing = false;
	let trigger;
	let gallery;
	let previousOverflow;
	let locked = false;
	const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

	function restoreScroll() {
		if (locked) document.body.style.overflow = previousOverflow;
		locked = false;
	}

	export async function open(position, button) {
		if (opened) return;
		index = position;
		trigger = button;
		gallery = button.closest('ul');
		const from = button.querySelector('img').getBoundingClientRect();
		previousOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		locked = true;
		opened = true;
		await tick();
		dialog.showModal();
		if (!reducedMotion()) {
			const to = picture.getBoundingClientRect();
			picture.animate([
				{ transform: `translate(${from.x - to.x}px, ${from.y - to.y}px) scale(${from.width / to.width}, ${from.height / to.height})` },
				{ transform: 'none' }
			], { duration: 280, easing: 'cubic-bezier(0.2, 0, 0, 1)' });
		}
	}

	async function close() {
		if (!opened || closing) return;
		closing = true;
		if (!reducedMotion()) {
			const target = gallery?.querySelector(`[data-phone-screen="${index}"] img`);
			const to = target?.getBoundingClientRect();
			const from = picture.getBoundingClientRect();
			const visible = to && to.top >= 0 && to.bottom <= window.innerHeight && to.left >= 0 && to.right <= window.innerWidth;
			await picture.animate([
				{ transform: 'none', opacity: 1 },
				{ transform: visible ? `translate(${to.x - from.x}px, ${to.y - from.y}px) scale(${to.width / from.width}, ${to.height / from.height})` : 'scale(0.96)', opacity: visible ? 1 : 0 }
			], { duration: 180, easing: 'ease-in', fill: 'forwards' }).finished.catch(() => {});
		}
		dialog.close();
		opened = false;
		closing = false;
		restoreScroll();
		trigger?.focus({ preventScroll: true });
	}

	async function change(step) {
		if (closing) return;
		index = (index + step + images.length) % images.length;
		await tick();
		if (!reducedMotion()) picture.animate([{ opacity: 0.4 }, { opacity: 1 }], { duration: 140 });
	}

	function keydown(event) {
		if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
		event.preventDefault();
		change((event.key === 'ArrowRight' ? 1 : -1) * (siteLanguage.code === 'fa' ? -1 : 1));
	}

	onDestroy(restoreScroll);
</script>

<dialog bind:this={dialog} class="viewer" id="phone-image-viewer" aria-label={t('Screenshot viewer')} aria-describedby="phone-image-caption"
	oncancel={(event) => { event.preventDefault(); close(); }} onkeydown={keydown}
	onclick={(event) => { if (event.target === dialog || event.target.classList.contains('image-stage')) close(); }}>
	{#if opened}
		<button class="close" type="button" aria-label={t('Close')} onclick={close}>{@html closeIcon}</button>
		<button class="previous" type="button" aria-label={t('Previous image')} onclick={() => change(-1)}>{@html siteLanguage.code === 'fa' ? rightIcon : leftIcon}</button>
		<div class="image-stage">
			<img bind:this={picture} src={images[index].src} alt={t(images[index].caption)} width="720" height="1600" draggable="false" />
		</div>
		<button class="next" type="button" aria-label={t('Next image')} onclick={() => change(1)}>{@html siteLanguage.code === 'fa' ? leftIcon : rightIcon}</button>
		<footer>
			<p id="phone-image-caption" aria-live="polite">{t(images[index].caption)}</p>
			<span class="count"><bdi>{index + 1} / {images.length}</bdi></span>
		</footer>
	{/if}
</dialog>

<style>
	.viewer { position: fixed; inset: 0; width: 100%; height: 100dvh; max-width: none; max-height: none; box-sizing: border-box; margin: 0; padding: 3.5rem 4rem 1rem; border: 0; background: rgb(0 0 0 / 93%); color: #fff; }
	.viewer[open] { display: grid; grid-template-rows: minmax(0, 1fr) auto; gap: 1rem; animation: appear 180ms ease-out; }
	.viewer::backdrop { background: rgb(0 0 0 / 65%); }
	.image-stage { display: flex; align-items: center; justify-content: center; min-height: 0; min-width: 0; }
	img { display: block; width: auto; height: 100%; max-width: 100%; object-fit: contain; transform-origin: top left; border-radius: 0.75rem; }
	button { position: absolute; z-index: 1; display: grid; place-items: center; width: 2.5rem; height: 2.5rem; padding: 0; border: 1px solid rgb(255 255 255 / 25%); border-radius: 50%; color: #fff; background: #282828; cursor: pointer; }
	button:hover { background: #444; }
	button:focus-visible { outline: 2px solid #fff; outline-offset: 3px; }
	button :global(svg) { width: 1.25rem; height: 1.25rem; }
	.close { top: 0.75rem; inset-inline-end: 1rem; }
	.previous { inset-inline-start: 0.75rem; top: 50%; transform: translateY(-50%); }
	.next { inset-inline-end: 0.75rem; top: 50%; transform: translateY(-50%); }
	footer { text-align: center; max-width: 44rem; margin-inline: auto; }
	p { margin: 0; font-size: 0.95rem; line-height: 1.6; }
	.count { display: block; margin-top: 0.35rem; color: #bbb; font-size: 0.8rem; }
	@keyframes appear { from { opacity: 0; } to { opacity: 1; } }
	@media (max-width: 640px) { .viewer { padding: 3.5rem 3rem 1rem; } }
	@media (prefers-reduced-motion: reduce) { .viewer[open] { animation: none; } }
</style>
