CREATE TABLE vault_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    vault_id TEXT NOT NULL UNIQUE,
    kdf_algorithm TEXT NOT NULL DEFAULT 'argon2id',
    kdf_salt BLOB NOT NULL,
    kdf_memory_kib INTEGER NOT NULL DEFAULT 65536,
    kdf_iterations INTEGER NOT NULL DEFAULT 3,
    kdf_parallelism INTEGER NOT NULL DEFAULT 4,
    wrapped_master_key BLOB NOT NULL,
    wrapped_key_nonce BLOB NOT NULL,
    biometrics_enabled INTEGER NOT NULL DEFAULT 0,
    auto_lock_minutes INTEGER NOT NULL DEFAULT 15,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE folders (
    id TEXT PRIMARY KEY,
    parent_id TEXT NULL REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NULL UNIQUE,
    icon TEXT NOT NULL DEFAULT 'folder',
    color TEXT NULL,
    is_system INTEGER NOT NULL DEFAULT 0,
    require_biometric_for_mcp INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER NULL
);

INSERT INTO folders (
    id, parent_id, name, slug, icon, color, is_system, created_at, updated_at
)
VALUES
    ('00000000-0000-4000-8000-000000000001', NULL, 'Personnel', 'personal', 'user', '#3b82f6', 1, strftime('%s','now'), strftime('%s','now')),
    ('00000000-0000-4000-8000-000000000002', NULL, 'Dev Local', 'dev-local', 'terminal', '#10b981', 1, strftime('%s','now'), strftime('%s','now')),
    ('00000000-0000-4000-8000-000000000003', NULL, 'Agents & MCP', 'agents-mcp', 'bot', '#8b5cf6', 1, strftime('%s','now'), strftime('%s','now'));

CREATE TABLE vault_items (
    id TEXT PRIMARY KEY,
    item_type TEXT NOT NULL DEFAULT 'login',
    title TEXT NOT NULL,
    primary_url TEXT NULL,
    username TEXT NULL,
    encrypted_password BLOB NULL,
    password_nonce BLOB NULL,
    encrypted_totp_secret BLOB NULL,
    totp_nonce BLOB NULL,
    encrypted_notes BLOB NULL,
    notes_nonce BLOB NULL,
    username_selector TEXT NULL,
    password_selector TEXT NULL,
    totp_selector TEXT NULL,
    submit_selector TEXT NULL,
    zxcvbn_score INTEGER NULL,
    password_sha1_prefix TEXT NULL,
    is_compromised INTEGER NOT NULL DEFAULT 0,
    password_updated_at INTEGER NULL,
    created_by_agent TEXT NULL,
    expires_at INTEGER NULL,
    favorite INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER NULL
);

CREATE INDEX idx_items_primary_url
    ON vault_items(primary_url) WHERE deleted_at IS NULL;
CREATE INDEX idx_items_updated_at ON vault_items(updated_at);

CREATE TABLE item_folders (
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (item_id, folder_id)
);

CREATE INDEX idx_item_folders_folder ON item_folders(folder_id);

CREATE TABLE item_uris (
    id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    uri TEXT NOT NULL,
    match_type TEXT NOT NULL DEFAULT 'domain'
);

CREATE TABLE item_custom_fields (
    id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,
    is_secret INTEGER NOT NULL DEFAULT 1,
    encrypted_value BLOB NOT NULL,
    value_nonce BLOB NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE password_history (
    id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    encrypted_password BLOB NOT NULL,
    password_nonce BLOB NOT NULL,
    changed_at INTEGER NOT NULL
);

CREATE TABLE passkeys (
    id TEXT PRIMARY KEY,
    item_id TEXT NULL REFERENCES vault_items(id) ON DELETE SET NULL,
    credential_id BLOB NOT NULL UNIQUE,
    rp_id TEXT NOT NULL,
    rp_name TEXT NOT NULL,
    user_handle BLOB NOT NULL,
    user_name TEXT NOT NULL,
    encrypted_private_key BLOB NOT NULL,
    private_key_nonce BLOB NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NULL,
    deleted_at INTEGER NULL
);

CREATE INDEX idx_passkeys_rp_id ON passkeys(rp_id) WHERE deleted_at IS NULL;

CREATE TABLE local_routing_rules (
    id TEXT PRIMARY KEY,
    pattern TEXT NOT NULL UNIQUE,
    pattern_type TEXT NOT NULL DEFAULT 'suffix',
    target_folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    is_default INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

INSERT INTO local_routing_rules (
    id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at
)
VALUES
    ('10000000-0000-4000-8000-000000000001', '.local', 'suffix', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000002', '.test', 'suffix', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000003', '.localhost', 'suffix', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000004', 'localhost', 'exact_host', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000005', '127.0.0.1', 'exact_host', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now'));

CREATE TABLE mcp_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    scopes_json TEXT NOT NULL,
    default_create_folder_id TEXT NULL REFERENCES folders(id) ON DELETE SET NULL,
    expires_at INTEGER NULL,
    revoked_at INTEGER NULL,
    last_used_at INTEGER NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE mcp_token_folders (
    token_id TEXT NOT NULL REFERENCES mcp_tokens(id) ON DELETE CASCADE,
    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    PRIMARY KEY (token_id, folder_id)
);

CREATE TABLE mcp_audit_logs (
    id TEXT PRIMARY KEY,
    token_id TEXT NULL REFERENCES mcp_tokens(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_item_id TEXT NULL,
    target_folder_id TEXT NULL,
    target_origin TEXT NULL,
    status TEXT NOT NULL,
    details TEXT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_audit_created_at ON mcp_audit_logs(created_at DESC);

CREATE TABLE backup_targets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    encrypted_config BLOB NOT NULL,
    config_nonce BLOB NOT NULL,
    interval_minutes INTEGER NOT NULL DEFAULT 60,
    retention_count INTEGER NOT NULL DEFAULT 24,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at INTEGER NULL,
    last_status TEXT NULL,
    created_at INTEGER NOT NULL
);
