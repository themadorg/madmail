<script lang="ts">
  import { store } from "$lib/state.svelte";
  import { t, getLocale } from "$lib/i18n";
  import {
    Users,
    Trash2,
    ArrowUpDown,
    UserPlus,
    Copy,
    ExternalLink,
    Check,
    AlertTriangle,
  } from "lucide-svelte";

  let locale = $state(getLocale());
  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }
  $effect(() => {
    locale = getLocale();
  });

  let sortBy = $state<"name" | "size" | "date" | "login">("name");
  let sortAsc = $state(true);
  let copied = $state(false);

  function fmtDate(ts: number): string {
    if (!ts) return _("acct.never");
    const d = new Date(ts * 1000);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 60_000) return "just now";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
    if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)}d ago`;
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year:
        d.getFullYear() !== new Date().getFullYear() ? "numeric" : undefined,
    });
  }

  let sortedAccounts = $derived.by(() => {
    if (!store.accounts) return [];
    const list = [...store.accounts.accounts];
    if (sortBy === "size") {
      list.sort((a, b) =>
        sortAsc ? a.used_bytes - b.used_bytes : b.used_bytes - a.used_bytes,
      );
    } else if (sortBy === "date") {
      list.sort((a, b) =>
        sortAsc ? a.created_at - b.created_at : b.created_at - a.created_at,
      );
    } else if (sortBy === "login") {
      list.sort((a, b) =>
        sortAsc
          ? a.last_login_at - b.last_login_at
          : b.last_login_at - a.last_login_at,
      );
    } else {
      list.sort((a, b) =>
        sortAsc
          ? a.username.localeCompare(b.username)
          : b.username.localeCompare(a.username),
      );
    }
    return list;
  });

  let isRegistrationClosed = $derived(
    store.settings?.registration === "closed",
  );

  function toggleSort(col: "name" | "size" | "date" | "login") {
    if (sortBy === col) {
      sortAsc = !sortAsc;
    } else {
      sortBy = col;
      sortAsc = col === "name";
    }
  }

  function buildDcloginLink(email: string, password: string): string {
    // Extract hostname from the admin API URL
    let host = "";
    try {
      const url = new URL(store.baseUrl);
      host = url.hostname;
    } catch {
      host = "localhost";
    }

    // Use smtp_hostname from settings if available (more accurate)
    if (store.settings?.smtp_hostname?.value) {
      host = store.settings.smtp_hostname.value;
    }

    const imapPort = store.settings?.imap_port?.value || "993";
    const smtpPort = store.settings?.submission_port?.value || "465";

    return `dclogin:${email}/?p=${encodeURIComponent(password)}&v=1&ih=${host}&ip=${imapPort}&sh=${host}&sp=${smtpPort}&ic=3&ss=default`;
  }

  function copyDclogin() {
    if (!store.newAccount) return;
    const link = buildDcloginLink(
      store.newAccount.email,
      store.newAccount.password,
    );
    navigator.clipboard.writeText(link);
    copied = true;
    setTimeout(() => {
      copied = false;
    }, 2000);
  }

  function openDeltaChat() {
    if (!store.newAccount) return;
    const link = buildDcloginLink(
      store.newAccount.email,
      store.newAccount.password,
    );
    window.location.href = link;
  }
</script>

{#if store.accounts}
  <div
    class="bg-surface-2 rounded-lg border border-border mb-4 p-3 flex flex-wrap justify-between items-center gap-2"
  >
    <span class="text-sm flex items-center gap-1.5">
      <Users size={14} class="text-text-2" />
      {store.accounts.total}
      {_("acct.total")}
    </span>
    <div class="flex items-center gap-3 text-xs text-text-2">
      {#if store.quota}
        <span>
          {store.fmtBytes(store.quota.total_storage_bytes)}
          {_("acct.used")} · {store.fmtBytes(store.quota.default_quota_bytes)}
          {_("acct.quota")}
        </span>
      {/if}
    </div>
  </div>

  <!-- Create Account Button — only when registration is closed -->
  {#if isRegistrationClosed}
    <div class="mb-4 p-3 rounded-lg border border-amber-500/20 bg-amber-500/5">
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <div class="flex items-center gap-2 text-xs text-amber-400">
          <AlertTriangle size={14} />
          <span>{_("acct.create_hint")}</span>
        </div>
        <button
          onclick={() => store.createAccount()}
          disabled={store.busy}
          class="px-3 py-1.5 text-xs rounded-lg bg-accent text-white hover:bg-accent/80 transition-colors font-medium flex items-center gap-1.5 disabled:opacity-50"
        >
          <UserPlus size={12} />
          {_("acct.create")}
        </button>
      </div>
    </div>
  {/if}

  <div class="flex gap-2 mb-3 flex-wrap">
    <button
      onclick={() => toggleSort("name")}
      class="px-2.5 py-1 text-xs rounded-lg border transition-colors flex items-center gap-1
        {sortBy === 'name'
        ? 'border-accent/40 text-accent bg-accent/5'
        : 'border-border text-text-2 hover:border-text-2/40'}"
    >
      <ArrowUpDown size={11} />
      {_("acct.sort_name")}
      {#if sortBy === "name"}<span class="text-[10px] opacity-60"
          >{sortAsc ? "↑" : "↓"}</span
        >{/if}
    </button>
    <button
      onclick={() => toggleSort("size")}
      class="px-2.5 py-1 text-xs rounded-lg border transition-colors flex items-center gap-1
        {sortBy === 'size'
        ? 'border-accent/40 text-accent bg-accent/5'
        : 'border-border text-text-2 hover:border-text-2/40'}"
    >
      <ArrowUpDown size={11} />
      {_("acct.sort_size")}
      {#if sortBy === "size"}<span class="text-[10px] opacity-60"
          >{sortAsc ? "↑" : "↓"}</span
        >{/if}
    </button>
    <button
      onclick={() => toggleSort("date")}
      class="px-2.5 py-1 text-xs rounded-lg border transition-colors flex items-center gap-1
        {sortBy === 'date'
        ? 'border-accent/40 text-accent bg-accent/5'
        : 'border-border text-text-2 hover:border-text-2/40'}"
    >
      <ArrowUpDown size={11} />
      {_("acct.sort_date")}
      {#if sortBy === "date"}<span class="text-[10px] opacity-60"
          >{sortAsc ? "↑" : "↓"}</span
        >{/if}
    </button>
    <button
      onclick={() => toggleSort("login")}
      class="px-2.5 py-1 text-xs rounded-lg border transition-colors flex items-center gap-1
        {sortBy === 'login'
        ? 'border-accent/40 text-accent bg-accent/5'
        : 'border-border text-text-2 hover:border-text-2/40'}"
    >
      <ArrowUpDown size={11} />
      {_("acct.sort_login")}
      {#if sortBy === "login"}<span class="text-[10px] opacity-60"
          >{sortAsc ? "↑" : "↓"}</span
        >{/if}
    </button>
  </div>

  <div class="space-y-1">
    {#each sortedAccounts as acct (acct.username)}
      <div
        class="flex items-center justify-between bg-surface-2 rounded-lg px-3 py-2 border border-border group"
      >
        <div class="min-w-0 flex-1">
          <span class="text-xs sm:text-sm font-mono truncate block"
            >{acct.username}</span
          >
          <div class="flex gap-3 mt-0.5 text-[10px] text-text-2">
            <span>{_("acct.created")}: {fmtDate(acct.created_at)}</span>
            <span>{_("acct.last_login")}: {fmtDate(acct.last_login_at)}</span>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0 ms-2">
          <span class="text-xs text-text-2 tabular-nums"
            >{store.fmtBytes(acct.used_bytes)}</span
          >
          <button
            onclick={() => store.requestDelete(acct.username)}
            class="p-1.5 text-danger/60 border border-transparent rounded hover:border-danger/30 hover:bg-danger/10 sm:opacity-0 sm:group-hover:opacity-100 transition-all"
            aria-label="Delete {acct.username}"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>
    {/each}
  </div>
{:else}
  <p class="text-text-2 text-sm">{_("misc.loading")}</p>
{/if}

<!-- New Account Created Modal (one-time dclogin display) -->
{#if store.newAccount}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
    onclick={() => {}}
  >
    <!-- Backdrop (non-dismissible — user must explicitly acknowledge) -->
    <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="relative bg-surface-1 border border-border rounded-xl shadow-2xl w-full max-w-md p-5 animate-modal"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="flex items-start gap-3 mb-4">
        <div class="p-2 rounded-lg bg-green-500/10 text-green-400 shrink-0">
          <UserPlus size={18} />
        </div>
        <div class="min-w-0">
          <h3 class="text-sm font-semibold text-text-1 mb-1">
            {_("acct.new_account")}
          </h3>
          <p class="text-xs text-text-2 font-mono break-all">
            {store.newAccount.email}
          </p>
        </div>
      </div>

      <!-- Warning -->
      <div
        class="mb-4 p-2.5 rounded-lg bg-amber-500/10 border border-amber-500/20 flex items-center gap-2"
      >
        <AlertTriangle size={14} class="text-amber-400 shrink-0" />
        <span class="text-xs text-amber-400">{_("acct.dclogin_warning")}</span>
      </div>

      <!-- dclogin link display -->
      <div class="mb-4 p-3 rounded-lg bg-black/30 border border-border">
        <p class="text-[10px] text-text-2 mb-1 uppercase tracking-wider">
          dclogin
        </p>
        <p
          class="text-xs font-mono text-accent break-all select-all leading-relaxed"
          dir="ltr"
        >
          {buildDcloginLink(store.newAccount.email, store.newAccount.password)}
        </p>
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex gap-2">
          <button
            onclick={copyDclogin}
            class="flex-1 px-3 py-2 text-xs rounded-lg border transition-colors font-medium flex items-center justify-center gap-1.5
              {copied
              ? 'border-green-500/40 text-green-400 bg-green-500/10'
              : 'border-accent/30 text-accent hover:bg-accent/10'}"
          >
            {#if copied}
              <Check size={12} />
              Copied!
            {:else}
              <Copy size={12} />
              {_("acct.copy_dclogin")}
            {/if}
          </button>
          <button
            onclick={openDeltaChat}
            class="flex-1 px-3 py-2 text-xs rounded-lg border border-border text-text-2 hover:bg-surface-2 transition-colors font-medium flex items-center justify-center gap-1.5"
          >
            <ExternalLink size={12} />
            {_("acct.open_dc")}
          </button>
        </div>
        <button
          onclick={() => store.dismissNewAccount()}
          class="w-full px-3 py-2 text-xs rounded-lg bg-surface-2 text-text-1 hover:bg-surface-2/80 transition-colors font-medium border border-border"
        >
          {_("acct.dismiss")}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Delete confirmation modal -->
{#if store.confirmingDelete}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4"
    onclick={() => store.cancelDelete()}
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
        <div class="p-2 rounded-lg bg-danger/10 text-danger shrink-0">
          <Trash2 size={18} />
        </div>
        <div class="min-w-0">
          <h3 class="text-sm font-semibold text-text-1 mb-1">
            {_("acct.confirm_delete", { username: "" })}
          </h3>
          <p class="text-xs text-text-2 font-mono break-all">
            {store.confirmingDelete}
          </p>
        </div>
      </div>

      <div class="flex gap-2 justify-end">
        <button
          onclick={() => store.cancelDelete()}
          class="px-3 py-1.5 text-xs rounded-lg border border-border text-text-2 hover:bg-surface-2 transition-colors"
        >
          {_("acct.confirm_no")}
        </button>
        <button
          onclick={() => store.confirmDelete(store.confirmingDelete)}
          class="px-3 py-1.5 text-xs rounded-lg bg-danger text-white hover:bg-danger/80 transition-colors font-medium"
        >
          {_("acct.confirm_yes")}
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
