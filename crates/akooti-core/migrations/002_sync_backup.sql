-- Phase 8: sync bookkeeping, backup scheduler state and run history.
--
-- Everything here stays local (rule AI-5): Supabase credentials, sync cursor and
-- backup configuration are never synced or exported. The only synced data are
-- the opaque records described in `docs/schema.md` §3.

CREATE TABLE sync_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    supabase_url TEXT NULL,
    anon_key TEXT NULL,
    user_id TEXT NULL,
    device_id TEXT NOT NULL,
    -- Access/refresh tokens, encrypted with the master key (AAD below).
    encrypted_session BLOB NULL,
    session_nonce BLOB NULL,
    -- Last `server_updated_at` received, ISO-8601 with microseconds.
    cursor TEXT NULL,
    last_sync_at INTEGER NULL,
    last_status TEXT NULL,
    last_error TEXT NULL
);

INSERT INTO sync_config (id, device_id)
VALUES (1, lower(hex(randomblob(16))));

CREATE TABLE backup_runs (
    id TEXT PRIMARY KEY,
    target_id TEXT NOT NULL REFERENCES backup_targets(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NULL,
    status TEXT NOT NULL,
    detail TEXT NULL,
    size_bytes INTEGER NULL,
    snapshot_name TEXT NULL
);

CREATE INDEX idx_backup_runs_started ON backup_runs(started_at DESC);
