CREATE TABLE federation_server_stats (
    domain TEXT PRIMARY KEY NOT NULL,
    queued_messages INTEGER NOT NULL DEFAULT 0,
    failed_http INTEGER NOT NULL DEFAULT 0,
    failed_https INTEGER NOT NULL DEFAULT 0,
    failed_smtp INTEGER NOT NULL DEFAULT 0,
    success_http INTEGER NOT NULL DEFAULT 0,
    success_https INTEGER NOT NULL DEFAULT 0,
    success_smtp INTEGER NOT NULL DEFAULT 0,
    inbound_deliveries INTEGER NOT NULL DEFAULT 0,
    successful_deliveries INTEGER NOT NULL DEFAULT 0,
    total_latency_ms INTEGER NOT NULL DEFAULT 0,
    last_active INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE federation_rules (
    domain TEXT PRIMARY KEY NOT NULL,
    action TEXT NOT NULL,
    comment TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
