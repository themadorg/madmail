<script lang="ts">
  import { store } from "$lib/state.svelte";
  import { t, getLocale } from "$lib/i18n";
  import { ShieldBan, ShieldCheck, SearchX } from "lucide-svelte";

  let locale = $state(getLocale());
  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }
  $effect(() => {
    locale = getLocale();
  });

  let search = $state("");
  let confirmingUnblock = $state("");

  let filtered = $derived.by(() => {
    if (!store.blocklist) return [];
    if (!search) return store.blocklist.blocked;
    const q = search.toLowerCase();
    return store.blocklist.blocked.filter(
      (e) =>
        e.username.toLowerCase().includes(q) ||
        e.reason.toLowerCase().includes(q),
    );
  });

  function formatDate(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return iso;
    }
  }

  function requestUnblock(username: string) {
    confirmingUnblock = username;
  }

  async function handleConfirmUnblock() {
    const user = confirmingUnblock;
    confirmingUnblock = "";
    await store.unblockUser(user);
  }
</script>

{#if store.blocklist}
  <div
    class="bg-surface-2 rounded-lg border border-border mb-4 p-3 flex flex-wrap justify-between items-center gap-2"
  >
    <span class="text-sm flex items-center gap-1.5">
      <ShieldBan size={14} class="text-danger/70" />
      {store.blocklist.total}
      {_("acct.blocked")}
    </span>
  </div>

  <!-- Search -->
  {#if store.blocklist.total > 0}
    <div class="mb-3">
      <input
        type="text"
        bind:value={search}
        placeholder="Search blocked users..."
        class="w-full px-3 py-2 bg-surface-2 border border-border rounded-lg text-sm text-text placeholder-text-2/40 focus:border-accent focus:ring-1 focus:ring-accent/30 outline-none transition"
      />
    </div>
  {/if}

  {#if filtered.length === 0 && store.blocklist.total === 0}
    <div class="flex flex-col items-center justify-center py-16 text-text-2/50">
      <ShieldCheck size={32} class="mb-2" />
      <p class="text-sm">No blocked users</p>
    </div>
  {:else if filtered.length === 0}
    <div class="flex flex-col items-center justify-center py-12 text-text-2/50">
      <SearchX size={24} class="mb-2" />
      <p class="text-sm">No results for "{search}"</p>
    </div>
  {:else}
    <div class="space-y-1">
      {#each filtered as entry (entry.username)}
        <div
          class="flex items-center justify-between bg-surface-2 rounded-lg px-3 py-2.5 border border-border group"
        >
          <div class="min-w-0 flex-1">
            <span
              class="text-xs sm:text-sm font-mono truncate block text-text-2 line-through decoration-danger/40"
              >{entry.username}</span
            >
            <div class="flex items-center gap-2 mt-0.5">
              <span class="text-[10px] text-text-2/50">{entry.reason}</span>
              {#if entry.blocked_at}
                <span class="text-[10px] text-text-2/30"
                  >· {formatDate(entry.blocked_at)}</span
                >
              {/if}
            </div>
          </div>
          <div class="flex items-center shrink-0 ms-2">
            <button
              onclick={() => requestUnblock(entry.username)}
              class="px-2 py-1.5 text-accent/60 border border-transparent rounded-lg hover:border-accent/30 hover:bg-accent/10 transition-all text-xs flex items-center gap-1"
              aria-label="Unblock {entry.username}"
            >
              <ShieldCheck size={12} />
              <span class="hidden sm:inline">{_("acct.unblock")}</span>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
{:else}
  <p class="text-text-2 text-sm">{_("misc.loading")}</p>
{/if}

<!-- Unblock confirmation modal -->
{#if confirmingUnblock}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
    onclick={() => (confirmingUnblock = "")}
  >
    <!-- Backdrop -->
    <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="relative bg-surface-1 border border-border rounded-xl shadow-2xl w-full max-w-sm p-5 animate-modal"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-start gap-3 mb-4">
        <div class="p-2 rounded-lg bg-accent/10 text-accent shrink-0">
          <ShieldCheck size={18} />
        </div>
        <div class="min-w-0">
          <h3 class="text-sm font-semibold text-text-1 mb-1">
            {_("acct.unblock")}
          </h3>
          <p class="text-xs text-text-2 font-mono break-all">
            {confirmingUnblock}
          </p>
        </div>
      </div>

      <div class="flex gap-2 justify-end">
        <button
          onclick={() => (confirmingUnblock = "")}
          class="px-3 py-1.5 text-xs rounded-lg border border-border text-text-2 hover:bg-surface-2 transition-colors"
        >
          {_("action.cancel")}
        </button>
        <button
          onclick={handleConfirmUnblock}
          class="px-3 py-1.5 text-xs rounded-lg bg-accent text-white hover:bg-accent-dim transition-colors font-medium"
        >
          {_("acct.unblock")}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
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
  .animate-modal {
    animation: modal-in 0.15s ease-out;
  }
</style>
