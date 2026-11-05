<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { localize, siteLanguage, t } = useSiteLanguage();
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import HeaderDropdown from './HeaderDropdown.svelte';
	import languageIcon from '$lib/icons/language.svg?raw';
	/** @type {{ languages?: {label:string,href:string,current:boolean}[], id?: string }} */
	let { languages = [], id = 'site-language' } = $props();
	const items = $derived.by(() => {
		const current = page.url.pathname + (browser ? page.url.search + page.url.hash : '');
		const choices = [
			{ label: 'English', lang: 'en' },
			{ label: 'فارسی', lang: 'fa' },
			...languages.filter((item) => !['English', 'فارسی'].includes(item.label)).map((item) => ({ label: item.label, lang: item.label === 'Русский' ? 'ru' : 'es' }))
		];
		return choices.map((choice) => ({ ...choice,
			href: localize(current, choice.lang),
			current: siteLanguage.code === choice.lang
		}));
	});
</script>
<HeaderDropdown {id} label={t('Change language')} menuLabel={t('Website language')} icon={languageIcon} align="right" {items} />
