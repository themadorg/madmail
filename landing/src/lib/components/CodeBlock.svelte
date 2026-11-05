<script>
	import { useSiteLanguage } from '$lib/siteLanguage.svelte.js';
	const { t } = useSiteLanguage();
	import { CHECK_ICON, flashCodeBlock, UNDO_ICON } from '$lib/codeBlock.js';
	import copyIcon from '$lib/icons/document-duplicate.svg?raw';

	/** @type {{ code: string, wrap?: boolean, class?: string }} */
	let { code, wrap = false, class: className = '' } = $props();

	/** @type {HTMLDivElement | undefined} */
	let blockEl = $state();
	/** @type {HTMLTextAreaElement | undefined} */
	let textareaEl = $state();
	let draft = $state('');
	let editing = $state(false);
	let copied = $state(false);

	$effect(() => {
		draft = code;
		editing = false;
	});

	function startEdit() {
		editing = true;
		queueMicrotask(() => textareaEl?.focus());
	}

	function finishEdit() {
		editing = false;
	}

	function undo() {
		draft = code;
		queueMicrotask(() => textareaEl?.focus());
	}

	function keepFocus(event) {
		event.preventDefault();
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(draft.replace(/\n$/, ''));
		} catch {
			return;
		}

		if (blockEl) flashCodeBlock(blockEl);
		copied = true;
		setTimeout(() => {
			copied = false;
		}, 1500);
	}
</script>

<div
	class="code-block {className}"
	class:code-block--wrap={wrap}
	class:code-block--editing={editing}
	bind:this={blockEl}
>
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
	<pre onclick={startEdit} aria-hidden={editing}><code>{draft}</code></pre>
	{#if editing}
		<textarea
			bind:this={textareaEl}
			bind:value={draft}
			spellcheck="false"
			aria-label={t("Edit code")}
			onblur={finishEdit}
		></textarea>
	{/if}

	<div class="code-toolbar" role="toolbar" aria-label={t("Code actions")} tabindex="-1" onmousedown={keepFocus}>
		<button
			type="button"
			class="code-action code-copy"
			aria-label={t("Copy code")}
			data-copied={copied ? '' : undefined}
			onclick={copy}
		>
			{@html copied ? CHECK_ICON : copyIcon}
		</button>
	</div>

	{#if editing}
		<div class="code-undo-bar" role="toolbar" aria-label={t("Undo changes")} tabindex="-1" onmousedown={keepFocus}>
			<button
				type="button"
				class="code-action code-undo"
				aria-label={t("Undo changes")}
				disabled={draft === code}
				onclick={undo}
			>
				{@html UNDO_ICON}
			</button>
		</div>
	{/if}
</div>
