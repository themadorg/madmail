// Admin API client for the Madmail RPC-style single-endpoint API.
// All requests are POST to the admin API URL provided by the user.

export interface ApiConfig {
    baseUrl: string;
    token: string;
}

export interface ApiResponse<T = unknown> {
    status: number;
    resource: string;
    body: T;
    error: string | null;
}

// --- Response types ---

export interface StatusResponse {
    imap?: { connections: number; unique_ips: number };
    turn?: { relays: number };
    shadowsocks?: { connections: number; unique_ips: number };
    users: { registered: number };
    uptime: { boot_time: string; duration: string };
    email_servers?: { connection_ips: number; domain_servers: number; ip_servers: number };
    sent_messages: number;
    outbound_messages: number;
}

export interface StorageResponse {
    disk: { total_bytes: number; used_bytes: number; available_bytes: number; percent_used: number };
    state_dir: { path: string; size_bytes: number };
    database?: { driver: string; size_bytes: number };
}

export interface ToggleStatus {
    status: string;
}

export interface SettingValue {
    key: string;
    value: string;
    is_set: boolean;
    restart_required?: boolean;
}

export interface AllSettings {
    registration: string;
    jit_registration: string;
    turn_enabled: string;
    iroh_enabled: string;
    ss_enabled: string;
    log_disabled: string;
    smtp_port: SettingValue;
    submission_port: SettingValue;
    imap_port: SettingValue;
    turn_port: SettingValue;
    sasl_port: SettingValue;
    iroh_port: SettingValue;
    ss_port: SettingValue;
    http_port: SettingValue;
    https_port: SettingValue;
    // Per-port access control: "public" or "local"
    smtp_access: string;
    submission_access: string;
    imap_access: string;
    turn_access: string;
    iroh_access: string;
    http_access: string;
    https_access: string;
    // Configuration
    smtp_hostname: SettingValue;
    turn_realm: SettingValue;
    turn_secret: SettingValue;
    turn_relay_ip: SettingValue;
    turn_ttl: SettingValue;
    iroh_relay_url: SettingValue;
    ss_cipher: SettingValue;
    ss_password: SettingValue;
    admin_path: SettingValue;
}

export interface AccountList {
    accounts: { username: string; used_bytes: number; max_bytes: number; is_default_quota: boolean; created_at: number; first_login_at: number; last_login_at: number }[];
    total: number;
}

export interface BlocklistEntry {
    username: string;
    reason: string;
    blocked_at: string;
}

export interface BlocklistResponse {
    blocked: BlocklistEntry[];
    total: number;
}

export interface QuotaStats {
    total_storage_bytes: number;
    accounts_count: number;
    default_quota_bytes: number;
}

export interface DnsEntry {
    lookup_key: string;
    target_host: string;
    comment?: string;
}

export interface DnsListResponse {
    overrides: DnsEntry[];
    total: number;
}

export interface ReloadResponse {
    status: string;
    message: string;
}

export interface CreateAccountResponse {
    email: string;
    password: string;
}

// --- API Client ---

export async function apiCall<T = unknown>(
    config: ApiConfig,
    resource: string,
    method: string = 'GET',
    body?: unknown
): Promise<{ data?: T; error?: string; status: number }> {
    try {
        const url = config.baseUrl.replace(/\/+$/, '');
        const res = await fetch(url, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                method,
                resource,
                headers: { Authorization: `Bearer ${config.token}` },
                body: body ?? {}
            })
        });

        const json: ApiResponse<T> = await res.json();

        if (json.error) {
            return { error: json.error, status: json.status };
        }
        return { data: json.body, status: json.status };
    } catch (e) {
        return { error: String(e), status: 0 };
    }
}

// Convenience wrappers
export const api = {
    status: (c: ApiConfig) => apiCall<StatusResponse>(c, '/admin/status'),
    storage: (c: ApiConfig) => apiCall<StorageResponse>(c, '/admin/storage'),
    accounts: (c: ApiConfig) => apiCall<AccountList>(c, '/admin/accounts'),
    createAccount: (c: ApiConfig) =>
        apiCall<CreateAccountResponse>(c, '/admin/accounts', 'POST'),
    deleteAccount: (c: ApiConfig, username: string) =>
        apiCall(c, '/admin/accounts', 'DELETE', { username }),
    quota: (c: ApiConfig) => apiCall<QuotaStats>(c, '/admin/quota'),
    setDefaultQuota: (c: ApiConfig, maxBytes: number) =>
        apiCall(c, '/admin/quota', 'PUT', { max_bytes: maxBytes }),
    setUserQuota: (c: ApiConfig, username: string, maxBytes: number) =>
        apiCall(c, '/admin/quota', 'PUT', { username, max_bytes: maxBytes }),
    resetUserQuota: (c: ApiConfig, username: string) =>
        apiCall(c, '/admin/quota', 'DELETE', { username }),
    settings: (c: ApiConfig) => apiCall<AllSettings>(c, '/admin/settings'),

    // Blocklist
    blocklist: (c: ApiConfig) => apiCall<BlocklistResponse>(c, '/admin/blocklist'),
    blockUser: (c: ApiConfig, username: string, reason?: string) =>
        apiCall(c, '/admin/blocklist', 'POST', { username, reason: reason || 'manually blocked' }),
    unblockUser: (c: ApiConfig, username: string) =>
        apiCall(c, '/admin/blocklist', 'DELETE', { username }),

    // Toggles
    getToggle: (c: ApiConfig, resource: string) =>
        apiCall<ToggleStatus>(c, resource),
    setToggle: (c: ApiConfig, resource: string, action: string) =>
        apiCall<ToggleStatus>(c, resource, 'POST', { action }),

    // Settings
    getSetting: (c: ApiConfig, key: string) =>
        apiCall<SettingValue>(c, `/admin/settings/${key}`),
    setSetting: (c: ApiConfig, key: string, value: string) =>
        apiCall<SettingValue>(c, `/admin/settings/${key}`, 'POST', { action: 'set', value }),
    resetSetting: (c: ApiConfig, key: string) =>
        apiCall<SettingValue>(c, `/admin/settings/${key}`, 'POST', { action: 'reset' }),

    // Queue
    purgeQueue: (c: ApiConfig, action: string) =>
        apiCall(c, '/admin/queue', 'POST', { action }),

    // DNS overrides
    dns: (c: ApiConfig) => apiCall<DnsListResponse>(c, '/admin/dns'),
    addDns: (c: ApiConfig, lookup_key: string, target_host: string, comment?: string) =>
        apiCall<DnsEntry>(c, '/admin/dns', 'POST', { lookup_key, target_host, comment: comment || '' }),
    deleteDns: (c: ApiConfig, lookup_key: string) =>
        apiCall(c, '/admin/dns', 'DELETE', { lookup_key }),

    // Reload
    reload: (c: ApiConfig) => apiCall<ReloadResponse>(c, '/admin/reload', 'POST'),

    // Restart service (needed after port access changes)
    restart: (c: ApiConfig) => apiCall(c, '/admin/restart', 'POST'),
};

