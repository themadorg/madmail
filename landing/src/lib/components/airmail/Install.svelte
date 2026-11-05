<script>
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import { repo } from '$lib/nav.js';

	const i18n = useI18n();

	const download = `ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
curl -fsSL "https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-\${ARCH}" -o madmail
chmod +x madmail`;

	/* Each slip declares one real way to deploy. The installer speaks the page's language (--lang en|fa|ru|es). */
	const slips = $derived([
		{
			id: 'domain',
			docs: '/docs/quick-setup',
			command: `${download}
sudo ./madmail install --simple --domain mail.example.org --acme-email you@example.com --lang ${i18n.lang}
sudo systemctl enable --now madmail`
		},
		{
			id: 'ip',
			docs: '/docs/project/user-guide/11-deployment-ip-domain-certs',
			command: `${download}
sudo ./madmail install --simple --ip YOUR_IP --auto-ip-cert --acme-email you@example.com --lang ${i18n.lang}
sudo systemctl enable --now madmail`
		}
	]);

	/** @type {string | null} */
	let copied = $state(null);
	/** @type {ReturnType<typeof setTimeout> | undefined} */
	let timer;

	/** @param {string} id @param {string} text */
	async function copy(id, text) {
		try {
			await navigator.clipboard.writeText(text);
			copied = id;
			clearTimeout(timer);
			timer = setTimeout(() => (copied = null), 1600);
		} catch {
			/* Clipboard blocked: the command stays selectable. */
		}
	}
</script>

