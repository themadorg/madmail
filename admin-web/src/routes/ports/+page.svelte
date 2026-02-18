<script lang="ts">
  import { store } from "$lib/state.svelte";
  import { t, getLocale } from "$lib/i18n";
  import {
    Pencil,
    RotateCcw,
    AlertTriangle,
    Globe,
    Lock,
    ShieldAlert,
  } from "lucide-svelte";

  let locale = $state(getLocale());
  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }

  /**
   * Map port setting keys to their access control key prefix.
   * Ports without access control (like Shadowsocks itself, SASL) are excluded.
   */
  const accessKeyMap: Record<string, string> = {
    smtp_port: "smtp",
    submission_port: "submission",
    imap_port: "imap",
    turn_port: "turn",
    sasl_port: "sasl",
    iroh_port: "iroh",
    http_port: "http",
    https_port: "https",
  };

  /** Map port keys to their display labels for the confirmation modal */
  const portLabels: Record<string, string> = {
    smtp_port: "SMTP",
    submission_port: "Submission",
    imap_port: "IMAP",
    turn_port: "TURN",
    sasl_port: "SASL",
    iroh_port: "Iroh",
    http_port: "HTTP",
    https_port: "HTTPS",
  };

  function getAccess(portKey: string): string | null {
    const prefix = accessKeyMap[portKey];
    if (!prefix || !store.settings) return null;
    const accessKey = `${prefix}_access` as keyof typeof store.settings;
    return (store.settings[accessKey] as string) || "public";
  }

  // Confirmation modal state
  let confirmingPort = $state<string | null>(null);
  let confirmingLabel = $state("");

  function requestMakeLocal(portKey: string) {
    confirmingPort = portKey;
    confirmingLabel = portLabels[portKey] || portKey;
  }

  function cancelConfirm() {
    confirmingPort = null;
    confirmingLabel = "";
  }

  async function confirmMakeLocal() {
    if (!confirmingPort) return;
    const prefix = accessKeyMap[confirmingPort];
    if (prefix) {
      await store.togglePortAccess(prefix, "public");
    }
    confirmingPort = null;
    confirmingLabel = "";
  }

  // Client-affecting port change warning
  const clientPorts = new Set(["smtp_port", "submission_port", "imap_port"]);
  let pendingSaveKey = $state<string | null>(null);
  let pendingSaveValue = $state("");

  function handleSave(key: string, value: string) {
    if (clientPorts.has(key)) {
      pendingSaveKey = key;
      pendingSaveValue = value;
    } else {
      store.save(key, value);
    }
  }

  async function confirmPortChange() {
    if (!pendingSaveKey) return;
    await store.save(pendingSaveKey, pendingSaveValue);
    pendingSaveKey = null;
    pendingSaveValue = "";
  }

  function cancelPortChange() {
    pendingSaveKey = null;
    pendingSaveValue = "";
  }
</script>

