<script>
	import { onMount } from 'svelte';
	import PhoneFrame from '$lib/components/phone/PhoneFrame.svelte';
	import PhoneMark from '$lib/components/phone/PhoneMark.svelte';
	import PhoneShell from '$lib/components/phone/PhoneShell.svelte';
	import deltachatIcon from '$lib/icons/deltachat.svg?raw';
	import { runMatrixResolve } from '$lib/matrixScramble.js';
	import { formatMegabytes, latestPhone, phoneCertSha256, phonePorts, phoneReleasesUrl } from '$lib/phone.js';
	import invite from '$lib/assets/phone/invite.webp';
	import mobileData from '$lib/assets/phone/mobile-data.webp';
	import onboarding from '$lib/assets/phone/onboarding.webp';
	import overview from '$lib/assets/phone/overview.webp';
	import phoneSetup from '$lib/assets/phone/phone-setup.webp';
	import servers from '$lib/assets/phone/servers.webp';
	import settings from '$lib/assets/phone/settings.webp';
	import statistics from '$lib/assets/phone/statistics.webp';

	const downloadHref = latestPhone?.apk?.browser_download_url ?? phoneReleasesUrl;

	const address = 'you@[192.168.43.1]';
	let shown = $state(address);

	onMount(() => {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		return runMatrixResolve(address, (letters) => (shown = letters.join('')));
	});

	const guests = [
		{ name: 'k7xq2m@[192.168.43.1]', pos: 'g1' },
		{ name: 'rw4ta9@[192.168.43.1]', pos: 'g2' },
		{ name: 'n0pz5e@[192.168.43.1]', pos: 'g3' }
	];

	const choices = [
		{ hue: 'red', icon: 'phone', title: 'This phone', text: 'Hosts the server on the phone itself, for guests on its Wi-Fi or hotspot.' },
		{ hue: 'blue', icon: 'server', title: 'Remote server', text: 'Installs Madmail on your Linux VPS over SSH and keeps it running, with a backup before each upgrade.' },
		{ hue: 'green', icon: 'link', title: 'Existing instance', text: 'Connects to a Madmail server you already run, using its admin key.' }
	];

	const screens = [
		{ src: onboarding, caption: 'First run. The title decodes out of falling glyphs.' },
		{ src: phoneSetup, caption: 'Phone setup checks the network and asks for background access.' },
		{ src: overview, caption: 'Overview with the live status, the hosting switch and connection details.' },
		{ src: mobileData, caption: 'On mobile data the status turns orange and offers Wi-Fi and hotspot shortcuts.' },
		{ src: statistics, caption: 'Live activity, message counts and storage.' },
		{ src: invite, caption: 'Invite shows the guest address, the account QR and invitation links.' },
		{ src: settings, caption: 'Settings, grouped into server, delivery and advanced.' },
		{ src: servers, caption: 'Long-press a saved server for its quick actions.' }
	];

	const groups = [
		{
			caption: 'Engine',
			rows: [
				{ hue: 'green', icon: 'chip', label: 'Madmail engine', value: latestPhone?.engine ?? 'bundled' },
				{ hue: 'graphite', icon: 'key', label: 'Checked before start', value: 'pinned SHA-256' },
				{ hue: 'pink', icon: 'pulse', label: 'If the app dies', value: 'engine stops too' }
			],
			footer: 'The same Rust server as on Linux, built for ARM64 Android.'
		},
		{
			caption: 'Network',
			rows: [
				{ hue: 'blue', icon: 'mail', label: 'Mail address', value: 'name@[phone IP]', mono: true },
				...phonePorts.map((port) => ({ hue: 'teal', icon: 'wifi', label: port.label, value: port.value, mono: true }))
			],
			footer: 'High ports, so the phone needs no root.'
		},
		{
			caption: 'Security',
			rows: [
				{ hue: 'graphite', icon: 'key', label: 'Saved keys and tokens', value: 'Android Keystore' },
				{ hue: 'graphite', icon: 'lock', label: 'App lock', value: 'fingerprint or PIN' },
				{ hue: 'yellow', icon: 'box', label: 'Before an upgrade', value: 'cold backup' }
			],
			footer: 'App lock guards the controls. Hosting keeps running behind it.'
		}
	];
</script>

