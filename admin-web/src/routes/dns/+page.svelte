<script lang="ts">
  import { store } from "$lib/state.svelte";
  import { t, getLocale } from "$lib/i18n";
  import { Globe, Trash2, Plus, ArrowRight, SearchX } from "lucide-svelte";

  let locale = $state(getLocale());
  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }
  $effect(() => {
    locale = getLocale();
  });

  let search = $state("");
  let showAdd = $state(false);
  let newLookup = $state("");
  let newTarget = $state("");
  let newComment = $state("");
  let confirmingDelete = $state("");

  let filtered = $derived.by(() => {
    if (!store.dnsOverrides) return [];
    if (!search) return store.dnsOverrides.overrides;
    const q = search.toLowerCase();
    return store.dnsOverrides.overrides.filter(
      (e) =>
        e.lookup_key.toLowerCase().includes(q) ||
        e.target_host.toLowerCase().includes(q) ||
        (e.comment && e.comment.toLowerCase().includes(q)),
    );
  });

  async function handleAdd() {
    if (!newLookup.trim() || !newTarget.trim()) return;
    await store.addDnsOverride(
      newLookup.trim(),
      newTarget.trim(),
      newComment.trim(),
    );
    newLookup = "";
    newTarget = "";
    newComment = "";
    showAdd = false;
  }

  function requestDelete(lookupKey: string) {
    confirmingDelete = confirmingDelete === lookupKey ? "" : lookupKey;
  }

  async function handleConfirmDelete() {
    const key = confirmingDelete;
    confirmingDelete = "";
    await store.deleteDnsOverride(key);
  }
</script>

