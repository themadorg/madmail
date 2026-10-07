<script>
	import MadmailLogo from '$lib/components/MadmailLogo.svelte';
	import MadGlitchText from '$lib/components/MadGlitchText.svelte';
	let { spacious = false } = $props();
	import { resources } from '$lib/data.js';
	import { repo } from '$lib/nav.js';
	function clipReveal(node) {
		if (!spacious) return;
		let frame = 0;
		function update() {
			frame = 0;
			const content = node.previousElementSibling;
			if (!content) return;
			const footerBounds = node.getBoundingClientRect();
			const covered = Math.max(0, Math.min(footerBounds.height, content.getBoundingClientRect().bottom - footerBounds.top));
			node.style.clipPath = `inset(${covered}px 0 0)`;
		}
		function schedule() { if (!frame) frame = requestAnimationFrame(update); }
		const observer = new ResizeObserver(schedule);
		observer.observe(document.body);
		window.addEventListener('scroll', schedule, { passive: true });
		window.addEventListener('resize', schedule);
		update();
		return { destroy() {
			observer.disconnect();
			window.removeEventListener('scroll', schedule);
			window.removeEventListener('resize', schedule);
			cancelAnimationFrame(frame);
		} };
	}

</script>

<footer use:clipReveal class:spacious data-doc-footer={spacious || undefined}>
	{#if spacious}
		<div class="footer-logo">
			<MadmailLogo href="/" size="8rem" />
			<div class="footer-name"><MadGlitchText text="madmail" /></div>
		</div>
	{/if}
	<nav aria-label="Resources">
		{#each resources as link}
			<a href={link.href}>{link.label}</a>
		{/each}
	</nav>
	<p class="license">
		<a href="{repo}/blob/main/LICENCE">AGPL-3.0-or-later</a> · Use at your own risk. Validate for your
		threat model before production use.
	</p>
</footer>

<style>
	footer {
		max-width: 42rem;
		margin: 0 auto;
		padding: 3.5rem 1.5rem 4rem;
		text-align: center;
		color: var(--color-text);
	}

	footer.spacious { max-width: none; margin-top: 0; padding: 5rem 1.5rem 3rem; border-top: 1px solid var(--color-border); position: sticky; bottom: 0; z-index: 0; background: var(--docs-navigation-bg, var(--color-bg)); }
	@media (max-height: 600px), (prefers-reduced-motion: reduce) { footer.spacious { position: relative; } }
	.footer-logo { display: flex; flex-direction: column; align-items: center; gap: 0.75rem; margin-bottom: 2rem; }
	.footer-name { font-size: 2rem; font-weight: 700; line-height: 1.2; color: var(--color-text); }

	nav {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 0.35rem 1.25rem;
		margin-bottom: 1.25rem;
	}

	a {
		font-size: 0.9rem;
		color: var(--color-text-subtle);
		text-decoration: none;
	}

	a:hover {
		color: var(--color-text);
		text-decoration: underline;
	}

	.license {
		margin: 0;
		font-size: 0.8rem;
		line-height: 1.5;
		color: var(--color-text-faint);
	}
</style>
