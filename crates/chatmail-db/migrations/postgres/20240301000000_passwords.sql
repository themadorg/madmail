-- Skip when Madmail `auth.pass_table` already created `passwords` (`key`/`value`).
CREATE TABLE IF NOT EXISTS passwords (
    username TEXT PRIMARY KEY NOT NULL,
    hash TEXT NOT NULL,
    created_at BIGINT NOT NULL DEFAULT (FLOOR(EXTRACT(EPOCH FROM NOW())))
);

CREATE TABLE IF NOT EXISTS push_tokens (
    username TEXT NOT NULL,
    device_token TEXT NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (username, device_token)
);
