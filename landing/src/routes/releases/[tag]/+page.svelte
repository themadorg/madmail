<script>
	import ReleasesPage from '$lib/components/ReleasesPage.svelte';
	let { data } = $props();
</script>

{#key data.release.tag_name}
	<ReleasesPage selectedRelease={data.release} />
{/key}
