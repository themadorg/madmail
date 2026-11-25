<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import { agentInstallCommand, agentQuickstart } from '$lib/agentInstall.js';

	const { t, localize } = useSiteLanguage();
</script>

<details class="agent-install">
	<summary>{t('Install for agents')}</summary>
	<div class="agent-install-panel">
		<p>{t('Set up a fresh Linux server for agent email in one command.')}</p>
		<p class="requirements">
			{t('Linux x86_64, systemd, glibc 2.38+, curl and sudo. Configure DNS and allow ACME on port 80 first. Replace the example domain and contact email before running. Do not use this on an existing installation.')}
		</p>
		<CodeBlock code={agentInstallCommand} wrap />
		<p class="requirements">
			{t('Installs Madmail, enables WebSMTP and WebIMAP server-wide, and starts the service. Account and encryption-key setup follow in the guide.')}
		</p>
		<a href={localize(`${agentQuickstart}#install-for-agents-on-linux`)}>{t('Full agent email quickstart')}</a>
	</div>
</details>

<style>
	.agent-install {
		width: min(44rem, 100%);
		margin: 1rem 0;
	}

	summary {
		display: inline-flex;
		min-height: 2.75rem;
		align-items: center;
		padding: 0.65rem 1.35rem;
		border: 1px solid var(--color-btn-border);
		border-radius: 999px;
		color: var(--color-btn-fg);
		font-size: 0.95rem;
		font-weight: 500;
		list-style: none;
		cursor: pointer;
	}

	summary::-webkit-details-marker { display: none; }

	summary:hover {
		border-color: var(--color-hover-border);
		background: var(--color-hover);
	}

	summary:focus-visible,
	a:focus-visible {
		outline: 2px solid currentColor;
		outline-offset: 4px;
	}

	.agent-install-panel {
		margin-top: 1rem;
		padding: 1.25rem;
		border: 1px solid var(--color-btn-border);
		border-radius: 0.75rem;
		background: var(--color-bg);
		color: var(--color-text);
		text-align: start;
		overflow-wrap: anywhere;
	}

	p { margin: 0 0 1rem; line-height: 1.6; }
	.requirements { font-size: 0.875rem; color: var(--color-text-subtle); }
	a { display: inline-block; margin-top: 1rem; color: var(--color-text); }
</style>
