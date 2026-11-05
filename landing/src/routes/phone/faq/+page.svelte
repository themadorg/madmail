<script>
	import PhoneShell from '$lib/components/phone/PhoneShell.svelte';
	import { slugifyHeading } from '$lib/slugify.js';

	let { data } = $props();
</script>

<PhoneShell current="/phone/faq">
	<article class="doc">
		<h1>Questions</h1>
		<p class="lede">
			What people ask before putting a mail server on a phone. Still curious? Ask in
			<a href="https://t.me/the_madmail">Telegram</a> or
			<a href="https://github.com/themadorg/madmail-phone/issues">open an issue</a>.
		</p>

		<div class="list">
			{#each data.faq as item}
				<details id={slugifyHeading(item.question)}>
					<summary>{item.question}</summary>
					<p>{item.answer}</p>
				</details>
			{/each}
		</div>

		<p class="next">
			Ready to try it? <a href="/phone/install">Install Madmail Mobile</a>.
		</p>
	</article>
</PhoneShell>

<style>
	.list {
		margin-top: 2.5rem;
		border-top: 1px solid var(--color-border);
	}

	details {
		border-bottom: 1px solid var(--color-border);
	}

	summary {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 1.2rem 0;
		font-size: 1.05rem;
		font-weight: 600;
		color: var(--color-text);
		cursor: pointer;
		list-style: none;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	summary::after {
		content: '';
		flex: none;
		width: 0.6rem;
		height: 0.6rem;
		margin-right: 0.3rem;
		border-right: 2px solid var(--color-text-subtle);
		border-bottom: 2px solid var(--color-text-subtle);
		transform: rotate(45deg) translateY(-25%);
		transition: transform 0.2s cubic-bezier(0.2, 0, 0, 1);
	}

	details[open] summary::after {
		transform: rotate(225deg) translateY(-25%);
		border-color: var(--pm-red);
	}

	details p {
		margin: 0 0 1.4rem;
		max-width: 40rem;
	}

	.next {
		margin-top: 2.5rem;
	}

	@media (prefers-reduced-motion: reduce) {
		summary::after {
			transition: none;
		}
	}
</style>
