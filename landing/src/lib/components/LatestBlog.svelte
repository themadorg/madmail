<script>
	import { posts } from '$lib/blog.js';
	import arrowRight from '$lib/icons/arrow-right.svg?raw';
	const latest = posts.find((post) => post.language === 'en') ?? posts[0] ?? null;
</script>

<section class="blog" id="blog" aria-labelledby="blog-heading">
	<div class="heading">
		<h2 id="blog-heading">Blog</h2>
		<a href="/blog">All posts <span class="icon" aria-hidden="true">{@html arrowRight}</span></a>
	</div>
	{#if latest}
		<a class="post-card" href="/blog/{latest.slug}" aria-labelledby="latest-post-title">
			<h3 id="latest-post-title">{latest.title}</h3>
			{#if latest.description}<p>{latest.description}</p>{/if}
		</a>
	{:else}
		<p>No posts yet.</p>
	{/if}
</section>

<style>
	.blog {
		max-width: 42rem;
		margin: 0 auto;
		padding: 3.5rem 1.5rem 0;
	}
	.heading {
		border-top: 1px solid var(--color-border);
		padding-top: 1.5rem;
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		margin-bottom: 1.25rem;
	}
	h2 {
		margin: 0;
		font-size: 1.35rem;
		font-weight: 600;
		letter-spacing: -0.02em;
	}
	h3 {
		margin: 0 0 0.5rem;
		font-size: 1rem;
		font-weight: 600;
	}
	.post-card {
		display: block;
		padding: 1rem;
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		background: var(--color-surface);
		transition: background-color 250ms ease, border-color 250ms ease;
	}
	.post-card:hover {
		border-color: var(--color-border-strong);
		background: var(--color-surface-raised);
		text-decoration: none;
	}
	.post-card:focus-visible {
		outline: 2px solid var(--color-text);
		outline-offset: 3px;
	}
	@media (prefers-reduced-motion: reduce) {
		.post-card {
			transition: none;
		}
	}
	a {
		color: inherit;
		text-decoration: none;
	}
	a:hover {
		text-decoration: underline;
	}
	.heading > a {
		font-size: 0.85rem;
		color: var(--color-text-subtle);
	}
	.heading > a {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
	}
	.icon :global(svg) {
		display: block;
		width: 0.9rem;
		height: 0.9rem;
	}
	p {
		margin: 0;
		font-size: 0.875rem;
		line-height: 1.65;
		color: var(--color-text-subtle);
	}
</style>
