<script>
	import { cubicOut } from 'svelte/easing';
	import { useI18n } from '$lib/i18n/use.svelte.js';
	import logo from '$lib/logoUrl.js';
	import { activateMadMode } from '$lib/madMode.svelte.js';
	import { repo } from '$lib/nav.js';
	import { releases } from '$lib/releases.js';
	import Postmark from './Postmark.svelte';

	const i18n = useI18n();
	const latest = releases[0] ?? null;

	/** Who the envelope is addressed to. Each role re-addresses it in place. */
	const roles = [
		{ id: 'operators', action: '#install', alt: '/docs/quick-setup' },
		{ id: 'users', action: 'https://get.delta.chat', alt: '/docs/project/user-guide/01-what-is-chatmail' },
		{ id: 'phone', action: '/phone', alt: '/phone/install' },
		{ id: 'devs', action: repo, alt: '/docs' }
	];

	let active = $state('operators');
	const current = $derived(roles.find((r) => r.id === active) ?? roles[0]);

	/* The cancellation is struck on the day the latest release went out. */
	const date = latest?.published_at
		? new Intl.DateTimeFormat('en', { month: 'short', year: 'numeric', timeZone: 'UTC' }).format(new Date(latest.published_at))
		: 'Madmail';
	const ringText = 'Madmail · chatmail relay · ';

	/** @type {HTMLButtonElement[]} */
	const tabs = [];

	/** @param {KeyboardEvent} e @param {number} index */
	function onTabKey(e, index) {
		const rtl = i18n.dir === 'rtl';
		const next = rtl ? 'ArrowLeft' : 'ArrowRight';
		const prev = rtl ? 'ArrowRight' : 'ArrowLeft';
		let target = -1;
		if (e.key === next) target = (index + 1) % roles.length;
		else if (e.key === prev) target = (index - 1 + roles.length) % roles.length;
		else if (e.key === 'Home') target = 0;
		else if (e.key === 'End') target = roles.length - 1;
		if (target < 0) return;
		e.preventDefault();
		active = roles[target].id;
		tabs[target]?.focus();
	}

	/** Re-addressing: the new line settles in from a slight blur, never from nothing. */
	function readdress() {
		const reduce = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
		return {
			duration: reduce ? 0 : 260,
			easing: cubicOut,
			/** @param {number} t */
			css: (t) => `opacity:${t};transform:translateY(${(1 - t) * 6}px);filter:blur(${(1 - t) * 3}px)`
		};
	}

	/** @param {string} href */
	const external = (href) => (href.startsWith('http') ? { target: '_blank', rel: 'noopener noreferrer' } : {});
</script>

