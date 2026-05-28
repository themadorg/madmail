CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE quotas (
    username TEXT PRIMARY KEY NOT NULL,
    max_storage BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    first_login_at BIGINT NOT NULL,
    last_login_at BIGINT NOT NULL,
    used_token TEXT
);

CREATE TABLE blocked_users (
    username TEXT PRIMARY KEY NOT NULL,
    reason TEXT NOT NULL,
    blocked_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE registration_tokens (
    token TEXT PRIMARY KEY NOT NULL,
    max_uses INTEGER NOT NULL DEFAULT 1,
    used_count INTEGER NOT NULL DEFAULT 0,
    comment TEXT,
    expires_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE dns_overrides (
    lookup_key TEXT PRIMARY KEY NOT NULL,
    target_host TEXT NOT NULL,
    comment TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
