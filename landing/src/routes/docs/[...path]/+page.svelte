<script>
	import DocDirListing from '$lib/components/DocDirListing.svelte';
	import DocPage from '$lib/components/DocPage.svelte';

	let { data } = $props();
</script>

{#key data.href}
	<DocPage directory={data.kind === 'dir'} href={data.href} language={data.language ?? 'en'} languages={data.languages ?? []}>
		{#if data.kind === 'dir'}
			<DocDirListing title={data.title} entries={data.entries} />
		{:else if data.Content}
			<data.Content />
		{:else if data.html}
			{@html data.html}
		{/if}
	</DocPage>
{/key}
