<script>
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import PhoneMark from '$lib/components/phone/PhoneMark.svelte';
	import PhoneShell from '$lib/components/phone/PhoneShell.svelte';
	import {
		describePhoneRelease,
		formatMegabytes,
		formatReleaseDate,
		latestPhone,
		phoneCertSha256,
		phoneReleases,
		phoneReleasesUrl
	} from '$lib/phone.js';

	const apkName = latestPhone?.apk?.name ?? 'madmail-mobile-<version>-arm64.apk';
	const verifyCommands = `sha256sum --ignore-missing -c SHA256SUMS\napksigner verify --print-certs ${apkName}`;
	const history = phoneReleases.slice(0, 8).map(describePhoneRelease);

	const requirements = [
		{ hue: 'red', icon: 'phone', label: 'Android', value: '8.0 or later' },
		{ hue: 'green', icon: 'chip', label: 'Processor', value: '64-bit ARM' },
		{ hue: 'teal', icon: 'wifi', label: 'For guests', value: 'Wi-Fi or hotspot' },
		{ hue: 'purple', icon: 'qr', label: 'On guest phones', value: 'Delta Chat' }
	];
</script>

<PhoneShell current="/phone/install">
	<article class="doc">
		<h1>Install Madmail Mobile</h1>
		<p class="lede">
			Madmail Mobile isn't in Google Play. You install the signed APK from GitHub, and newer versions
			update it in place.
		</p>

		<ul class="group requirements" aria-label="Requirements">
			{#each requirements as row}
				<li>
					<PhoneMark hue={row.hue} icon={row.icon} />
					<span>{row.label}</span>
					<span class="value">{row.value}</span>
				</li>
			{/each}
		</ul>

		{#if latestPhone}
			<div class="download">
				<div>
					<p class="version">Madmail Mobile {latestPhone.version}</p>
					<p class="small">
						{latestPhone.prerelease ? 'Pre-release' : 'Release'} from {formatReleaseDate(latestPhone.date)}{latestPhone.engine
							? `, with Madmail engine ${latestPhone.engine}`
							: ''}.
					</p>
				</div>
				<div class="download-actions">
					{#if latestPhone.apk}
						<a class="pill" href={latestPhone.apk.browser_download_url}>
							Download APK, {formatMegabytes(latestPhone.apk.size)}
						</a>
					{/if}
					<a class="pill pill--soft" href={latestPhone.url}>Release notes</a>
				</div>
				<p class="files small">
					{#if latestPhone.sums}<a href={latestPhone.sums.browser_download_url}>SHA256SUMS</a>{/if}
					{#if latestPhone.source}<a href={latestPhone.source.browser_download_url}>Source code</a>{/if}
					<a href={phoneReleasesUrl}>All releases</a>
				</p>
			</div>
		{:else}
			<p><a class="pill" href={phoneReleasesUrl}>Get the APK from GitHub</a></p>
		{/if}

		<h2>Set up hosting</h2>
		<ol class="steps">
			<li>Download the APK on the phone that will host the server.</li>
			<li>
				Open it. The first time, Android asks you to allow installs from your browser or file manager. Allow
				it for that app only.
			</li>
			<li>Open Madmail Mobile, tap <strong>Get started</strong> and choose <strong>This phone</strong>.</li>
			<li>
				Name the server and allow background access, so Android doesn't pause it when the screen is off.
				Then tap <strong>Add this phone</strong>.
			</li>
			<li>Turn on Wi-Fi or a hotspot, and tap <strong>Start hosting</strong> on the overview.</li>
			<li>Open <strong>Invite</strong> and let your guest scan the QR code in Delta Chat.</li>
		</ol>
		<p>
			Managing a server elsewhere? Choose <strong>Remote server</strong> to install Madmail on a Linux
			machine over SSH, or <strong>Existing instance</strong> to connect with an admin key.
		</p>

		<h2>Check the download</h2>
		<p>
			Every release APK is signed with the same certificate. Its SHA-256 fingerprint is
		</p>
		<p class="fingerprint">{phoneCertSha256}</p>
		<p>
			On a computer, put the APK next to <code>SHA256SUMS</code> and run these commands.
			<code>apksigner</code> comes with the Android SDK build tools. Its
			<em>certificate SHA-256 digest</em> line must match the fingerprint above.
		</p>
		<CodeBlock code={verifyCommands} />

		<h2>Update</h2>
		<p>
			On the overview, <strong>Versions</strong> lists releases from GitHub. Madmail Mobile checks a newer
			APK's package, signature and version before handing it to Android to install. You can also install a
			newer APK over the old one yourself. A backup is made before an upgraded engine first opens your data.
		</p>
		<div class="warning" role="note">
			<PhoneMark hue="orange" icon="alert" />
			<p>
				<strong>Don't uninstall to reinstall.</strong> Uninstalling deletes the server's private data,
				including its accounts and mail. A release APK also can't update a development build.
			</p>
		</div>

		{#if history.length}
			<h2>Recent releases</h2>
			<ul class="group history">
				{#each history as release}
					{#if release}
						<li>
							<a href={release.url}>{release.version}</a>
							<span class="value">{formatReleaseDate(release.date)}</span>
						</li>
					{/if}
				{/each}
			</ul>
		{/if}
	</article>
</PhoneShell>

<style>
	.group {
		margin: 2rem 0 0;
		padding: 0;
		list-style: none;
		border-radius: 1.25rem;
		background: var(--pm-group);
		color: var(--pm-ink);
		overflow: hidden;
		box-shadow: 0 0 0 0.6rem var(--pm-panel);
	}

	.group li {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: 0.85rem;
		min-height: 3.6rem;
		padding: 0.4rem 1.1rem;
		color: var(--pm-ink);
	}

	.group li + li {
		background: linear-gradient(var(--pm-divider), var(--pm-divider)) no-repeat 3.95rem 0 / 100% 1px;
	}

	.value {
		color: var(--pm-ink-soft);
	}

	.download {
		display: grid;
		gap: 1.1rem;
		margin-top: 2.5rem;
		padding: 1.6rem;
		border-radius: 1.6rem;
		border: 1px solid var(--color-border);
	}

	.download p {
		margin: 0;
	}

	.version {
		font-size: 1.3rem;
		font-weight: 700;
		color: var(--color-text) !important;
	}

	.small {
		font-size: 0.88rem;
	}

	.download-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
	}

	.files {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem 1.25rem;
	}

	.steps {
		margin: 0 0 1.25rem;
		padding-left: 1.4rem;
	}

	.steps li {
		padding-left: 0.3rem;
		margin-bottom: 0.55rem;
	}

	.steps li::marker {
		font-weight: 700;
		color: var(--pm-red);
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.88em;
		padding: 0.1em 0.35em;
		border-radius: 0.3rem;
		background: var(--color-surface-code);
	}

	.fingerprint {
		padding: 0.9rem 1.1rem;
		border-radius: 0.9rem;
		background: var(--color-surface);
		font-family: var(--font-mono);
		font-size: 0.85rem;
		color: var(--color-text) !important;
		overflow-wrap: anywhere;
	}

	.warning {
		display: flex;
		gap: 0.9rem;
		align-items: flex-start;
		margin-top: 1.25rem;
		padding: 1rem 1.1rem;
		border-radius: 1.1rem;
		background: color-mix(in srgb, #f08a1c 12%, transparent);
	}

	.warning p {
		margin: 0;
	}

	.history li {
		grid-template-columns: minmax(0, 1fr) auto;
		padding: 0.4rem 1.2rem;
	}

	.history li + li {
		background: linear-gradient(var(--pm-divider), var(--pm-divider)) no-repeat 1.2rem 0 / 100% 1px;
	}

	.history a {
		font-weight: 600;
		color: var(--pm-ink) !important;
		text-decoration: none;
	}

	.history a:hover {
		text-decoration: underline;
	}
</style>
