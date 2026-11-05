<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { siteLanguage, t } = useSiteLanguage();
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import arrowRight from '$lib/icons/arrow-right.svg?raw';
	import arrowLeft from '$lib/icons/arrow-left.svg?raw';
	import { repo } from '$lib/nav.js';

	const installCode = `ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')

curl -fsSL "https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-\${ARCH}" \\
  -o madmail

chmod +x madmail

sudo ./madmail install --simple --domain mail.example.org \\
  --acme-email you@example.com --lang en

sudo systemctl enable --now madmail`;
</script>

<section class="install">
	<h2>{t("Quick Setup")}</h2>
	<p>{t("Download the release binary, then install and start the service:")}</p>
	<CodeBlock code={installCode} wrap class="install-code" />
	<p class="hint">
		{t("Also works with a public IP and no domain.")}
		<a href="/docs/deployment" class="hint-link">
			{t("See deployment options")}
			<span class="icon" aria-hidden="true">{@html siteLanguage.code === 'fa' ? arrowLeft : arrowRight}</span>
		</a>
	</p>
	<p class="hint">
		{t("On Windows, use")}
		<code>madmail-windows-amd64-setup.exe</code>
		{t("from")}
		<a href="{repo}/releases">{t("Releases")}</a>{t(", or the CLI in")}
		<a href="/docs/project/user-guide/02-quick-start#windows" class="hint-link">
			{t("Quick Start — Windows")}
			<span class="icon" aria-hidden="true">{@html siteLanguage.code === 'fa' ? arrowLeft : arrowRight}</span>
		</a>
	</p>
</section>

<style>
	.install {
		max-width: 42rem;
		margin: 0 auto;
		padding: 3.5rem 1.5rem 0;
	}

	h2 {
		margin: 0 0 1.25rem;
		font-size: 1.35rem;
		font-weight: 600;
		letter-spacing: -0.02em;
		color: var(--color-text);
	}

	p {
		margin: 0 0 1rem;
		line-height: 1.65;
		color: var(--color-text);
	}

	.install :global(.install-code) {
		margin: 0 0 1rem;
		font-size: 0.8rem;
		line-height: 1.55;
		font-family: var(--font-mono);
	}

	.hint {
		font-size: 0.9rem;
		color: var(--color-text-subtle);
	}

	.hint a:hover,
	.hint-link:hover {
		color: var(--color-text);
	}

	.hint-link {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		color: inherit;
		text-decoration: none;
	}

	.hint-link:hover {
		text-decoration: underline;
	}

	.hint code {
		font-family: var(--font-mono);
		font-size: 0.85em;
	}

	.icon :global(svg) {
		display: block;
		width: 0.9rem;
		height: 0.9rem;
	}
</style>
