<script>
	import About from '$lib/components/About.svelte';
	import DeltachatModal from '$lib/components/DeltachatModal.svelte';
	import Features from '$lib/components/Features.svelte';
	import Hero from '$lib/components/Hero.svelte';
	import Install from '$lib/components/Install.svelte';
	import LatestBlog from '$lib/components/LatestBlog.svelte';
	import SiteFooter from '$lib/components/SiteFooter.svelte';

	let deltachatOpen = $state(false);
</script>

<main>
	<Hero onDeltachat={() => (deltachatOpen = true)} />
	<About />
	<Features />
	<Install />
	<LatestBlog />
	<SiteFooter />
</main>

<DeltachatModal bind:open={deltachatOpen} />

<style>
	main {
		background: var(--color-bg);
		color: var(--color-text);
		transition: var(--transition-theme);
	}
</style>