<section class="install" id="install" aria-labelledby="install-title">
	<div class="wrap layout">
		<h2 id="install-title">{i18n.t('install_title')}</h2>
		<p class="lede">{i18n.t('install_lede')}</p>

		{#each slips as slip (slip.id)}
			<article class="slip sheet" aria-labelledby="install-{slip.id}">
				<div class="row form-head">
					<p class="form-title">{i18n.t('install_form_title')}</p>
					<p class="form-code" dir="ltr" lang="en">CN 22</p>
				</div>

				<div class="row from">
					<p class="address-label">{i18n.t('env_from')}</p>
					<div class="sender">
						<p class="sender-name" lang="en">Madmail</p>
						<h3 class="what" id="install-{slip.id}">{i18n.t(`slip_${slip.id}`)}</h3>
					</div>
				</div>

				<div class="row facts">
					<div class="box">
						<p class="caption">{i18n.t('install_license')}</p>
						<p class="value"><bdi>AGPL-3.0-or-later</bdi></p>
					</div>
					<div class="box">
						<p class="caption">{i18n.t('install_platform')}</p>
						<p class="value value-wrap"><bdi>Linux amd64 · arm64</bdi></p>
					</div>
					<div class="box docs">
						<a class="textlink" href={slip.docs}>{i18n.t('install_docs')}</a>
					</div>
				</div>

				<div class="row contents">
					<div class="contents-head">
						<p class="caption">{i18n.t('install_contents')}</p>
						<button type="button" class="copy" class:done={copied === slip.id} onclick={() => copy(slip.id, slip.command)}>
							<svg class="icon" viewBox="0 0 20 20" width="18" height="18" aria-hidden="true">
								<g class="i-copy" fill="none" stroke="currentColor" stroke-width="1.75">
									<path d="M7 7h9v9H7z" />
									<path d="M13 4H4v9" />
								</g>
								<path class="i-done" d="M4 10.5l4 4 8-9" fill="none" stroke="currentColor" stroke-width="2.25" />
							</svg>
							<span aria-live="polite">{copied === slip.id ? i18n.t('install_copied') : i18n.t('install_copy')}</span>
						</button>
					</div>
					<pre dir="ltr"><code>{#each slip.command.split('\n') as line, n (n)}<span class="ln">{line}</span>{/each}</code></pre>
				</div>

				<div class="row note">
					<p class="caption">{i18n.t('install_note_label')}</p>
					<p class="note-text">{i18n.t(`slip_${slip.id}_note`)}</p>
				</div>
			</article>
		{/each}

		<p class="other">
			{i18n.t('install_other')}
			<a class="textlink" href="/releases">{i18n.t('install_all_releases')}</a>
		</p>
	</div>
</section>

<style>
	.install {
		padding-block: var(--section-gap) 0;
	}

	/* Header and slips share one grid: the heading runs the full width, the
	   lede drops under it into the second slip's column, so the type and the
	   forms line up on the same edge instead of a separate header split. */
	.layout {
		display: grid;
		grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
		/* Five shared rows per slip (head, from, facts, contents, note) via subgrid,
		   so the two forms rule their boxes on the same lines. */
		grid-template-rows: auto auto repeat(5, auto) auto;
		column-gap: clamp(1.5rem, 3vw, 2.5rem);
		align-items: start;
	}

	h2 {
		grid-column: 1 / -1;
		font-size: var(--step-h2);
	}

	.lede {
		grid-column: 2;
		max-width: 34rem;
		margin-block: 0.9rem clamp(2.5rem, 4vw, 3.5rem);
		font-size: var(--step-lead);
	}

	/* CN 22: a ruled customs form. Paper in both themes, ink rules, square. */
	.slip {
		display: grid;
		grid-row: span 5;
		grid-template-rows: subgrid;
		align-self: stretch;
		min-width: 0;
		border: 2px solid var(--ink);
		/* The paper's own margin around the printed frame. Invisible on the
		   light desk (same white), it keeps the frame readable on the dark one.
		   Outline, not padding: the frame stays on the heading's edge. */
		outline: 0.375rem solid var(--paper);
	}

	.row {
		min-width: 0;
		border-block-start: 1px solid var(--ink);
	}

	.row.form-head {
		border-block-start: 0;
	}

	.caption {
		color: var(--ink-2);
		font-size: var(--step-small);
		font-weight: 600;
		line-height: 1.3;
	}

	.form-head {
		display: flex;
		align-items: stretch;
		justify-content: space-between;
	}

	.form-title {
		padding: 0.7rem 1.1rem 0.6rem;
		font-family: var(--font-display);
		font-size: var(--step-lead);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
		align-self: center;
	}

	/* The rule sits on the title (page direction), not on the forced-LTR
	   form code, so it lands between them in RTL too. */
	.form-title {
		flex: 1;
		border-inline-end: 1px solid var(--ink);
	}

	.form-code {
		display: flex;
		align-items: center;
		padding: 0.6rem 1rem;
		font-family: var(--font-display);
		font-size: var(--step-lead);
		font-weight: 800;
		line-height: 1;
		white-space: nowrap;
	}

	/* The sender line uses the envelope's address label, set beside its
	   field rather than above it. */
	.from {
		display: flex;
		align-items: baseline;
		gap: 0.5rem 1.25rem;
		padding: clamp(1.1rem, 2vw, 1.5rem) 1.1rem;
	}

	.address-label {
		flex: none;
		color: var(--red);
		font-family: var(--font-display);
		font-size: var(--step-lead);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
	}

	.sender {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.35rem 1rem;
		min-width: 0;
	}

	.sender-name {
		font-family: var(--font-display);
		font-size: var(--step-h2);
		font-weight: 800;
		line-height: 0.92;
		text-transform: uppercase;
	}

	.what {
		color: var(--red);
		font-family: var(--font-text);
		font-size: var(--step-body);
		font-weight: 700;
		line-height: 1.3;
		letter-spacing: 0;
		text-transform: none;
	}

	.other {
		grid-column: 1 / -1;
		max-width: 52rem;
		margin-block-start: clamp(2rem, 4vw, 3rem);
		font-size: var(--step-small);
		color: var(--ink-2);
	}

	.other .textlink {
		margin-inline-start: 0.4rem;
		white-space: nowrap;
	}

	/* Licence and language boxes never shrink below their token, so a long
	   SPDX id (AGPL-3.0-or-later) takes room from the docs box, not the rule. */
	.facts {
		display: grid;
		grid-template-columns: minmax(max-content, 1fr) minmax(max-content, 1fr) minmax(0, 1.4fr);
	}

	.box {
		display: grid;
		align-content: start;
		gap: 0.2rem;
		padding: 0.75rem 1.1rem 0.85rem;
	}

	.box + .box {
		border-inline-start: 1px solid var(--ink);
	}

	/* Licence and language are tokens: AGPL-3.0 never breaks at its hyphen. */
	.value {
		font-size: var(--step-body);
		font-weight: 700;
		line-height: 1.3;
		white-space: nowrap;
	}

	.docs {
		align-content: center;
		font-size: var(--step-small);
	}

	.contents {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		padding: 0.75rem 1.1rem 1.1rem;
	}

	.contents-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}

	/* Typed into the contents field: ink on the form's tinted write-in area. */
	pre {
		margin: 0;
		padding: 1rem 1.1rem;
		background: var(--paper-2);
		color: var(--ink);
		font-size: var(--step-small);
		line-height: 1.7;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		text-align: left;
	}

	/* One block per real command line, with a hanging indent: a soft-wrapped
	   continuation sits indented, so it never reads as a separate command. */
	.ln {
		display: block;
		padding-inline-start: 2ch;
		text-indent: -2ch;
	}

	.copy {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.45rem;
		min-width: 6.5rem;
		min-height: 2.5rem;
		padding: 0.45rem 0.9rem;
		border: 0;
		background: var(--ink);
		color: var(--paper);
		font-size: var(--step-small);
		font-weight: 700;
		cursor: pointer;
		transition:
			transform var(--dur-press) var(--ease-out),
			background-color var(--dur-ui) var(--ease-out);
	}

	.copy:active {
		transform: scale(0.96);
	}

	.copy.done {
		background: var(--red-field);
		color: var(--red-field-ink);
	}

	.icon {
		flex: none;
	}

	.icon :is(.i-copy, .i-done) {
		transform-origin: 50% 50%;
		transition:
			opacity var(--dur-ui) var(--ease-out),
			transform var(--dur-ui) var(--ease-out);
	}

	.i-done {
		opacity: 0;
		transform: scale(0.6);
	}

	.copy.done .i-copy {
		opacity: 0;
		transform: scale(0.6);
	}

	.copy.done .i-done {
		opacity: 1;
		transform: scale(1);
	}

	@media (hover: hover) and (pointer: fine) {
		.copy:not(.done):hover {
			background: color-mix(in oklab, var(--ink) 82%, var(--paper));
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.copy,
		.icon :is(.i-copy, .i-done) {
			transition: none;
		}

		.copy:active {
			transform: none;
		}
	}

	.note {
		display: grid;
		align-content: start;
		gap: 0.2rem;
		padding: 0.75rem 1.1rem 1rem;
	}

	.note-text {
		max-width: 36rem;
		font-size: var(--step-small);
	}

	/* Small laptops: the narrower slip would crush its fact boxes, so both
	   forms take even halves until there is room for the 7:5 split. */
	@media (max-width: 1199px) and (min-width: 961px) {
		.layout {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		}
	}

	@media (max-width: 960px) {
		.layout {
			grid-template-columns: minmax(0, 1fr);
			grid-template-rows: none;
			row-gap: 0;
		}

		.lede {
			grid-column: 1;
		}

		.other {
			grid-column: 1;
		}

		.slip + .slip {
			margin-block-start: clamp(1.5rem, 4vw, 2.5rem);
		}
	}

	@media (max-width: 480px) {
		.facts {
			grid-template-columns: max-content minmax(0, 1fr);
		}

		.value-wrap {
			white-space: normal;
		}

		/* Give the command every column the phone has: the write-in field runs
		   rule to rule and its text sits on the form's text edge, so whole
		   tokens stay on a line instead of orphaning a closing paren. */
		.contents {
			padding-block-end: 0;
		}

		pre {
			margin-inline: -1.1rem;
			padding: 0.85rem 1.1rem;
			font-size: 0.8125rem;
		}

		.box.docs {
			grid-column: 1 / -1;
			border-block-start: 1px solid var(--ink);
			border-inline-start: 0;
			padding-block: 0.9rem;
		}
	}
</style>
