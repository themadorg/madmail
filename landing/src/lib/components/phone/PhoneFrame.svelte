<script>
	/** @type {{ src: string, alt: string, eager?: boolean, class?: string }} */
	let { src, alt, eager = false, class: className = '' } = $props();
</script>

<figure class="device {className}">
	<img
		{src}
		{alt}
		width="720"
		height="1600"
		loading={eager ? 'eager' : 'lazy'}
		fetchpriority={eager ? 'high' : undefined}
		decoding="async"
	/>
</figure>

<style>
	.device {
		position: relative;
		margin: 0;
		width: var(--device-width, 16rem);
		aspect-ratio: 9 / 20;
		padding: calc(var(--device-width, 16rem) * 0.035);
		border-radius: calc(var(--device-width, 16rem) * 0.15);
		background: #121212;
		box-shadow:
			inset 0 0 0 1.5px #3a3a3a,
			inset 0 0 0 4px #121212,
			inset 0 0 0 5px #262626,
			0 1.5rem 3rem -1rem rgb(0 0 0 / 0.55);
	}

	img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
		border-radius: calc(var(--device-width, 16rem) * 0.12);
		background: #1e1b1a;
	}
</style>
