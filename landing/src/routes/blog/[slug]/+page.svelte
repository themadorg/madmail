<script>
	import BlogPage from '$lib/components/BlogPage.svelte';

	let { data } = $props();
</script>

<BlogPage posts={[data.post]} selected={data.post} />
