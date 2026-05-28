-- Align federation_rules with Madmail GORM model (id, domain, created_at unix).
CREATE TABLE IF NOT EXISTS federation_rules_madmail (
    id SERIAL PRIMARY KEY,
    domain TEXT NOT NULL UNIQUE,
    created_at BIGINT NOT NULL DEFAULT (FLOOR(EXTRACT(EPOCH FROM NOW())))
);

INSERT INTO federation_rules_madmail (domain, created_at)
SELECT domain, COALESCE(
    EXTRACT(EPOCH FROM created_at::timestamptz)::BIGINT,
    FLOOR(EXTRACT(EPOCH FROM NOW()))
)
FROM federation_rules
ON CONFLICT (domain) DO NOTHING;

DROP TABLE IF EXISTS federation_rules;
ALTER TABLE federation_rules_madmail RENAME TO federation_rules;

-- Madmail message counters (admin dashboard).
CREATE TABLE IF NOT EXISTS message_stats (
    name TEXT PRIMARY KEY NOT NULL,
    count BIGINT NOT NULL DEFAULT 0
);

INSERT INTO message_stats (name, count) VALUES
    ('sent_messages', 0),
    ('outbound_messages', 0),
    ('received_messages', 0)
ON CONFLICT (name) DO NOTHING;

-- Madmail pull-based exchangers (optional federation ingress).
CREATE TABLE IF NOT EXISTS exchangers (
    name TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    poll_interval INTEGER NOT NULL DEFAULT 60,
    last_poll_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