{#if store.settings}
  <div class="space-y-2">
    <!-- Hint about Local mode -->
    <div class="text-xs text-text-2 flex items-center gap-1.5 mb-3 px-1">
      <Lock size={11} class="opacity-60" />
      <span>{_("port.access_hint")}</span>
    </div>

    {#snippet editableRow(key: string, label: string, fallback: string)}
      {@const access = getAccess(key)}
      <div
        class="flex items-center justify-between bg-surface-2 rounded-lg p-3 border border-border gap-3"
      >
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium">{label}</div>
          {#if store.editingField === key}
            <div class="flex gap-2 mt-1.5">
              <input
                type="number"
                bind:value={store.editValue}
                min="1"
                max="65535"
                class="w-28 px-2 py-1 bg-surface border border-border rounded text-xs text-text outline-none focus:border-accent"
              />
              <button
                onclick={() => handleSave(key, store.editValue)}
                class="px-2 py-1 bg-accent text-white text-xs rounded hover:bg-accent-dim transition-colors"
                >{_("action.save")}</button
              >
              <button
                onclick={() => (store.editingField = "")}
                class="px-2 py-1 text-text-2 text-xs border border-border rounded hover:bg-surface-3 transition-colors"
                >{_("action.cancel")}</button
              >
            </div>
          {:else}
            <div class="text-xs text-text-2 truncate">
              {store.setting(key).is_set ? store.setting(key).value : fallback}
              {#if !store.setting(key).is_set}<span class="opacity-50"
                  >{_("misc.default")}</span
                >{/if}
            </div>
          {/if}
        </div>
        {#if store.editingField !== key}
          <div class="flex gap-1 shrink-0 items-center">
            <!-- Access toggle (Public / Local) -->
            {#if access}
              <button
                onclick={() => {
                  const prefix = accessKeyMap[key];
                  if (!prefix || !access) return;
                  if (access === "local") {
                    // Going local → public: safe, do it directly
                    store.togglePortAccess(prefix, access);
                  } else {
                    // Going public → local: show confirmation modal
                    requestMakeLocal(key);
                  }
                }}
                disabled={store.busy}
                class="flex items-center gap-1 px-2 py-1 text-xs rounded border transition-colors {access ===
                'local'
                  ? 'bg-warning/15 border-warning/40 text-warning'
                  : 'bg-success/10 border-success/30 text-success'} hover:opacity-80 disabled:opacity-50"
                title={access === "local" ? _("port.local") : _("port.public")}
              >
                {#if access === "local"}
                  <Lock size={11} />
                {:else}
                  <Globe size={11} />
                {/if}
                <span
                  >{access === "local"
                    ? _("port.local")
                    : _("port.public")}</span
                >
              </button>
            {/if}

            {#if store.setting(key).is_set}<span
                class="w-1.5 h-1.5 rounded-full bg-warning"
              ></span>{/if}
            <button
              onclick={() =>
                store.startEdit(
                  key,
                  store.setting(key).is_set
                    ? store.setting(key).value
                    : fallback,
                )}
              class="p-1.5 text-text-2 border border-border rounded hover:bg-surface-3 transition-colors"
            >
              <Pencil size={12} />
            </button>
            {#if store.setting(key).is_set}
              <button
                onclick={() => store.reset(key)}
                class="p-1.5 text-danger border border-danger/30 rounded hover:bg-danger/10 transition-colors"
              >
                <RotateCcw size={12} />
              </button>
            {/if}
          </div>
        {/if}
      </div>
    {/snippet}

    {@render editableRow("smtp_port", _("port.smtp"), "25")}
    {@render editableRow("submission_port", _("port.submission"), "587")}
    {@render editableRow("imap_port", _("port.imap"), "993")}
    {@render editableRow("turn_port", _("port.turn"), "3478")}
    {@render editableRow("sasl_port", _("port.sasl"), "24")}
    {@render editableRow("iroh_port", _("port.iroh"), "3340")}
    {@render editableRow("ss_port", _("port.shadowsocks"), "8388")}
    {@render editableRow("http_port", _("port.http"), "80")}
    {@render editableRow("https_port", _("port.https"), "443")}
  </div>

  {#if store.pendingRestart}
    <div
      class="mt-4 p-3 bg-warning/10 border border-warning/30 rounded-lg flex items-center justify-between"
    >
      <span class="text-warning text-sm flex items-center gap-1.5">
        <AlertTriangle size={14} />
        {_("action.restart_needed")}
      </span>
      <button
        onclick={() => store.reload()}
        disabled={store.reloading}
        class="px-3 py-1.5 bg-warning text-black text-xs font-medium rounded-lg hover:bg-warning/80 transition-colors disabled:opacity-50"
      >
        {store.reloading ? _("action.restarting") : _("action.apply_restart")}
      </button>
    </div>
  {/if}

  <!-- Confirmation Modal: Public → Local Only -->
  {#if confirmingPort}
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4"
    >
      <div
        class="bg-surface-2 border border-border rounded-xl p-5 max-w-sm w-full shadow-2xl"
      >
        <div class="flex items-center gap-2 mb-3">
          <div class="p-2 bg-warning/15 rounded-lg">
            <ShieldAlert size={20} class="text-warning" />
          </div>
          <h3 class="text-base font-semibold text-text">
            {_("port.confirm_local", { port: confirmingLabel })}
          </h3>
        </div>

        <p class="text-sm text-text-2 mb-5 leading-relaxed">
          {_("port.confirm_local_warn")}
        </p>

        <div class="flex gap-2 justify-end">
          <button
            onclick={cancelConfirm}
            class="px-3 py-1.5 text-text-2 text-xs border border-border rounded-lg hover:bg-surface-3 transition-colors"
          >
            {_("port.confirm_no")}
          </button>
          <button
            onclick={confirmMakeLocal}
            disabled={store.busy}
            class="px-3 py-1.5 bg-warning text-black text-xs font-medium rounded-lg hover:bg-warning/80 transition-colors disabled:opacity-50"
          >
            {_("port.confirm_yes")}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Warning Modal: Client-Affecting Port Change -->
  {#if pendingSaveKey}
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 p-4"
    >
      <div
        class="bg-surface-2 border border-border rounded-xl p-5 max-w-sm w-full shadow-2xl"
      >
        <div class="flex items-center gap-2 mb-3">
          <div class="p-2 bg-warning/15 rounded-lg">
            <AlertTriangle size={20} class="text-warning" />
          </div>
          <h3 class="text-base font-semibold text-text">
            {_("port.client_warn_title", {
              port: portLabels[pendingSaveKey] || pendingSaveKey,
            })}
          </h3>
        </div>

        <p class="text-sm text-text-2 mb-5 leading-relaxed">
          {_("port.client_warn_body")}
        </p>

        <div class="flex gap-2 justify-end">
          <button
            onclick={cancelPortChange}
            class="px-3 py-1.5 text-text-2 text-xs border border-border rounded-lg hover:bg-surface-3 transition-colors"
          >
            {_("port.confirm_no")}
          </button>
          <button
            onclick={confirmPortChange}
            disabled={store.busy}
            class="px-3 py-1.5 bg-warning text-black text-xs font-medium rounded-lg hover:bg-warning/80 transition-colors disabled:opacity-50"
          >
            {_("port.client_warn_confirm")}
          </button>
        </div>
      </div>
    </div>
  {/if}
{:else}
  <p class="text-text-2 text-sm">{_("misc.loading")}</p>
{/if}
