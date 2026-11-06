# sv

Everything you need to build a Svelte project, powered by [`sv`](https://github.com/sveltejs/cli).

## Creating a project

If you're seeing this, you've probably already done this step. Congrats!

```sh
# create a new project
npx sv create my-app
```

To recreate this project with the same configuration:

```sh
# recreate this project
bun x sv@0.16.1 create --template minimal --no-types --add sveltekit-adapter="adapter:static" mdsvex --install bun landing
```

## Developing

Once you've created a project and installed dependencies with `npm install` (or `pnpm install` or `yarn`), start a development server:

```sh
npm run dev

# or start the server and open the app in a new browser tab
npm run dev -- --open
```

## Building

To create a production version of your app:

```sh
npm run build
```

You can preview the production build with `npm run preview`.

> To deploy your app, you may need to install an [adapter](https://svelte.dev/docs/kit/adapters) for your target environment.

## Agent discovery response headers

The production site is a static GitHub Pages deployment behind Cloudflare.
GitHub Pages does not support custom response headers, and SvelteKit server
hooks do not run on Pages. The Worker in `cloudflare/link-headers.js` adds an
[RFC 8288](https://www.rfc-editor.org/rfc/rfc8288) `Link` response header to
successful GET and HEAD requests for the English and Persian homepages.
It advertises the existing `/llms.txt` and `/llms-full.txt` summaries using
`describedby`, and `/docs.md` using `service-doc`. No API catalog is advertised
because this landing site does not publish one.

Deploy once from this directory with a Cloudflare account authorized to manage
Workers and routes for the existing proxied `madmail.chat` zone:

```sh
npx wrangler deploy --config cloudflare/wrangler.jsonc
```

Keep the current GitHub Pages DNS origin and Cloudflare proxy enabled. The Worker
forwards requests to that origin and preserves its body and headers. Deployments
of the landing pages continue through the existing GitHub Actions workflow;
changes to the Worker require running the command above again. Do not replace
an existing Worker route without incorporating its behavior into this Worker.

After deployment, verify the actual response headers (a local static preview
does not run the Worker):

```sh
curl -sSI https://madmail.chat/
curl -sS https://madmail.chat/ -D - -o /dev/null
curl -f https://madmail.chat/llms.txt
curl -f https://madmail.chat/llms-full.txt
curl -f https://madmail.chat/docs.md
```

Both homepage responses should include:

```http
Link: </llms.txt>; rel="describedby"; type="text/plain", </llms-full.txt>; rel="describedby"; type="text/plain", </docs.md>; rel="service-doc"; type="text/markdown"
```
