<script>
	import BlogPage from '$lib/components/BlogPage.svelte';
	import { posts } from '$lib/blog.js';
</script>

<BlogPage {posts} />
