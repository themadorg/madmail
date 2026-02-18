<script lang="ts">
  import "./layout.css";
  import { page } from "$app/stores";
  import { base } from "$app/paths";
  import { store } from "$lib/state.svelte";
  import { t, getLocale, setLocale, LOCALES, type Locale } from "$lib/i18n";
  import {
    getSavedServers,
    removeServer,
    type SavedServer,
  } from "$lib/servers";
  import {
    Mail,
    Plug,
    RefreshCw,
    LogOut,
    AlertTriangle,
    Globe,
    LayoutDashboard,
    Settings,
    Network,
    Users,
    ShieldBan,
    Sun,
    Moon,
    Server,
    Trash2,
  } from "lucide-svelte";

  let { children } = $props();

  let langOpen = $state(false);
  let locale = $state(getLocale());

  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }

  function switchLocale(l: Locale) {
    setLocale(l);
    locale = l;
    langOpen = false;
  }

  let canConnect = $derived(
    store.baseUrl.length > 0 && store.token.length > 0 && !store.connecting,
  );

  // Theme toggle
  let isDark = $state(true);
  if (typeof localStorage !== "undefined") {
    const saved = localStorage.getItem("madmail_theme");
    isDark = saved !== "light";
  }
  $effect(() => {
    document.documentElement.setAttribute(
      "data-theme",
      isDark ? "dark" : "light",
    );
    localStorage.setItem("madmail_theme", isDark ? "dark" : "light");
  });
  function toggleTheme() {
    isDark = !isDark;
  }

  const NAV_ITEMS = [
    { href: "/", key: "tab.overview", icon: LayoutDashboard },
    { href: "/services", key: "tab.services", icon: Settings },
    { href: "/ports", key: "tab.ports", icon: Network },
    { href: "/accounts", key: "tab.accounts", icon: Users },
    { href: "/blocked", key: "tab.blocked", icon: ShieldBan },
    { href: "/dns", key: "tab.dns", icon: Globe },
  ];

  // Saved servers from IndexedDB
  let savedServers = $state<SavedServer[]>([]);

  async function loadSavedServers() {
    savedServers = await getSavedServers();
  }

  function selectServer(s: SavedServer) {
    store.baseUrl = s.url;
    store.token = s.token;
    store.connect();
  }

  async function deleteSavedServer(e: MouseEvent, id: string) {
    e.stopPropagation();
    await removeServer(id);
    await loadSavedServers();
  }

  // Auto-connect on mount
  let autoConnectDone = false;
  $effect(() => {
    if (!autoConnectDone && store.baseUrl && store.token) {
      autoConnectDone = true;
      store.connect();
    }
  });

  // Load saved servers when not connected
  $effect(() => {
    if (!store.connected) {
      loadSavedServers();
    }
  });

  function isActive(href: string, path: string): boolean {
    // Strip base prefix for comparison
    const p = base ? path.replace(base, '') || '/' : path;
    if (href === "/") return p === "/";
    return p.startsWith(href);
  }
</script>

<svelte:head>
  <title>Madmail Admin</title>
  <meta name="description" content="Madmail server administration dashboard" />
</svelte:head>