{#if store.dnsOverrides}
  <div class="dns-header">
    <span class="dns-count">
      <Globe size={14} />
      {_("dns.total", { count: String(store.dnsOverrides.total) })}
    </span>
    <button class="btn-add" onclick={() => (showAdd = !showAdd)}>
      <Plus size={12} />
      {_("dns.add")}
    </button>
  </div>

  <!-- Add form -->
  {#if showAdd}
    <div class="add-form">
      <div class="form-grid">
        <div class="field">
          <label for="dns-lookup">{_("dns.lookup")}</label>
          <input
            id="dns-lookup"
            type="text"
            bind:value={newLookup}
            placeholder="example.com or 1.2.3.4"
          />
        </div>
        <div class="field">
          <label for="dns-target">{_("dns.target")}</label>
          <input
            id="dns-target"
            type="text"
            bind:value={newTarget}
            placeholder="5.6.7.8"
          />
        </div>
      </div>
      <div class="field">
        <label for="dns-comment">{_("dns.comment")}</label>
        <input
          id="dns-comment"
          type="text"
          bind:value={newComment}
          placeholder="Optional note..."
        />
      </div>
      <div class="form-actions">
        <button class="btn-cancel" onclick={() => (showAdd = false)}>
          {_("action.cancel")}
        </button>
        <button
          class="btn-submit"
          onclick={handleAdd}
          disabled={!newLookup.trim() || !newTarget.trim()}
        >
          {_("dns.add")}
        </button>
      </div>
    </div>
  {/if}

  <!-- Search -->
  {#if store.dnsOverrides.total > 0}
    <div class="search-wrap">
      <input
        type="text"
        bind:value={search}
        placeholder="Search overrides..."
        class="search-input"
      />
    </div>
  {/if}

  {#if filtered.length === 0 && store.dnsOverrides.total === 0 && !showAdd}
    <div class="empty-state">
      <Globe size={32} />
      <p>{_("dns.empty")}</p>
      <button class="btn-add" onclick={() => (showAdd = true)}>
        <Plus size={12} />
        {_("dns.add")}
      </button>
    </div>
  {:else if filtered.length === 0 && store.dnsOverrides.total > 0}
    <div class="empty-state">
      <SearchX size={24} />
      <p>No results for "{search}"</p>
    </div>
  {:else}
    <div class="entry-list">
      {#each filtered as entry (entry.lookup_key)}
        <div class="entry">
          <div class="entry-info">
            <div class="entry-mapping">
              <span class="lookup-key">{entry.lookup_key}</span>
              <ArrowRight size={12} class="arrow-icon" />
              <span class="target-host">{entry.target_host}</span>
            </div>
            {#if entry.comment}
              <span class="entry-comment">{entry.comment}</span>
            {/if}
          </div>
          <div class="entry-actions">
            <button
              class="btn-delete"
              onclick={() => requestDelete(entry.lookup_key)}
              aria-label="Delete {entry.lookup_key}"
            >
              <Trash2 size={13} />
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
{:else}
  <p class="loading">{_("misc.loading")}</p>
{/if}

<!-- Delete confirmation modal -->
{#if confirmingDelete}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="modal-overlay" onclick={() => (confirmingDelete = "")}>
    <div class="modal-backdrop"></div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="modal-box" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <div class="modal-icon">
          <Trash2 size={18} />
        </div>
        <div class="modal-title">
          <h3>{_("dns.confirm_delete", { key: "" })}</h3>
          <p class="modal-detail">{confirmingDelete}</p>
        </div>
      </div>
      <div class="modal-actions">
        <button class="btn-cancel" onclick={() => (confirmingDelete = "")}>
          {_("action.cancel")}
        </button>
        <button class="btn-danger" onclick={handleConfirmDelete}>
          Delete
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Header bar ── */
  .dns-header {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem;
    margin-bottom: 1rem;
    background: var(--th-surface-2);
    border: 1px solid var(--th-border);
    border-radius: 0.5rem;
  }

  .dns-count {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.875rem;
    color: var(--th-text);
  }
  .dns-count :global(svg) {
    color: var(--th-accent);
    opacity: 0.7;
  }

  /* ── Buttons ── */
  .btn-add {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.375rem 0.625rem;
    font-size: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--th-accent) 30%, transparent);
    border-radius: 0.5rem;
    color: var(--th-accent);
    background: transparent;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
  }
  .btn-add:hover {
    background: color-mix(in srgb, var(--th-accent) 10%, transparent);
  }

  .btn-cancel {
    padding: 0.375rem 0.75rem;
    font-size: 0.75rem;
    border: 1px solid var(--th-border);
    border-radius: 0.5rem;
    color: var(--th-text-2);
    background: transparent;
    cursor: pointer;
    transition: background 0.15s;
  }
  .btn-cancel:hover {
    background: var(--th-surface-2);
  }

  .btn-submit {
    padding: 0.375rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 500;
    border: none;
    border-radius: 0.5rem;
    color: #fff;
    background: var(--th-accent);
    cursor: pointer;
    transition: background 0.15s;
  }
  .btn-submit:hover {
    background: var(--th-accent-dim);
  }
  .btn-submit:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .btn-delete {
    padding: 0.375rem;
    border: none;
    border-radius: 0.5rem;
    background: transparent;
    color: var(--th-danger);
    opacity: 0.35;
    cursor: pointer;
    transition:
      opacity 0.15s,
      background 0.15s;
  }
  .btn-delete:hover {
    opacity: 1;
    background: color-mix(in srgb, var(--th-danger) 10%, transparent);
  }

  .btn-danger {
    padding: 0.375rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 500;
    border: none;
    border-radius: 0.5rem;
    color: #fff;
    background: var(--th-danger);
    cursor: pointer;
    transition: background 0.15s;
  }
  .btn-danger:hover {
    opacity: 0.85;
  }

  /* ── Add form ── */
  .add-form {
    padding: 1rem;
    margin-bottom: 1rem;
    background: var(--th-surface-2);
    border: 1px solid color-mix(in srgb, var(--th-accent) 20%, transparent);
    border-radius: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    animation: slide-in 0.15s ease-out;
  }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }
  @media (max-width: 480px) {
    .form-grid {
      grid-template-columns: 1fr;
    }
  }

  .field label {
    display: block;
    font-size: 0.6875rem;
    color: var(--th-text-2);
    opacity: 0.7;
    margin-bottom: 0.25rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .field input {
    width: 100%;
    padding: 0.5rem 0.75rem;
    background: var(--th-surface-1);
    border: 1px solid var(--th-border);
    border-radius: 0.5rem;
    font-size: 0.875rem;
    color: var(--th-text);
    outline: none;
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
    box-sizing: border-box;
  }
  .field input::placeholder {
    color: var(--th-text-2);
    opacity: 0.35;
  }
  .field input:focus {
    border-color: var(--th-accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--th-accent) 20%, transparent);
  }

  .form-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  /* ── Search ── */
  .search-wrap {
    margin-bottom: 0.75rem;
  }

  .search-input {
    width: 100%;
    padding: 0.5rem 0.75rem;
    background: var(--th-surface-2);
    border: 1px solid var(--th-border);
    border-radius: 0.5rem;
    font-size: 0.875rem;
    color: var(--th-text);
    outline: none;
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
    box-sizing: border-box;
  }
  .search-input::placeholder {
    color: var(--th-text-2);
    opacity: 0.4;
  }
  .search-input:focus {
    border-color: var(--th-accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--th-accent) 20%, transparent);
  }

  /* ── Empty state ── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 4rem 0;
    color: var(--th-text-2);
    opacity: 0.5;
  }
  .empty-state :global(svg) {
    margin-bottom: 0.5rem;
    opacity: 0.4;
  }
  .empty-state p {
    font-size: 0.875rem;
    margin: 0 0 0.75rem;
  }

  /* ── Entry list ── */
  .entry-list {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .entry {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.625rem 0.75rem;
    background: var(--th-surface-2);
    border: 1px solid var(--th-border);
    border-radius: 0.5rem;
    transition: border-color 0.15s;
  }
  .entry:hover {
    border-color: color-mix(in srgb, var(--th-accent) 30%, var(--th-border));
  }
  .entry:hover .btn-delete {
    opacity: 1;
  }

  .entry-info {
    min-width: 0;
    flex: 1;
  }

  .entry-mapping {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .lookup-key {
    font-size: 0.8125rem;
    font-family: ui-monospace, "Cascadia Code", "Source Code Pro", Menlo,
      monospace;
    color: var(--th-text);
    word-break: break-all;
  }

  .entry-mapping :global(.arrow-icon) {
    color: var(--th-text-2);
    opacity: 0.3;
    flex-shrink: 0;
  }

  .target-host {
    font-size: 0.8125rem;
    font-family: ui-monospace, "Cascadia Code", "Source Code Pro", Menlo,
      monospace;
    color: var(--th-accent);
    opacity: 0.85;
    word-break: break-all;
  }

  .entry-comment {
    display: block;
    margin-top: 0.125rem;
    font-size: 0.625rem;
    color: var(--th-text-2);
    opacity: 0.5;
  }

  .entry-actions {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-inline-start: 0.5rem;
  }

  /* ── Modal ── */
  .modal-overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
  }

  .modal-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(4px);
  }

  .modal-box {
    position: relative;
    background: var(--th-surface-1);
    border: 1px solid var(--th-border);
    border-radius: 0.75rem;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.25);
    width: 100%;
    max-width: 24rem;
    padding: 1.25rem;
    animation: modal-in 0.15s ease-out;
  }

  .modal-header {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }

  .modal-icon {
    padding: 0.5rem;
    border-radius: 0.5rem;
    background: color-mix(in srgb, var(--th-danger) 10%, transparent);
    color: var(--th-danger);
    flex-shrink: 0;
  }

  .modal-title h3 {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--th-text);
    margin: 0 0 0.25rem;
  }

  .modal-detail {
    font-size: 0.75rem;
    font-family: ui-monospace, "Cascadia Code", "Source Code Pro", Menlo,
      monospace;
    color: var(--th-text-2);
    word-break: break-all;
    margin: 0;
  }

  .modal-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }

  /* ── Loading ── */
  .loading {
    font-size: 0.875rem;
    color: var(--th-text-2);
  }

  /* ── Animations ── */
  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateY(-8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes modal-in {
    from {
      opacity: 0;
      transform: scale(0.95) translateY(8px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }
</style>
