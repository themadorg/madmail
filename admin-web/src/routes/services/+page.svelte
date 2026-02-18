<script lang="ts">
  import { store } from "$lib/state.svelte";
  import { t, getLocale } from "$lib/i18n";
  import {
    ToggleLeft,
    ToggleRight,
    Pencil,
    RotateCcw,
    Dice5,
  } from "lucide-svelte";

  function randomPath(): string {
    const chars = "abcdefghijklmnopqrstuvwxyz0123456789";
    let s = "/";
    for (let i = 0; i < 16; i++)
      s += chars[Math.floor(Math.random() * chars.length)];
    return s;
  }

  let locale = $state(getLocale());
  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }

  let isWorking = $derived(store.refreshing || store.busy || store.reloading);
</script>

{#if store.settings}
  <!-- Toggle Services -->
  <div class="space-y-2">
    {#snippet toggleRow(
      label: string,
      resource: string,
      current: string,
      onLabel: string,
    )}
      <div
        class="flex items-center justify-between bg-surface-2 rounded-lg p-3 border border-border"
      >
        <div>
          <div class="text-sm font-medium">{label}</div>
          <div class="text-xs text-text-2 font-mono">{resource}</div>
        </div>
        <button
          onclick={() => store.toggleService(resource, current)}
          disabled={isWorking}
          class="flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-full transition-colors disabled:opacity-50
            {current === onLabel
            ? 'bg-success/15 text-success border border-success/30'
            : 'bg-surface-3 text-text-2 border border-border hover:border-text-2/50'}"
        >
          {#if current === onLabel}
            <ToggleRight size={14} />
          {:else}
            <ToggleLeft size={14} />
          {/if}
          {current}
        </button>
      </div>
    {/snippet}

    {@render toggleRow(
      _("svc.registration"),
      "/admin/registration",
      store.settings.registration,
      "open",
    )}
    {@render toggleRow(
      _("svc.jit_registration"),
      "/admin/registration/jit",
      store.settings.jit_registration,
      "enabled",
    )}
    {@render toggleRow(
      _("svc.turn"),
      "/admin/services/turn",
      store.settings.turn_enabled,
      "enabled",
    )}
    {@render toggleRow(
      _("svc.iroh"),
      "/admin/services/iroh",
      store.settings.iroh_enabled,
      "enabled",
    )}
    {@render toggleRow(
      _("svc.shadowsocks"),
      "/admin/services/shadowsocks",
      store.settings.ss_enabled,
      "enabled",
    )}
  </div>

  <!-- Configuration -->
  <h3 class="text-sm font-medium mt-6 mb-3 text-text-2">
    {_("svc.configuration")}
  </h3>
  <div class="space-y-2">
    {#snippet editableRow(
      key: string,
      label: string,
      fallback: string,
      inputType: string,
    )}
      <div
        class="flex items-center justify-between bg-surface-2 rounded-lg p-3 border border-border gap-3"
      >
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium">{label}</div>
          {#if store.editingField === key}
            <div class="flex gap-2 mt-1.5">
              {#if inputType === "number"}
                <input
                  type="number"
                  bind:value={store.editValue}
                  min="1"
                  max="65535"
                  class="w-28 px-2 py-1 bg-surface border border-border rounded text-xs text-text outline-none focus:border-accent"
                />
              {:else}
                <input
                  type="text"
                  bind:value={store.editValue}
                  class="flex-1 px-2 py-1 bg-surface border border-border rounded text-xs text-text outline-none focus:border-accent"
                />
                {#if key === "admin_path"}
                  <button
                    onclick={() => (store.editValue = randomPath())}
                    class="p-1.5 text-text-2 border border-border rounded hover:bg-surface-3 transition-colors"
                    title="Generate random path"
                  >
                    <Dice5 size={12} />
                  </button>
                {/if}
              {/if}
              <button
                onclick={() => store.save(key, store.editValue)}
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

    {@render editableRow("smtp_hostname", _("svc.smtp_hostname"), "", "text")}
    {@render editableRow("turn_realm", _("svc.turn_realm"), "", "text")}
    {@render editableRow("turn_secret", _("svc.turn_secret"), "", "text")}
    {@render editableRow("turn_relay_ip", _("svc.turn_relay_ip"), "", "text")}
    {@render editableRow("turn_ttl", _("svc.turn_ttl"), "", "number")}
    {@render editableRow("iroh_relay_url", _("svc.iroh_relay_url"), "", "text")}
    {@render editableRow("ss_cipher", _("svc.ss_cipher"), "", "text")}
    {@render editableRow("ss_password", _("svc.ss_password"), "", "text")}
    {@render editableRow(
      "admin_path",
      _("svc.admin_path"),
      "/api/admin",
      "text",
    )}
  </div>
{:else}
  <p class="text-text-2 text-sm">{_("misc.loading")}</p>
{/if}