<section class="envelope" id="top" aria-labelledby="hero-title">
	<div class="frame sheet">
		<div class="face">
			<p class="from">
				<span class="label">{i18n.t('env_from')}</span>
				<strong>Madmail</strong>
				<span class="from-line">{i18n.t('env_sender_line')}</span>
			</p>

			<div class="stamp-corner">
				<button type="button" class="stamp" onclick={activateMadMode} aria-label={i18n.t('env_stamp_alt')}>
					<span class="stamp-picture">
						<img src={logo} alt="" width="320" height="320" fetchpriority="high" />
					</span>
					<span class="stamp-foot" lang="en" dir="ltr">
						<span>Madmail</span>
						{#if latest}<span>{latest.tag_name}</span>{/if}
					</span>
				</button>
				<Postmark class="postmark" top={ringText} {date} mirrored={i18n.dir === 'rtl'} />
			</div>

			<h1 id="hero-title">{i18n.t('hero_title')}</h1>
			<p class="pitch">{i18n.t('hero_pitch')}</p>

			<div class="to">
				<p class="to-head">
					<span class="label">{i18n.t('env_to')}</span>
				</p>
				<div class="tabs" role="tablist" aria-label={i18n.t('env_choose')}>
					{#each roles as role, i (role.id)}
						<button
							bind:this={tabs[i]}
							type="button"
							role="tab"
							id="role-tab-{role.id}"
							aria-selected={active === role.id}
							aria-controls="role-panel"
							tabindex={active === role.id ? 0 : -1}
							onclick={() => (active = role.id)}
							onkeydown={(e) => onTabKey(e, i)}
						>
							{i18n.t(`role_${role.id}`)}
						</button>
					{/each}
				</div>

				<div class="panel" id="role-panel" role="tabpanel" aria-labelledby="role-tab-{current.id}">
					{#key current.id}
						<div class="panel-inner" in:readdress>
							<p class="line">{i18n.t(`role_${current.id}_line`)}</p>
							<div class="actions">
								<a class="action" href={current.action} {...external(current.action)}>
									{i18n.t(`role_${current.id}_action`)}
									<svg class="arrow" width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
										<path d="M3 9h11M10 4.5 14.5 9 10 13.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" />
									</svg>
								</a>
								<a class="textlink" href={current.alt} {...external(current.alt)}>{i18n.t(`role_${current.id}_alt`)}</a>
							</div>
						</div>
					{/key}
				</div>
			</div>

			<p class="par-avion" aria-hidden="true">
				<span class="pa-main" lang="fr">{i18n.t('env_airmail')}</span>
				<span class="pa-sub">{i18n.t('env_airmail_sub')}</span>
			</p>
		</div>
	</div>
</section>

<style>
	.envelope {
		padding: 0 var(--margin) var(--margin);
	}

	/* The airmail border: diagonal red and blue bands on every edge. */
	.frame {
		max-width: var(--content-max);
		margin-inline: auto;
		padding: clamp(10px, 1.2vw, 16px);
		background: var(--paper) var(--airmail);
		background-size: 40px 40px;
	}

	/* Laid out like a real envelope: sender and airmail label top start, stamp
	   top end, the message large across, the delivery address lower end. */
	.face {
		position: relative;
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		grid-template-rows: auto auto 1fr auto;
		grid-template-areas:
			'from  stamp'
			'par   stamp'
			'h1    h1'
			'pitch to';
		gap: 1.5rem clamp(1.5rem, 4vw, 4rem);
		min-height: min(calc(100svh - 5.25rem), 58rem);
		padding: clamp(1.25rem, 3vw, 2.75rem);
		background: var(--paper);
	}

	.label {
		display: block;
		font-family: var(--font-display);
		font-size: var(--step-lead);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
		color: var(--red);
	}

	.from {
		grid-area: from;
		display: grid;
		align-content: start;
		justify-items: start;
		margin: 0;
		font-size: var(--step-small);
		line-height: 1.3;
	}

	.from .label {
		margin-bottom: 0.5rem;
	}

	.from strong {
		font-size: var(--step-body);
	}

	.from-line {
		color: var(--ink-2);
	}

	/* The stamp: Madmail's mark, perforated by a dotted border in the envelope's colour. */
	.stamp-corner {
		grid-area: stamp;
		position: relative;
		align-self: start;
		justify-self: end;
		width: clamp(9rem, 15vw, 14rem);
		margin-block-start: 0.5rem;
		margin-inline-end: 0.75rem;
	}

	.stamp {
		position: relative;
		display: block;
		width: 100%;
		padding: clamp(10px, 1.2vw, 16px) clamp(10px, 1.2vw, 16px) 0.4rem;
		border: 0;
		background: var(--paper-2);
		color: var(--ink);
		rotate: 4deg;
		cursor: pointer;
		transition: transform var(--dur-press) var(--ease-out);
	}

	.stamp:active {
		transform: scale(0.97);
	}

	.stamp::before {
		content: '';
		position: absolute;
		inset: -5px;
		border: 10px dotted var(--paper);
		pointer-events: none;
	}

	.stamp-picture {
		display: block;
		padding: 7%;
		border: 3px solid var(--red-field);
		background: var(--slip-paper);
	}

	.stamp-picture img {
		width: 100%;
		height: auto;
		aspect-ratio: 1;
	}

	.stamp-foot {
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		padding-block: 0.35rem 0.1rem;
		font-family: var(--font-display);
		font-size: clamp(0.875rem, 1.2vw, 1.125rem);
		font-weight: 800;
		line-height: 1;
		text-transform: uppercase;
	}

	.stamp-foot span:last-child {
		color: var(--red);
	}

	.stamp-corner :global(.postmark) {
		position: absolute;
		top: 56%;
		/* Physical, not logical: the SVG is forced LTR so its Latin text reads right. */
		right: 42%;
		width: 150%;
		color: var(--ink);
		opacity: 0.82;
		pointer-events: none;
		rotate: -8deg;
		mix-blend-mode: multiply;
	}

	:dir(rtl) .stamp-corner :global(.postmark) {
		right: auto;
		left: 42%;
		rotate: 8deg;
	}

	@media (prefers-reduced-motion: no-preference) {
		:global(.js) .stamp {
			animation: stamp-in 700ms var(--ease-out) 120ms both;
		}

		:global(.js) .stamp-corner :global(.postmark) {
			animation: cancel 520ms var(--ease-out) 620ms both;
		}
	}

	@keyframes stamp-in {
		from {
			opacity: 0;
			transform: translateY(-14px) rotate(-6deg);
		}
	}

	@keyframes cancel {
		from {
			opacity: 0;
			transform: scale(1.18);
		}
	}

	h1 {
		grid-area: h1;
		align-self: end;
		max-width: 14ch;
		margin: clamp(1.5rem, 4vw, 3rem) 0 0;
		font-family: var(--font-display);
		/* The second term only bites on phones: it keeps CHATMAIL inside the envelope. */
		font-size: min(var(--step-display), calc(17vw - 1rem));
		font-weight: 800;
		line-height: 0.92;
		letter-spacing: -0.01em;
		text-transform: uppercase;
		text-wrap: balance;
	}

	/* ch is measured on the Latin display face, far narrower than Persian letters. */
	:lang(fa) h1 {
		max-width: 11em;
		line-height: 1.2;
		letter-spacing: 0;
	}

	.pitch {
		grid-area: pitch;
		align-self: start;
		max-width: 40rem;
		margin: 0;
		font-size: var(--step-lead);
		line-height: 1.5;
		text-wrap: pretty;
	}

	.to {
		grid-area: to;
		align-self: end;
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: 0.75rem 1.5rem;
		align-items: baseline;
		max-width: 44rem;
		padding-top: 1.25rem;
		border-top: 3px solid var(--ink);
	}

	.to-head {
		grid-row: 1 / span 2;
		margin: 0;
	}

	.tabs {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1.1rem;
	}

	[role='tab'] {
		position: relative;
		padding: 0.35rem 0 0.45rem;
		border: 0;
		background: none;
		color: var(--ink-2);
		font: inherit;
		font-size: var(--step-body);
		font-weight: 700;
		cursor: pointer;
		transition: color var(--dur-ui) var(--ease-out);
	}

	[role='tab']::after {
		content: '';
		position: absolute;
		inset-inline: 0;
		bottom: 0;
		height: 3px;
		background: var(--red);
		transform: scaleX(0);
		transform-origin: var(--origin, left);
		transition: transform var(--dur-ui) var(--ease-out);
	}

	:dir(rtl) [role='tab']::after {
		--origin: right;
	}

	[role='tab'][aria-selected='true'] {
		color: var(--ink);
	}

	[role='tab'][aria-selected='true']::after {
		transform: scaleX(1);
	}

	[role='tab']:active {
		transform: scale(0.97);
	}

	@media (hover: hover) and (pointer: fine) {
		[role='tab']:hover {
			color: var(--ink);
		}
	}

	.panel {
		grid-column: 2;
		display: grid;
		min-height: 8.5rem;
	}

	.panel-inner {
		grid-area: 1 / 1;
	}

	.line {
		max-width: 34rem;
		margin: 0 0 1rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 1rem 1.75rem;
	}

	/* The blue airmail label, stuck on slightly crooked. */
	.par-avion {
		grid-area: par;
		justify-self: start;
		align-self: start;
		display: flex;
		flex-direction: column;
		align-items: center;
		margin: 0;
		padding: 0.55rem 1rem 0.6rem;
		background: var(--blue-field);
		color: var(--blue-field-ink);
		line-height: 1;
		rotate: -2deg;
	}

	.pa-main {
		font-family: var(--font-display);
		font-size: var(--step-h3);
		font-weight: 800;
		text-transform: uppercase;
	}

	.pa-sub {
		margin-top: 0.2rem;
		font-size: var(--step-small);
		font-weight: 600;
	}

	@media (max-width: 1360px) and (min-width: 901px) {
		.to {
			grid-template-columns: minmax(0, 1fr);
		}

		.to-head {
			grid-row: auto;
		}

		.panel {
			grid-column: 1;
		}
	}

	@media (max-width: 1140px) and (min-width: 901px) {
		.tabs {
			display: grid;
			grid-template-columns: repeat(2, max-content);
		}
	}

	@media (max-width: 900px) {
		.face {
			grid-template-columns: minmax(0, 1fr) auto;
			grid-template-rows: auto;
			grid-template-areas:
				'from  stamp'
				'par   stamp'
				'h1    h1'
				'pitch pitch'
				'to    to';
			min-height: auto;
		}

		.stamp-corner {
			width: clamp(6.5rem, 26vw, 9rem);
			margin-block-start: 0;
			margin-inline-end: 0.5rem;
		}
	}

	@media (max-width: 640px) {
		.face {
			gap: 2.5rem 1rem;
		}

		.stamp-corner :global(.postmark) {
			top: 62%;
			right: 30%;
			width: 112%;
		}

		:dir(rtl) .stamp-corner :global(.postmark) {
			right: auto;
			left: 30%;
		}

		.from-line {
			max-width: 9rem;
		}

		.to {
			grid-template-columns: 1fr;
		}

		.to-head {
			grid-row: auto;
		}

		.panel {
			grid-column: 1;
			min-height: 11rem;
		}
	}
</style>
