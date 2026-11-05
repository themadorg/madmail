<script>
	/** @type {{ href: string, variant?: 'primary' | 'default', children: import('svelte').Snippet }} */
	let { href, variant = 'default', children } = $props();
</script>

<a class="btn" class:primary={variant === 'primary'} {href}>
	{@render children()}
</a>

<style>
	.btn {
		display: inline-block;
		padding: 0.65rem 1.35rem;
		border: 1px solid var(--color-btn-border);
		border-radius: 0;
		font-size: 0.95rem;
		font-weight: 500;
		color: var(--color-btn-fg);
		text-decoration: none;
		transition: border-color 0.15s, background 0.15s, color 0.15s;
	}

	.btn:hover {
		border-color: var(--color-hover-border);
		background: var(--color-hover);
	}

	.btn.primary {
		border-color: var(--color-btn-primary-bg);
		background: var(--color-btn-primary-bg);
		color: var(--color-btn-primary-fg);
	}

	.btn.primary:hover {
		background: var(--color-btn-primary-hover-bg);
		border-color: var(--color-btn-primary-hover-bg);
	}
</style>
