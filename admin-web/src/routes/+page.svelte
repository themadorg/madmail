<script lang="ts">
  import { store } from "$lib/state.svelte";
  import { t, getLocale } from "$lib/i18n";
  import {
    Users,
    Clock,
    HardDrive,
    Server,
    Network,
    Shield,
    Trash2,
    Mail,
    SendHorizonal,
    Inbox,
  } from "lucide-svelte";

  let locale = $state(getLocale());
  function _(key: string, params?: Record<string, string>): string {
    void locale;
    return t(key, params);
  }

  let diskPercent = $derived(store.storage?.disk?.percent_used ?? 0);
  let diskColor = $derived(
    diskPercent > 90
      ? "bg-danger"
      : diskPercent > 70
        ? "bg-warning"
        : "bg-accent",
  );
</script>

{#snippet statCard(Icon: any, label: string, value: string)}
  <div class="bg-surface-2 rounded-lg p-3 border border-border">
    <div class="flex items-center gap-1.5 text-text-2 text-xs mb-1">
      <Icon size={12} />
      {label}
    </div>
    <div class="text-xl font-semibold">{value}</div>
  </div>
{/snippet}

<!-- Stat Cards -->
<div class="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-5">
  {@render statCard(
    Users,
    _("stat.users"),
    String(store.status?.users?.registered ?? "—"),
  )}
  {@render statCard(
    Clock,
    _("stat.uptime"),
    store.status?.uptime?.duration ?? "—",
  )}
  {@render statCard(
    HardDrive,
    _("stat.disk"),
    store.storage?.disk ? `${diskPercent.toFixed(0)}%` : "—",
  )}
  {@render statCard(
    Server,
    _("stat.storage"),
    store.quota ? store.fmtBytes(store.quota.total_storage_bytes) : "—",
  )}
  {@render statCard(
    Mail,
    _("stat.sent"),
    store.status?.sent_messages != null
      ? store.status.sent_messages.toLocaleString()
      : "—",
  )}
  {@render statCard(
    SendHorizonal,
    _("stat.outbound"),
    store.status?.outbound_messages != null
      ? store.status.outbound_messages.toLocaleString()
      : "—",
  )}
  {@render statCard(
    Inbox,
    _("stat.received"),
    store.status?.received_messages != null
      ? store.status.received_messages.toLocaleString()
      : "—",
  )}
</div>

{#if store.status?.imap || store.status?.turn || store.status?.shadowsocks}
  <div class="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-5">
    {#if store.status.imap}
      {@render statCard(
        Network,
        _("stat.imap"),
        `${store.status.imap.connections} (${store.status.imap.unique_ips} IPs)`,
      )}
    {/if}
    {#if store.status.turn}
      {@render statCard(
        Server,
        _("stat.turn_relays"),
        String(store.status.turn.relays),
      )}
    {/if}
    {#if store.status.shadowsocks}
      {@render statCard(
        Shield,
        _("stat.ss_conns"),
        `${store.status.shadowsocks.connections} (${store.status.shadowsocks.unique_ips} IPs)`,
      )}
    {/if}
  </div>
{/if}

<!-- Disk Usage -->
{#if store.storage}
  <div class="bg-surface-2 rounded-lg p-4 border border-border mb-4">
    <h3 class="text-sm font-medium mb-3 flex items-center gap-1.5">
      <HardDrive size={14} class="text-text-2" />
      {_("disk.title")}
    </h3>
    <div class="h-1.5 bg-surface rounded-full overflow-hidden mb-2">
      <div
        class="h-full rounded-full transition-all duration-500 {diskColor}"
        style="width: {diskPercent}%"
      ></div>
    </div>
    <div class="flex justify-between text-xs text-text-2">
      <span
        >{store.fmtBytes(store.storage.disk.used_bytes)} {_("disk.used")}</span
      >
      <span
        >{store.fmtBytes(store.storage.disk.available_bytes)}
        {_("disk.free_of")}
        {store.fmtBytes(store.storage.disk.total_bytes)}</span
      >
    </div>
  </div>
{/if}

<!-- Email Traffic -->
{#if store.status?.email_servers}
  <div class="bg-surface-2 rounded-lg p-4 border border-border mb-4">
    <h3 class="text-sm font-medium mb-2 flex items-center gap-1.5">
      <Network size={14} class="text-text-2" />
      {_("traffic.title")}
    </h3>
    <div class="grid grid-cols-3 gap-3 text-center">
      <div>
        <div class="text-lg font-semibold">
          {store.status.email_servers.connection_ips}
        </div>
        <div class="text-text-2 text-xs">{_("traffic.connections")}</div>
      </div>
      <div>
        <div class="text-lg font-semibold">
          {store.status.email_servers.domain_servers}
        </div>
        <div class="text-text-2 text-xs">{_("traffic.domains")}</div>
      </div>
      <div>
        <div class="text-lg font-semibold">
          {store.status.email_servers.ip_servers}
        </div>
        <div class="text-text-2 text-xs">{_("traffic.ip_servers")}</div>
      </div>
    </div>
  </div>
{/if}

<!-- Queue -->
<div class="bg-surface-2 rounded-lg p-4 border border-border">
  <h3 class="text-sm font-medium mb-3 flex items-center gap-1.5">
    <Shield size={14} class="text-text-2" />
    {_("queue.title")}
  </h3>
  <div class="flex gap-2">
    <button
      onclick={() => store.purge("purge_read")}
      class="px-3 py-1.5 text-xs border border-border rounded-lg hover:bg-surface-3 text-text-2 transition-colors"
      >{_("queue.purge_read")}</button
    >
    <button
      onclick={() => store.purge("purge_all")}
      class="px-3 py-1.5 text-xs border border-danger/30 rounded-lg hover:bg-danger/10 text-danger transition-colors flex items-center gap-1"
      ><Trash2 size={11} /> {_("queue.purge_all")}</button
    >
  </div>
</div>
