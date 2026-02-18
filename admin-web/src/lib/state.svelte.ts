// Shared reactive state for the admin dashboard.
// Uses a singleton object with $state fields so it can be exported from a module.

import {
    api,
    type ApiConfig,
    type StatusResponse,
    type StorageResponse,
    type AllSettings,
    type AccountList,
    type QuotaStats,
    type SettingValue,
    type BlocklistResponse,
    type DnsListResponse,
    type CreateAccountResponse,
} from '$lib/api';
import { t } from '$lib/i18n';

// --- Initialization ---
let savedUrl = '';
let savedToken = '';
if (typeof localStorage !== 'undefined') {
    savedUrl = localStorage.getItem('madmail_url') ?? '';
    savedToken = localStorage.getItem('madmail_token') ?? '';
    try {
        if (savedUrl && !new URL(savedUrl).pathname.replace(/\/+$/, '')) {
            savedUrl = savedUrl.replace(/\/+$/, '') + '/api/admin';
            localStorage.setItem('madmail_url', savedUrl);
        }
    } catch { /* invalid URL */ }
}

// --- Singleton reactive store ---
class AdminState {
    // Connection
    baseUrl = $state(savedUrl);
    token = $state(savedToken);
    connected = $state(false);
    connectError = $state('');
    connecting = $state(false);

    // Data
    status = $state<StatusResponse | null>(null);
    storage = $state<StorageResponse | null>(null);
    settings = $state<AllSettings | null>(null);
    accounts = $state<AccountList | null>(null);
    quota = $state<QuotaStats | null>(null);
    blocklist = $state<BlocklistResponse | null>(null);
    dnsOverrides = $state<DnsListResponse | null>(null);

    // UI
    toast = $state('');
    toastType = $state<'ok' | 'err'>('ok');
    pendingRestart = $state(false);
    reloading = $state(false);
    editingField = $state('');
    editValue = $state('');
    refreshing = $state(false);
    busy = $state(false);
    confirmingDelete = $state('');
    newAccount = $state<CreateAccountResponse | null>(null);

    // --- Helpers ---
    cfg(): ApiConfig {
        return { baseUrl: this.baseUrl, token: this.token };
    }

    notify(msg: string, type: 'ok' | 'err' = 'ok') {
        this.toast = msg;
        this.toastType = type;
        setTimeout(() => { this.toast = ''; }, 3000);
    }

    setting(key: string): SettingValue {
        if (!this.settings) return { key, value: '', is_set: false };
        return (this.settings as Record<string, any>)[key] ?? { key, value: '', is_set: false };
    }

    fmtBytes(b: number): string {
        if (!b || isNaN(b)) return '0 B';
        if (b < 1024) return `${b} B`;
        if (b < 1048576) return `${(b / 1024).toFixed(1)} KB`;
        if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
        return `${(b / 1073741824).toFixed(1)} GB`;
    }

    // --- API actions ---
    async connect() {
        if (!this.baseUrl || !this.token || this.connecting) return;
        this.connecting = true;
        this.connectError = '';
        const res = await api.status(this.cfg());
        this.connecting = false;
        if (res.error) { this.connectError = res.error; return; }
        localStorage.setItem('madmail_url', this.baseUrl);
        localStorage.setItem('madmail_token', this.token);
        this.connected = true;
        this.status = res.data!;
        this.refresh();
    }

    async refresh() {
        if (this.refreshing) return;
        this.refreshing = true;
        try {
            const [a, b, c, d, e, f, g] = await Promise.all([
                api.storage(this.cfg()),
                api.settings(this.cfg()),
                api.accounts(this.cfg()),
                api.quota(this.cfg()),
                api.status(this.cfg()),
                api.blocklist(this.cfg()),
                api.dns(this.cfg()),
            ]);
            if (a.data) this.storage = a.data;
            if (b.data) this.settings = b.data;
            if (c.data) this.accounts = c.data;
            if (d.data) this.quota = d.data;
            if (e.data) this.status = e.data;
            if (f.data) this.blocklist = f.data;
            if (g.data) this.dnsOverrides = g.data;
        } finally { this.refreshing = false; }
    }

    disconnect() {
        this.connected = false;
        localStorage.removeItem('madmail_url');
        localStorage.removeItem('madmail_token');
        this.baseUrl = '';
        this.token = '';
        this.status = this.storage = this.settings = this.accounts = this.quota = this.blocklist = this.dnsOverrides = null;
        this.newAccount = null;
    }

