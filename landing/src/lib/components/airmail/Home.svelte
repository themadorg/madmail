<script>
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import Envelope from './Envelope.svelte';
	import Faq from './Faq.svelte';
	import Install from './Install.svelte';
	import Letter from './Letter.svelte';
	import Route from './Route.svelte';
	import SiteNav from './SiteNav.svelte';
	import SortingOffice from './SortingOffice.svelte';
	import StampSet from './StampSet.svelte';
</script>

<!--
	Madmail's own mail, in the order a letter travels: the envelope (what Madmail is and
	who it is for), the letter inside (what chatmail is), the sorting office (what the
	relay does), the route (how a message moves), the customs slips (install), the rest
	of the stamp set (app, panel, notes, releases), enquiries, and the back of the envelope.
-->
<div class="home airmail">
	<SiteNav />
	<main id="main" tabindex="-1">
		<Envelope />
		<Letter />
		<SortingOffice />
		<Route />
		<Install />
		<StampSet />
		<Faq />
	</main>
	<SiteFooter />
</div>

<style>
	.home {
		background: var(--paper);
		color: var(--ink);
	}

	main:focus {
		outline: none;
	}
</style>
