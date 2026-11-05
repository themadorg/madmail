<script>
	/**
	 * Gradient squircle mark from Madmail Mobile's design kit. The hue carries the
	 * same meaning as in the app: red this phone, blue remote server, green running
	 * and services, teal network, purple invites, orange warnings, yellow storage,
	 * pink activity, graphite security.
	 * @type {{ hue?: string, icon: keyof typeof glyphs, size?: 'row' | 'choice' | 'hero', label?: string }}
	 */
	let { hue = 'gray', icon, size = 'row', label = '' } = $props();

	const glyphs = {
		phone: '<rect x="7" y="2.75" width="10" height="18.5" rx="2.4"/><path d="M10.75 18.1h2.5"/>',
		server: '<rect x="4" y="4" width="16" height="6.5" rx="1.8"/><rect x="4" y="13.5" width="16" height="6.5" rx="1.8"/><path d="M7.5 7.25h.01M7.5 16.75h.01"/>',
		link: '<path d="M10 13.5a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-.8.8"/><path d="M14 10.5a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l.8-.8"/>',
		qr: '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2.5v2.5M20 14v.01M14 20h.01M17.5 20H20v-3"/>',
		wifi: '<path d="M3.5 9.5a12 12 0 0 1 17 0M6.5 12.75a7.75 7.75 0 0 1 11 0M9.5 16a3.5 3.5 0 0 1 5 0"/><path d="M12 19.25h.01"/>',
		lock: '<rect x="5" y="10.5" width="14" height="10" rx="2.2"/><path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5"/>',
		key: '<circle cx="8" cy="15" r="4"/><path d="M11 12l8.5-8.5M16.5 6.5l2 2M14.5 8.5l1.5 1.5"/>',
		chip: '<rect x="6.5" y="6.5" width="11" height="11" rx="2"/><path d="M9.5 3.5v3M14.5 3.5v3M9.5 17.5v3M14.5 17.5v3M3.5 9.5h3M3.5 14.5h3M17.5 9.5h3M17.5 14.5h3"/>',
		battery: '<rect x="3.5" y="7.5" width="15" height="9" rx="2"/><path d="M21 10.75v2.5M7 10.5v3"/>',
		mail: '<rect x="3.5" y="5.5" width="17" height="13" rx="2"/><path d="m4 7 8 6 8-6"/>',
		pulse: '<path d="M3.5 12h4l2.5-6 4 12 2.5-6h4"/>',
		box: '<path d="M4 7.5 12 3.5l8 4v9l-8 4-8-4z"/><path d="M4 7.5 12 11.5l8-4M12 11.5v9"/>',
		alert: '<path d="M12 4 3 19.5h18z"/><path d="M12 10v4M12 16.75h.01"/>'
	};
</script>

<span
	class="mark mark--{size} hue-{hue}"
	role={label ? 'img' : undefined}
	aria-label={label || undefined}
	aria-hidden={label ? undefined : 'true'}
>
	<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">
		{@html glyphs[icon]}
	</svg>
</span>

<style>
	.mark {
		--a: #b4b4b4;
		--b: #7a7a7a;
		display: inline-grid;
		flex: none;
		place-items: center;
		width: var(--s);
		height: var(--s);
		/* A superellipse reads closer to the Android squircle than a plain radius. */
		border-radius: calc(var(--s) * 0.3);
		background: linear-gradient(160deg, var(--a), var(--b));
		color: #fff;
		box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.25);
	}

	.mark--row { --s: 2rem; }
	.mark--choice { --s: 2.5rem; }
	.mark--hero { --s: 3.5rem; }

	svg {
		width: 58%;
		height: 58%;
	}

	.hue-red { --a: #ff6b6b; --b: #df2730; }
	.hue-blue { --a: #5aaeff; --b: #1d6ae0; }
	.hue-green { --a: #4fdc86; --b: #189e4f; }
	.hue-teal { --a: #47d3d6; --b: #11929e; }
	.hue-purple { --a: #b48bff; --b: #7238e0; }
	.hue-orange { --a: #ffb24d; --b: #ee7110; }
	.hue-yellow { --a: #ffd84f; --b: #e5a800; }
	.hue-indigo { --a: #8189ff; --b: #4549d1; }
	.hue-pink { --a: #ff86ba; --b: #dc3f86; }
	.hue-graphite { --a: #8f949c; --b: #464b53; }
</style>
