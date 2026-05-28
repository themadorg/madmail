-- Outbound federation domains whose mail is accepted but not delivered.
CREATE TABLE IF NOT EXISTS federation_silent_dismiss (
    domain TEXT PRIMARY KEY NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