{#snippet langPicker()}
  <div class="relative">
    <button
      onclick={() => (langOpen = !langOpen)}
      class="p-1.5 text-text-2 border border-border rounded-lg hover:bg-surface-3 transition-colors flex items-center gap-1"
      title={_("misc.language")}
    >
      <Globe size={14} />
      <span class="text-xs">{LOCALES.find((l) => l.code === locale)?.flag}</span
      >
    </button>
    {#if langOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="absolute top-full mt-1 end-0 bg-surface-2 border border-border rounded-lg shadow-xl z-50 min-w-[140px] py-1 overflow-hidden"
        onclick={(e: MouseEvent) => e.stopPropagation()}
      >
        {#each LOCALES as l}
          <button
            onclick={() => switchLocale(l.code)}
            class="w-full px-3 py-2 text-start text-sm hover:bg-surface-3 transition-colors flex items-center gap-2
              {locale === l.code ? 'text-accent font-medium' : 'text-text'}"
          >
            <span>{l.flag}</span>
            <span>{l.label}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

<!-- Toast -->
{#if store.toast}
  <div
    class="fixed top-4 end-4 z-50 px-4 py-2 rounded-lg text-sm font-medium shadow-lg transition-all
    {store.toastType === 'ok'
      ? 'bg-success/20 text-success border border-success/30'
      : 'bg-danger/20 text-danger border border-danger/30'}"
  >
    {store.toast}
  </div>
{/if}

<!-- Login Gate -->
{#if !store.connected}
  <div
    class="min-h-screen bg-surface text-text flex items-center justify-center p-4"
    style="font-family: 'Inter', system-ui, sans-serif;"
  >
    <div
      class="w-full max-w-sm p-6 bg-surface-2 rounded-xl border border-border"
    >
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-2">
          <div class="p-2 bg-accent/15 rounded-lg">
            <Mail size={18} class="text-accent" />
          </div>
          <div>
            <h1 class="text-lg font-semibold">{_("login.title")}</h1>
            <p class="text-text-2 text-xs">{_("login.subtitle")}</p>
          </div>
        </div>
        {@render langPicker()}
      </div>

      <label for="url" class="block text-xs text-text-2 mb-1"
        >{_("login.url_label")}</label
      >
      <input
        id="url"
        type="url"
        bind:value={store.baseUrl}
        placeholder={_("login.url_placeholder")}
        class="w-full mb-3 px-3 py-2 bg-surface border border-border rounded-lg text-sm text-text placeholder-text-2/40 focus:border-accent focus:ring-1 focus:ring-accent/30 outline-none transition"
      />

      <label for="tok" class="block text-xs text-text-2 mb-1"
        >{_("login.token_label")}</label
      >
      <input
        id="tok"
        type="password"
        bind:value={store.token}
        placeholder={_("login.token_placeholder")}
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === "Enter") store.connect();
        }}
        class="w-full mb-4 px-3 py-2 bg-surface border border-border rounded-lg text-sm text-text placeholder-text-2/40 focus:border-accent focus:ring-1 focus:ring-accent/30 outline-none transition"
      />

      {#if store.connectError}
        <p class="text-danger text-xs mb-3 flex items-center gap-1">
          <AlertTriangle size={12} />
          {store.connectError}
        </p>
      {/if}

      <button
        onclick={() => store.connect()}
        disabled={!canConnect}
        class="w-full py-2.5 bg-accent hover:bg-accent-dim text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed flex items-center justify-center gap-2"
      >
        {#if store.connecting}
          <RefreshCw size={14} class="animate-spin" /> {_("login.connecting")}
        {:else}
          <Plug size={14} /> {_("login.connect")}
        {/if}
      </button>

      <!-- Saved Servers -->
      {#if savedServers.length > 0}
        <div class="mt-5 pt-4 border-t border-border">
          <h3
            class="text-xs text-text-2 font-medium mb-2.5 flex items-center gap-1.5"
          >
            <Server size={12} />
            {_("login.saved_servers")}
          </h3>
          <div class="space-y-1.5">
            {#each savedServers as s (s.id)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                onclick={() => selectServer(s)}
                class="w-full group flex items-center gap-2.5 px-3 py-2 bg-surface border border-border rounded-lg hover:border-accent/50 hover:bg-accent/5 transition-all text-start cursor-pointer"
              >
                <div class="p-1.5 bg-accent/10 rounded-md shrink-0">
                  <Server size={13} class="text-accent" />
                </div>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium truncate">{s.label}</div>
                  <div class="text-[11px] text-text-2 truncate">{s.url}</div>
                </div>
                <button
                  onclick={(e: MouseEvent) => deleteSavedServer(e, s.id)}
                  class="p-1 text-text-2 opacity-0 group-hover:opacity-100 hover:text-danger transition-all rounded"
                  title="Remove"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>
{:else}
  <!-- Authenticated Shell -->
  <div
    class="min-h-screen bg-surface text-text"
    style="font-family: 'Inter', system-ui, sans-serif;"
  >
    <div class="max-w-4xl mx-auto px-4 py-6">
      <!-- Header -->
      <header class="flex items-center justify-between mb-6">
        <div class="flex items-center gap-2">
          <a href="{base}/" class="p-1.5 bg-accent/15 rounded-lg">
            <Mail size={16} class="text-accent" />
          </a>
          <div>
            <h1 class="text-base font-semibold">Madmail</h1>
            <p class="text-text-2 text-[11px] truncate max-w-[200px]">
              {store.baseUrl}
            </p>
          </div>
        </div>
        <div class="flex items-center gap-1.5">
          {#if store.pendingRestart}
            <button
              onclick={() => store.reload()}
              disabled={store.reloading}
              class="px-3 py-1.5 bg-warning/20 text-warning text-xs font-medium rounded-lg border border-warning/30 hover:bg-warning/30 transition-colors disabled:opacity-50 flex items-center gap-1"
            >
              <AlertTriangle size={12} />
              {store.reloading
                ? _("action.restarting")
                : _("action.apply_restart")}
            </button>
          {/if}
          {@render langPicker()}
          <button
            onclick={toggleTheme}
            class="p-1.5 text-text-2 border border-border rounded-lg hover:bg-surface-3 transition-colors"
            title={isDark ? "Light mode" : "Dark mode"}
          >
            {#if isDark}
              <Sun size={14} />
            {:else}
              <Moon size={14} />
            {/if}
          </button>
          <button
            onclick={() => store.refresh()}
            disabled={store.refreshing}
            class="p-1.5 text-text-2 border border-border rounded-lg hover:bg-surface-3 transition-colors disabled:opacity-50"
            title={_("action.refresh")}
          >
            <RefreshCw
              size={14}
              class={store.refreshing ? "animate-spin" : ""}
            />
          </button>
          <button
            onclick={() => store.disconnect()}
            class="p-1.5 text-text-2 border border-border rounded-lg hover:bg-surface-3 transition-colors"
            title={_("action.disconnect")}
          >
            <LogOut size={14} />
          </button>
        </div>
      </header>

      <!-- Navigation -->
      <nav class="flex gap-0.5 mb-5 border-b border-border">
        {#each NAV_ITEMS as item}
          <a
            href="{base}{item.href}"
            class="px-3 py-2 text-sm transition-colors -mb-px flex items-center gap-1.5
              {isActive(item.href, $page.url.pathname)
              ? 'text-accent border-b-2 border-accent font-medium'
              : 'text-text-2 hover:text-text'}"
          >
            <svelte:component this={item.icon} size={13} />
            {_(item.key)}
          </a>
        {/each}
      </nav>

      <!-- Page Content -->
      {@render children()}
    </div>
  </div>
{/if}
