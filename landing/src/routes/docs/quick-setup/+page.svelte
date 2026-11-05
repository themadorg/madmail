<script>
	import DocPage from '$lib/components/DocPage.svelte';

	let { data } = $props();
</script>

{#key data.href}
	<DocPage href={data.href} language={data.language} languages={data.languages}>
		{#if data.Content}
			<data.Content />
		{:else if data.html}
			{@html data.html}
		{/if}
	</DocPage>
{/key}