    async toggleService(resource: string, current: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const isOn = current === 'enabled' || current === 'open';
            const action = isOn
                ? resource === '/admin/registration' ? 'close' : 'disable'
                : resource === '/admin/registration' ? 'open' : 'enable';
            const res = await api.setToggle(this.cfg(), resource, action);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(`${resource.split('/').pop()} → ${res.data?.status}`);
            await this.refresh();
        } finally { this.busy = false; }
    }

    async save(key: string, value: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.setSetting(this.cfg(), key, value);
            if (res.error) { this.notify(res.error, 'err'); return; }
            if (res.data?.restart_required) this.pendingRestart = true;
            this.notify(t('notify.updated', { key }));
            this.editingField = '';
            // If admin_path changed, reload immediately with the new URL
            if (key === 'admin_path' && value) {
                // Send reload on the OLD url (still active until restart)
                await api.reload(this.cfg());
                // Now update our baseUrl to the new path
                const url = new URL(this.baseUrl);
                url.pathname = value;
                this.baseUrl = url.toString().replace(/\/+$/, '');
                localStorage.setItem('madmail_url', this.baseUrl);
                this.pendingRestart = false;
                this.notify(t('action.restarting'));
                // Wait for service to come back on the new path
                setTimeout(async () => {
                    for (let i = 0; i < 10; i++) {
                        await new Promise(r => setTimeout(r, 2000));
                        if ((await api.status(this.cfg())).data) {
                            this.notify(t('notify.online'));
                            await this.refresh();
                            return;
                        }
                    }
                    this.notify(t('notify.restart_pending'), 'err');
                }, 1000);
                return;
            }
            await this.refresh();
        } finally { this.busy = false; }
    }

    async reset(key: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.resetSetting(this.cfg(), key);
            if (res.error) { this.notify(res.error, 'err'); return; }
            if (res.data?.restart_required) this.pendingRestart = true;
            this.notify(t('notify.reset', { key }));
            // If admin_path reset, reload immediately with the default URL
            if (key === 'admin_path') {
                await api.reload(this.cfg());
                const url = new URL(this.baseUrl);
                url.pathname = '/api/admin';
                this.baseUrl = url.toString().replace(/\/+$/, '');
                localStorage.setItem('madmail_url', this.baseUrl);
                this.pendingRestart = false;
                this.notify(t('action.restarting'));
                setTimeout(async () => {
                    for (let i = 0; i < 10; i++) {
                        await new Promise(r => setTimeout(r, 2000));
                        if ((await api.status(this.cfg())).data) {
                            this.notify(t('notify.online'));
                            await this.refresh();
                            return;
                        }
                    }
                    this.notify(t('notify.restart_pending'), 'err');
                }, 1000);
                return;
            }
            await this.refresh();
        } finally { this.busy = false; }
    }

    async reload() {
        this.reloading = true;
        const res = await api.reload(this.cfg());
        this.reloading = false;
        if (res.error) { this.notify(res.error, 'err'); return; }
        this.pendingRestart = false;
        this.notify(t('action.restarting'));
        setTimeout(async () => {
            for (let i = 0; i < 10; i++) {
                await new Promise(r => setTimeout(r, 2000));
                if ((await api.status(this.cfg())).data) {
                    this.notify(t('notify.online'));
                    await this.refresh();
                    return;
                }
            }
            this.notify(t('notify.restart_pending'), 'err');
        }, 1000);
    }

    async togglePortAccess(portKey: string, currentAccess: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const settingKey = portKey + '_local_only';
            if (currentAccess === 'public') {
                // Make local-only
                await api.setSetting(this.cfg(), settingKey, 'true');
            } else {
                // Make public
                await api.resetSetting(this.cfg(), settingKey);
            }
            this.pendingRestart = true;
            await this.refresh();
        } finally { this.busy = false; }
    }

    requestDelete(u: string) {
        this.confirmingDelete = this.confirmingDelete === u ? '' : u;
    }

    cancelDelete() {
        this.confirmingDelete = '';
    }

    async confirmDelete(u: string) {
        if (this.busy) return;
        this.busy = true;
        this.confirmingDelete = '';
        try {
            const res = await api.deleteAccount(this.cfg(), u);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.deleted', { username: u }));
            await this.refresh();
        } finally { this.busy = false; }
    }

    async createAccount() {
        if (this.busy) return;
        this.busy = true;
        this.newAccount = null;
        try {
            const res = await api.createAccount(this.cfg());
            if (res.error) { this.notify(res.error, 'err'); return; }
            if (res.data) {
                this.newAccount = res.data;
                this.notify(t('notify.account_created', { email: res.data.email }));
                await this.refresh();
            }
        } finally { this.busy = false; }
    }

    dismissNewAccount() {
        this.newAccount = null;
    }

    async setDefaultQuota(maxBytes: number) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.setDefaultQuota(this.cfg(), maxBytes);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.quota_updated'));
            await this.refresh();
        } finally { this.busy = false; }
    }

    async setUserQuota(username: string, maxBytes: number) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.setUserQuota(this.cfg(), username, maxBytes);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.quota_updated'));
            await this.refresh();
        } finally { this.busy = false; }
    }

    async resetUserQuota(username: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.resetUserQuota(this.cfg(), username);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.quota_reset'));
            await this.refresh();
        } finally { this.busy = false; }
    }

    async unblockUser(u: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.unblockUser(this.cfg(), u);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.unblocked', { username: u }));
            await this.refresh();
        } finally { this.busy = false; }
    }

    async purge(action: string) {
        if (this.busy) return;
        if (!confirm(`${action}?`)) return;
        this.busy = true;
        try {
            const res = await api.purgeQueue(this.cfg(), action);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.purge_done', { action }));
        } finally { this.busy = false; }
    }

    startEdit(key: string, current: string) {
        this.editingField = key;
        this.editValue = current;
    }

    async addDnsOverride(lookupKey: string, targetHost: string, comment?: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.addDns(this.cfg(), lookupKey, targetHost, comment);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.dns_added', { key: lookupKey }));
            await this.refresh();
        } finally { this.busy = false; }
    }

    async deleteDnsOverride(lookupKey: string) {
        if (this.busy) return;
        this.busy = true;
        try {
            const res = await api.deleteDns(this.cfg(), lookupKey);
            if (res.error) { this.notify(res.error, 'err'); return; }
            this.notify(t('notify.dns_deleted', { key: lookupKey }));
            await this.refresh();
        } finally { this.busy = false; }
    }
}

export const store = new AdminState();
