<script>
	import { goto } from '$app/navigation';
	/** @type {{ id: string, label: string, menuLabel: string, icon: string, items: { label: string, href: string, current: boolean, lang?: string, icon?: string }[], align?: 'left' | 'right' }} */
	let { id, label, menuLabel, icon, items, align = 'left' } = $props();
	let menu;
	async function select(event, item) {
		if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
		event.preventDefault();
		menu?.hidePopover();
		if (item.current) return;
		await Promise.allSettled(menu.getAnimations().map((animation) => animation.finished));
		if (menu.matches(':popover-open')) return;
		await goto(item.href);
	}
</script>

<button type="button" class="section-toggle" class:language-toggle={id === 'post-languages'} popovertarget={id} aria-label={label} title={label}>
	<span class="icon" aria-hidden="true">{@html icon}</span>
</button>
<nav bind:this={menu} {id} class="section-navigation" class:align-right={align === 'right'} popover aria-label={menuLabel}>
	{#each items as item}
		<a href={item.href} lang={item.lang} aria-current={item.current ? 'true' : undefined} onclick={(event) => select(event, item)}>
			{#if item.icon}<span class="item-icon" aria-hidden="true">{@html item.icon}</span>{/if}
			{item.label}
		</a>
	{/each}
</nav>

<style>
	.section-toggle {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 1.9rem;
		padding: 0;
		border-radius: 0.5rem;
		background: transparent;
		color: var(--color-text-subtle);
		cursor: pointer;
	}
	.section-toggle:hover { background: var(--color-hover); color: var(--color-text); }
	.icon :global(svg) { display: block; }
	.section-toggle {
		width: 1.5rem;
	}
	.section-toggle,
	.section-toggle:focus,
	.section-toggle:focus-visible,
	.section-toggle:active {
		border: none;
		outline: none;
		box-shadow: none;
	}
	.section-toggle .icon :global(svg) {
		width: 1rem;
		height: 1rem;
	}
	.language-toggle {
		width: 1.9rem;
		height: 1.9rem;
		flex-shrink: 0;
	}
	.language-toggle .icon :global(svg) {
		width: 1.25rem;
		height: 1.25rem;
	}
	.section-navigation {
		opacity: 0;
		transform: translateY(-6px);
		transition: opacity 180ms ease, transform 180ms ease,
			display 180ms allow-discrete, overlay 180ms allow-discrete;
		position: fixed;
		inset: 3.25rem auto auto 2.75rem;
		margin: 0;
		min-width: 10rem;
		padding: 0.35rem;
		border: 1px solid var(--color-border);
		border-radius: 0.6rem;
		background: var(--color-surface);
		color: var(--color-text);
		box-shadow: 0 8px 24px rgb(0 0 0 / 15%);
	}
	.section-navigation a {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.6rem 0.75rem;
		border-radius: 0.35rem;
		color: inherit;
		font-size: 0.85rem;
		text-decoration: none;
		transition: background-color 180ms ease, color 180ms ease;
	}
	.align-right {
		inset: 3.25rem 1rem auto auto;
		text-align: center;
	}
	.align-right a { justify-content: center; }
	.item-icon :global(svg) { display: block; width: 1rem; height: 1rem; }
	.section-navigation a + a {
		margin-top: 0.25rem;
	}
	.section-navigation:popover-open {
		opacity: 1;
		transform: translateY(0);
	}
	@starting-style {
		.section-navigation:popover-open {
			opacity: 0;
			transform: translateY(-6px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.section-navigation, .section-navigation a { transition: none; }
	}
	.section-navigation a:hover,
	.section-navigation a[aria-current='true'] {
		background: var(--color-hover);
	}
</style>