<PhoneShell current="/phone">
	<section class="hero">
		<div class="hero-copy">
			<h1>Your phone is the mail server.</h1>
			<p class="lede">
				Madmail Mobile runs a Madmail chatmail server on an Android phone. Turn on a hotspot, show a QR
				code, and the people around you chat in Delta Chat with end-to-end encryption. No domain, no VPS.
			</p>
			<p class="address" aria-label="Example address: {address}">
				<span aria-hidden="true">{shown}</span>
			</p>
			<p class="address-note">Addresses use the phone's IP, so there's nothing to register.</p>
			<div class="actions">
				<a class="pill" href={downloadHref}>
					Download for Android
					<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3.5v10m0 0-4-4m4 4 4-4M4 16.5h12" /></svg>
				</a>
				<a class="pill pill--soft" href="/phone/install">How to install</a>
			</div>
			{#if latestPhone}
				<p class="meta">
					Version {latestPhone.version}{latestPhone.prerelease ? ' pre-release' : ''}{latestPhone.apk
						? `, ${formatMegabytes(latestPhone.apk.size)}`
						: ''}. For ARM64 phones with Android 8 or later.
				</p>
			{/if}
		</div>

		<div class="stage" aria-hidden="true">
			<div class="rings">
				<span></span><span></span><span></span>
			</div>
			{#each guests as guest}
				<span class="guest {guest.pos}">
					<span class="dot"></span>{guest.name}
				</span>
			{/each}
			<PhoneFrame src={overview} alt="" eager class="hero-device" />
		</div>
	</section>

	<section class="choose">
		<div class="choose-copy">
			<h2>One app, three places to run it</h2>
			<p>
				The first screen asks where the server lives. Pick the phone in your hand, a VPS you rent, or a
				Madmail you already run. Every server you add gets the same overview, invites and settings.
			</p>
			<p class="aside">
				Madmail Mobile isn't a chat app. Guests chat in
				<a href="https://delta.chat">Delta Chat</a>; this app runs the server they connect to.
			</p>
		</div>
		<div class="sheet">
			<ul class="group">
				{#each choices as choice}
					<li class="choice">
						<PhoneMark hue={choice.hue} icon={choice.icon} size="choice" />
						<div>
							<strong>{choice.title}</strong>
							<span>{choice.text}</span>
						</div>
					</li>
				{/each}
			</ul>
		</div>
	</section>

	<section class="join">
		<h2>How a guest joins</h2>
		<ol class="steps">
			<li>
				<PhoneFrame src={overview} alt="Overview screen with the hosting switch and server status" />
				<div>
					<h3>Start hosting</h3>
					<p>
						Tap Start hosting on the overview. The status dot turns green only after the server answers its
						health check.
					</p>
				</div>
			</li>
			<li>
				<PhoneFrame src={invite} alt="Invite screen with the guest address and an account QR code" />
				<div>
					<h3>Show the QR code</h3>
					<p>
						Invite makes a personal account QR, or invitation links with use limits and an expiry. While the
						QR is on screen it changes every five seconds, so one code means one guest.
					</p>
				</div>
			</li>
			<li>
				<div class="dc-card">
					<span class="dc-icon">{@html deltachatIcon}</span>
					<span class="dc-title">Delta Chat</span>
					<span class="dc-row"><span class="dot"></span>Encrypted chat</span>
				</div>
				<div>
					<h3>Scan it in Delta Chat</h3>
					<p>
						The guest scans the code in Delta Chat and has an account on your phone. Messages between guests
						are end-to-end encrypted.
					</p>
				</div>
			</li>
		</ol>
	</section>

	<section class="screens" aria-labelledby="screens-title">
		<div class="screens-head">
			<h2 id="screens-title">Around the app</h2>
			<p>Built natively in Kotlin and Jetpack Compose. It follows your phone's light or dark theme.</p>
		</div>
		<ul class="strip">
			{#each screens as screen}
				<li>
					<PhoneFrame src={screen.src} alt={screen.caption} />
					<p>{screen.caption}</p>
				</li>
			{/each}
		</ul>
	</section>

	<section class="inside">
		<h2>Under the hood</h2>
		<div class="inside-grid">
			{#each groups as group}
				<div class="panel">
					<h3 class="caption">{group.caption}</h3>
					<ul class="group">
						{#each group.rows as row}
							<li class="row">
								<PhoneMark hue={row.hue} icon={row.icon} />
								<span class="row-label">{row.label}</span>
								<span class="row-value" class:mono={row.mono}>{row.value}</span>
							</li>
						{/each}
					</ul>
					<p class="footer">{group.footer}</p>
				</div>
			{/each}
		</div>
	</section>

	<section class="limits">
		<h2>Not there yet</h2>
		<p class="limits-intro">Madmail Mobile is a pre-release. Here's what it can't do yet.</p>
		<ul>
			<li>Android only, on ARM64 phones. There's no iPhone version and it isn't in Google Play.</li>
			<li>Guests on mobile data may not reach you. Many carriers put phones behind shared addresses.</li>
			<li>Hosting keeps the address it started with. After switching networks, stop and start hosting.</li>
			<li>Calls and attachments between guests haven't been fully tested on real networks.</li>
			<li>Root ports, guided backup restore and some web admin settings aren't in the app yet.</li>
			<li>Translations haven't been reviewed by native speakers.</li>
		</ul>
	</section>

	<section class="cta">
		<PhoneMark hue="red" icon="phone" size="hero" />
		<h2>Try it on a spare phone</h2>
		<p>An Android phone and its hotspot are enough to see it work. Your main phone joins as a guest.</p>
		<div class="actions">
			<a class="pill" href={downloadHref}>Download{latestPhone ? ` ${latestPhone.version}` : ''}</a>
			<a class="pill pill--soft" href="/phone/faq">Read the questions</a>
		</div>
		<p class="cert">
			Every release is signed with the same key.<br />
			<span class="mono">SHA-256 {phoneCertSha256}</span>
		</p>
	</section>
</PhoneShell>

<style>
	section {
		max-width: 72rem;
		margin: 0 auto;
		padding: 0 1.5rem;
	}

	h2 {
		margin: 0 0 1rem;
		font-size: clamp(1.6rem, 3.4vw, 2.3rem);
		font-weight: 700;
		letter-spacing: -0.025em;
		line-height: 1.1;
	}

	h3 {
		margin: 0 0 0.4rem;
		font-size: 1.05rem;
		font-weight: 650;
	}

	p {
		margin: 0;
		line-height: 1.6;
	}

	.mono {
		font-family: var(--font-mono);
	}

	/* Hero */
	.hero {
		display: grid;
		grid-template-columns: minmax(0, 1.05fr) minmax(0, 0.95fr);
		align-items: center;
		gap: 2rem;
		min-height: calc(100dvh - 4rem);
		/* Rings and guest chips may spill past the stage; clip without making a scroll container. */
		overflow-x: clip;
		padding-top: 2rem;
		padding-bottom: 3rem;
	}

	h1 {
		margin: 0 0 1.4rem;
		font-size: clamp(2.6rem, 6.6vw, 5rem);
		font-weight: 800;
		line-height: 0.98;
		letter-spacing: -0.045em;
		text-wrap: balance;
	}

	.lede {
		max-width: 33rem;
		font-size: 1.12rem;
		color: var(--color-text-muted);
	}

	.address {
		margin-top: 1.75rem;
		font-family: var(--font-mono);
		font-size: clamp(1.15rem, 2.6vw, 1.6rem);
		font-weight: 500;
		color: var(--color-text);
		white-space: nowrap;
	}

	.address-note {
		margin-top: 0.3rem;
		font-size: 0.88rem;
		color: var(--color-text-subtle);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-top: 2rem;
	}

	.actions svg {
		width: 1.1rem;
		height: 1.1rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.8;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.meta {
		margin-top: 1rem;
		font-size: 0.85rem;
		color: var(--color-text-subtle);
	}

	.stage {
		position: relative;
		display: grid;
		place-items: center;
		min-height: 36rem;
		--device-width: 15.5rem;
	}

	.stage :global(.hero-device) {
		position: relative;
		z-index: 2;
		transform: rotate(-4deg);
	}

	/* Wi-Fi range: the one ambient motion on the page. */
	.rings {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
	}

	.rings span {
		grid-area: 1 / 1;
		width: 30rem;
		aspect-ratio: 1;
		border-radius: 50%;
		border: 1px solid color-mix(in srgb, var(--pm-red) 55%, transparent);
		background: radial-gradient(circle, color-mix(in srgb, var(--pm-red) 10%, transparent), transparent 70%);
		opacity: 0;
		animation: ripple 6s cubic-bezier(0.2, 0, 0, 1) infinite;
	}

	.rings span:nth-child(2) { animation-delay: 2s; }
	.rings span:nth-child(3) { animation-delay: 4s; }

	@keyframes ripple {
		0% { transform: scale(0.35); opacity: 0; }
		12% { opacity: 1; }
		100% { transform: scale(1.15); opacity: 0; }
	}

	.guest {
		position: absolute;
		z-index: 3;
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.8rem;
		border-radius: 0.9rem;
		background: var(--pm-group);
		color: var(--pm-ink);
		font-family: var(--font-mono);
		font-size: 0.78rem;
		box-shadow:
			0 0 0 1px var(--pm-divider),
			0 0.6rem 1.6rem -0.6rem rgb(0 0 0 / 0.45);
	}

	.g1 { top: 12%; left: 0; }
	.g2 { top: 46%; right: -1rem; }
	.g3 { bottom: 10%; left: 6%; }

	.dot {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		background: #2fca6b;
		box-shadow: 0 0 0 3px color-mix(in srgb, #2fca6b 25%, transparent);
	}

	/* Choose */
	.choose {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 3rem;
		align-items: center;
		padding-top: 6rem;
		padding-bottom: 6rem;
	}

	.choose-copy p {
		max-width: 30rem;
		color: var(--color-text-muted);
	}

	.choose-copy .aside {
		margin-top: 1.25rem;
		font-size: 0.92rem;
		color: var(--color-text-subtle);
	}

	.choose-copy a {
		color: var(--color-text);
	}

	.sheet {
		padding: 1.25rem;
		border-radius: 1.75rem;
		background: var(--pm-panel);
	}

	.group {
		margin: 0;
		padding: 0;
		list-style: none;
		border-radius: 1.25rem;
		background: var(--pm-group);
		color: var(--pm-ink);
		overflow: hidden;
	}

	.choice {
		display: flex;
		gap: 0.9rem;
		align-items: flex-start;
		padding: 1.05rem 1.1rem;
	}

	.choice + .choice {
		box-shadow: inset 0 1px 0 var(--pm-divider);
	}

	.choice div {
		display: grid;
		gap: 0.2rem;
	}

	.choice strong {
		font-size: 1rem;
	}

	.choice span {
		font-size: 0.9rem;
		line-height: 1.5;
		color: var(--pm-ink-soft);
	}

	/* Join */
	.join {
		padding-bottom: 6rem;
	}

	.steps {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 2rem;
		margin: 2rem 0 0;
		padding: 0;
		list-style: none;
		counter-reset: step;
	}

	.steps li {
		display: grid;
		grid-template-rows: auto 1fr;
		gap: 1.5rem;
		justify-items: start;
		counter-increment: step;
		--device-width: 12.5rem;
	}

	.steps h3::before {
		content: counter(step) '. ';
		color: var(--pm-red);
	}

	.steps p {
		max-width: 21rem;
		font-size: 0.95rem;
		color: var(--color-text-muted);
	}

	.dc-card {
		display: grid;
		align-content: center;
		justify-items: center;
		gap: 0.75rem;
		width: 12.5rem;
		aspect-ratio: 9 / 20;
		border-radius: 1.9rem;
		background: var(--pm-panel);
		color: var(--pm-ink);
	}

	.dc-icon :global(svg) {
		width: 4rem;
		height: 4rem;
	}

	.dc-title {
		font-weight: 650;
	}

	.dc-row {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.45rem 0.75rem;
		border-radius: 999px;
		background: var(--pm-group);
		font-size: 0.8rem;
	}

	/* Screens */
	.screens {
		max-width: none;
		padding: 5rem 0;
		background: var(--pm-panel);
		color: var(--pm-ink);
	}

	.screens-head {
		max-width: 72rem;
		margin: 0 auto;
		padding: 0 1.5rem;
	}

	.screens-head p {
		color: var(--pm-ink-soft);
	}

	.strip {
		display: flex;
		gap: 1.75rem;
		margin: 2.5rem 0 0;
		padding: 0.5rem max(1.5rem, calc((100vw - 69rem) / 2)) 1.5rem;
		list-style: none;
		overflow-x: auto;
		scroll-snap-type: x mandatory;
		scroll-padding-inline: max(1.5rem, calc((100vw - 69rem) / 2));
		--device-width: 13rem;
	}

	.strip li {
		flex: none;
		width: 13rem;
		scroll-snap-align: start;
	}

	.strip p {
		margin-top: 1rem;
		font-size: 0.86rem;
		line-height: 1.5;
		color: var(--pm-ink-soft);
	}

	/* Inside */
	.inside {
		padding-top: 6rem;
		padding-bottom: 5rem;
	}

	.inside-grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 1.25rem;
		margin-top: 1.5rem;
	}

	.panel {
		padding: 1.1rem;
		border-radius: 1.6rem;
		background: var(--pm-panel);
	}

	.caption {
		margin: 0.2rem 0 0.6rem 1rem;
		font-size: 0.82rem;
		font-weight: 600;
		color: var(--pm-ink-soft);
	}

	.row {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.85rem;
		min-height: 3.6rem;
		padding: 0.4rem 1rem;
		font-size: 0.9rem;
	}

	/* Dividers start where the text does, as in the app. */
	.row + .row {
		background: linear-gradient(var(--pm-divider), var(--pm-divider)) no-repeat 3.85rem 0 / 100% 1px;
	}

	.row-value {
		color: var(--pm-ink-soft);
		text-align: right;
	}

	.footer {
		margin: 0.6rem 1rem 0.2rem;
		font-size: 0.8rem;
		color: var(--pm-ink-soft);
	}

	/* Limits */
	.limits {
		max-width: 46rem;
		padding-bottom: 6rem;
	}

	.limits-intro {
		color: var(--color-text-muted);
	}

	.limits ul {
		display: grid;
		gap: 0.75rem;
		margin: 1.5rem 0 0;
		padding: 0;
		list-style: none;
	}

	.limits li {
		position: relative;
		padding-left: 1.4rem;
		line-height: 1.55;
		color: var(--color-text-muted);
	}

	.limits li::before {
		content: '';
		position: absolute;
		left: 0;
		top: 0.6em;
		width: 0.5rem;
		height: 0.5rem;
		border-radius: 50%;
		background: #f08a1c;
	}

	/* CTA */
	.cta {
		display: grid;
		justify-items: center;
		gap: 0.25rem;
		max-width: 44rem;
		padding-top: 4rem;
		padding-bottom: 2rem;
		text-align: center;
	}

	.cta h2 {
		margin-top: 1.25rem;
		margin-bottom: 0.5rem;
	}

	.cta > p {
		color: var(--color-text-muted);
	}

	.cta .actions {
		justify-content: center;
		margin-top: 1.5rem;
	}

	.cert {
		margin-top: 2rem;
		font-size: 0.78rem;
		color: var(--color-text-faint);
		overflow-wrap: anywhere;
	}

	@media (prefers-reduced-motion: reduce) {
		.rings span {
			animation: none;
			opacity: 0.6;
		}

		.rings span:nth-child(1) { transform: scale(0.55); }
		.rings span:nth-child(2) { transform: scale(0.8); }
		.rings span:nth-child(3) { transform: scale(1.05); }
	}

	@media (max-width: 900px) {
		.hero,
		.choose {
			grid-template-columns: minmax(0, 1fr);
		}

		.hero {
			min-height: 0;
		}

		.stage {
			min-height: 30rem;
			--device-width: 13rem;
		}

		.rings span {
			width: 24rem;
		}

		.g1 { left: 2%; }
		.g2 { right: 2%; }

		.steps,
		.inside-grid {
			grid-template-columns: minmax(0, 1fr);
		}

		.steps li {
			grid-template-columns: auto minmax(0, 1fr);
			grid-template-rows: none;
			align-items: center;
			--device-width: 8.5rem;
		}

		.dc-card {
			width: 8.5rem;
			border-radius: 1.3rem;
		}

		.dc-icon :global(svg) {
			width: 2.75rem;
			height: 2.75rem;
		}

		.dc-row {
			font-size: 0.7rem;
		}
	}

	@media (max-width: 480px) {
		.stage {
			min-height: 26rem;
			--device-width: 11rem;
		}

		.rings span {
			width: 20rem;
		}

		.guest {
			font-size: 0.68rem;
			padding: 0.4rem 0.6rem;
		}

		.g2 { right: 0; }

		.steps li {
			--device-width: 6.5rem;
		}

		.dc-card {
			width: 6.5rem;
		}

		.dc-title,
		.dc-row {
			display: none;
		}
	}
</style>
