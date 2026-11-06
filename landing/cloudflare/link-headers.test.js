import assert from 'node:assert/strict';
import { test } from 'node:test';
import { addDiscoveryHeaders, discoveryLinks } from './link-headers.js';

for (const path of ['/', '/index.html', '/fa', '/fa/']) {
	for (const method of ['GET', 'HEAD']) {
		test(`${method} ${path} advertises resources and preserves origin headers`, async () => {
			const request = new Request(`https://madmail.chat${path}?lang=en`, { method });
			const response = await addDiscoveryHeaders(request, async (forwarded) => {
				assert.equal(forwarded, request);
				return new Response(method === 'HEAD' ? null : '<html>Madmail</html>', {
					headers: { Link: '</existing>; rel="preload"', 'Cache-Control': 'max-age=600' }
				});
			});
			assert.equal(response.headers.get('Link'), `</existing>; rel="preload", ${discoveryLinks}`);
			assert.equal(response.headers.get('Cache-Control'), 'max-age=600');
			assert.equal(await response.text(), method === 'HEAD' ? '' : '<html>Madmail</html>');
		});
	}
}

for (const [path, method, status] of [
	['/docs', 'GET', 200], ['/', 'POST', 200], ['/', 'GET', 301], ['/', 'GET', 404], ['/', 'GET', 500]
]) {
	test(`${method} ${path} (${status}) passes through unchanged`, async () => {
		const origin = new Response('origin', { status });
		const response = await addDiscoveryHeaders(new Request(`https://madmail.chat${path}`, { method }), async () => origin);
		assert.equal(response, origin);
	});
}
