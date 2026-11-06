// GitHub Pages cannot set custom response headers. Run this Worker on the
// existing proxied hostname; fetch(request) continues to the Pages origin.
export const discoveryLinks = [
	'</llms.txt>; rel="describedby"; type="text/plain"',
	'</llms-full.txt>; rel="describedby"; type="text/plain"',
	'</docs.md>; rel="service-doc"; type="text/markdown"'
].join(', ');

export async function addDiscoveryHeaders(request, fetchOrigin = fetch) {
	const response = await fetchOrigin(request);
	const path = new URL(request.url).pathname;
	if (!['/', '/index.html', '/fa', '/fa/'].includes(path) ||
		!['GET', 'HEAD'].includes(request.method) ||
		response.status !== 200) return response;

	const headers = new Headers(response.headers);
	headers.append('Link', discoveryLinks);
	return new Response(response.body, {
		status: response.status,
		statusText: response.statusText,
		headers
	});
}

export default { fetch: (request) => addDiscoveryHeaders(request) };
