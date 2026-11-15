Voici les deux schémas SQL complets et prêts pour la production pour **Akooti** :

1. **Le schéma SQLCipher local (`schema_local.sql`) :** Base relationnelle chiffrée au niveau fichier (AES-256 via SQLCipher) + double chiffrement applicatif (`XChaCha20-Poly1305`) sur les champs les plus critiques en RAM. Il intègre les **dossiers système par défaut** (`Personnel`, `Dev Local`, `Agents & MCP`), la **relation N:N multi-dossiers (`item_folders`)**, les **sélecteurs CSS/JS mémorisés**, l'**historique des mots de passe**, les **jetons MCP cloisonnés** et les **règles de routage local**.
2. **Le schéma Supabase (`schema_supabase.sql`) :** Architecture **100 % Zero-Knowledge (E2EE)**. Supabase ne stocke jamais de texte en clair (ni titres, ni URLs, ni noms de dossiers) : chaque enregistrement est un **blob binaire/base64 chiffré côté client** accompagné uniquement des métadonnées techniques nécessaires à la synchronisation temps réel (*Supabase Realtime*), à la résolution de conflits (*Last-Write-Wins*) et aux suppressions logiques (*tombstones*).

L'amorçage d'un coffre utilise `vault.meta` (sel et paramètres Argon2id) et
`vault.db` (SQLCipher). Les paramètres KDF sont dupliqués dans `vault_meta` après
ouverture et vérifiés contre l'en-tête public ; voir
[ADR 0004](adr/0004-vault-bootstrap-header.md).

---

### 1. Schéma SQLCipher Local (`crates/akooti-core/migrations/001_init.sql`)

```sql
-- ============================================================================
-- AKOOTI - SCHÉMA SQLCIPHER LOCAL (v1.0)
-- ============================================================================
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

-- ----------------------------------------------------------------------------
-- 1. MÉTADONNÉES DU COFFRE & CONFIGURATION CRYPTOGRAPHIQUE
-- ----------------------------------------------------------------------------
CREATE TABLE vault_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    vault_id TEXT NOT NULL UNIQUE,               -- UUIDv4 du coffre (partagé avec Supabase)
    kdf_algorithm TEXT NOT NULL DEFAULT 'argon2id',
    kdf_salt BLOB NOT NULL,                      -- Sel 16 octets pour Argon2id
    kdf_memory_kib INTEGER NOT NULL DEFAULT 65536,
    kdf_iterations INTEGER NOT NULL DEFAULT 3,
    kdf_parallelism INTEGER NOT NULL DEFAULT 4,
    wrapped_master_key BLOB NOT NULL,            -- Clé maître chiffrée par la KEK dérivée
    wrapped_key_nonce BLOB NOT NULL,             -- Nonce 24 octets (XChaCha20)
    biometrics_enabled INTEGER NOT NULL DEFAULT 0,
    auto_lock_minutes INTEGER NOT NULL DEFAULT 15,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- ----------------------------------------------------------------------------
-- 2. DOSSIERS (Système & Utilisateur)
-- ----------------------------------------------------------------------------
CREATE TABLE folders (
    id TEXT PRIMARY KEY,                         -- UUIDv4
    parent_id TEXT NULL REFERENCES folders(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NULL UNIQUE,                       -- Identifiant stable pour dossiers système
    icon TEXT NOT NULL DEFAULT 'folder',
    color TEXT NULL,
    is_system INTEGER NOT NULL DEFAULT 0,        -- 1 = non supprimable/renommable
    require_biometric_for_mcp INTEGER NOT NULL DEFAULT 0, -- Garde-fou supplémentaire
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER NULL                      -- Soft-delete (tombstone pour sync)
);

-- Insertion des 3 dossiers système par défaut à l'initialisation
INSERT INTO folders (id, parent_id, name, slug, icon, color, is_system, created_at, updated_at)
VALUES
    ('00000000-0000-4000-8000-000000000001', NULL, 'Personnel',    'personal',  'user',     '#3b82f6', 1, strftime('%s','now'), strftime('%s','now')),
    ('00000000-0000-4000-8000-000000000002', NULL, 'Dev Local',    'dev-local', 'terminal', '#10b981', 1, strftime('%s','now'), strftime('%s','now')),
    ('00000000-0000-4000-8000-000000000003', NULL, 'Agents & MCP', 'agents-mcp','bot',      '#8b5cf6', 1, strftime('%s','now'), strftime('%s','now'));

-- ----------------------------------------------------------------------------
-- 3. ENTRÉES DU COFFRE (Identifiants, Clés API, Notes, Passkeys)
-- ----------------------------------------------------------------------------
CREATE TABLE vault_items (
    id TEXT PRIMARY KEY,                         -- UUIDv4
    item_type TEXT NOT NULL DEFAULT 'login',     -- 'login' | 'api_key' | 'note' | 'env_set'
    title TEXT NOT NULL,
    primary_url TEXT NULL,                       -- URL principale pour matching rapide
    username TEXT NULL,
    
    -- Secrets chiffrés individuellement en RAM avec XChaCha20-Poly1305
    encrypted_password BLOB NULL,
    password_nonce BLOB NULL,
    encrypted_totp_secret BLOB NULL,
    totp_nonce BLOB NULL,
    encrypted_notes BLOB NULL,
    notes_nonce BLOB NULL,
    
    -- Sélecteurs DOM mémorisés pour l'injection aveugle (Extension WXT & CDP)
    username_selector TEXT NULL,                 -- ex: "input[name='email']"
    password_selector TEXT NULL,                 -- ex: "#password-input"
    totp_selector TEXT NULL,                     -- ex: "input[autocomplete='one-time-code']"
    submit_selector TEXT NULL,                   -- ex: "button[type='submit']"
    
    -- Métadonnées d'audit & hygiène (calculées localement)
    zxcvbn_score INTEGER NULL,                   -- Score 0 à 4
    password_sha1_prefix TEXT NULL,              -- 5 premiers chars SHA-1 (check HIBP)
    is_compromised INTEGER NOT NULL DEFAULT 0,
    password_updated_at INTEGER NULL,
    
    -- Traçabilité IA & Éphémérité (comptes de test créés par MCP/CLI)
    created_by_agent TEXT NULL,                  -- Nom du token MCP créateur (si applicable)
    expires_at INTEGER NULL,                     -- TTL optionnel pour purge auto des tests
    
    favorite INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER NULL                      -- Soft-delete pour synchronisation
);

CREATE INDEX idx_items_primary_url ON vault_items(primary_url) WHERE deleted_at IS NULL;
CREATE INDEX idx_items_updated_at ON vault_items(updated_at);

-- ----------------------------------------------------------------------------
-- 4. RELATION MULTI-DOSSIERS (Many-to-Many : 1 entrée -> N dossiers)
-- ----------------------------------------------------------------------------
CREATE TABLE item_folders (
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (item_id, folder_id)
);

CREATE INDEX idx_item_folders_folder ON item_folders(folder_id);

-- ----------------------------------------------------------------------------
-- 5. URIs MULTIPLES, CHAMPS PERSONNALISÉS & HISTORIQUE DES MOTS DE PASSE
-- ----------------------------------------------------------------------------
CREATE TABLE item_uris (
    id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    uri TEXT NOT NULL,
    match_type TEXT NOT NULL DEFAULT 'domain'    -- 'domain' | 'host' | 'starts_with' | 'exact' | 'regex'
);

CREATE TABLE item_custom_fields (
    id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE,
    field_name TEXT NOT NULL,                    -- ex: "DATABASE_URL", "API_SECRET"
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

-- ----------------------------------------------------------------------------
-- 6. PASSKEYS (WebAuthn / FIDO2)
-- ----------------------------------------------------------------------------
CREATE TABLE passkeys (
    id TEXT PRIMARY KEY,                         -- UUIDv4 interne
    item_id TEXT NULL REFERENCES vault_items(id) ON DELETE SET NULL,
    credential_id BLOB NOT NULL UNIQUE,          -- ID WebAuthn brut
    rp_id TEXT NOT NULL,                         -- Relying Party ID (ex: "github.com")
    rp_name TEXT NOT NULL,
    user_handle BLOB NOT NULL,
    user_name TEXT NOT NULL,
    encrypted_private_key BLOB NOT NULL,         -- Clé privée P-256 / Ed25519 chiffrée
    private_key_nonce BLOB NOT NULL,
    sign_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NULL,
    deleted_at INTEGER NULL
);

CREATE INDEX idx_passkeys_rp_id ON passkeys(rp_id) WHERE deleted_at IS NULL;

-- ----------------------------------------------------------------------------
-- 7. RÈGLES DE ROUTAGE AUTOMATIQUE VERS "DEV LOCAL"
-- ----------------------------------------------------------------------------
CREATE TABLE local_routing_rules (
    id TEXT PRIMARY KEY,
    pattern TEXT NOT NULL UNIQUE,                -- ex: "*.local", "*.test", "*.localhost", "127.0.0.1"
    pattern_type TEXT NOT NULL DEFAULT 'suffix', -- 'suffix' | 'exact_host' | 'cidr' | 'regex'
    target_folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    is_default INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

-- Règles par défaut activées dès la création du coffre (et lors des imports)
INSERT INTO local_routing_rules (id, pattern, pattern_type, target_folder_id, is_default, enabled, created_at)
VALUES
    ('10000000-0000-4000-8000-000000000001', '.local',      'suffix',     '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000002', '.test',       'suffix',     '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000003', '.localhost',  'suffix',     '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000004', 'localhost',   'exact_host', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now')),
    ('10000000-0000-4000-8000-000000000005', '127.0.0.1',   'exact_host', '00000000-0000-4000-8000-000000000002', 1, 1, strftime('%s','now'));

-- ----------------------------------------------------------------------------
-- 8. SÉCURITÉ IA : JETONS MCP CLOISONNÉS (RBAC) & JOURNAL D'AUDIT
-- ----------------------------------------------------------------------------
CREATE TABLE mcp_tokens (
    id TEXT PRIMARY KEY,                         -- UUIDv4
    name TEXT NOT NULL,                          -- ex: "Claude Code - Docker Local"
    token_hash TEXT NOT NULL UNIQUE,             -- Hash BLAKE3/SHA-256 du secret (jamais stocké en clair)
    token_prefix TEXT NOT NULL,                  -- 8 premiers chars pour affichage UI ("ak_mcp_8f9a...")
    
    -- Permissions granulaires (JSON array: ["read_metadata", "blind_fill", "create_and_fill", "exec_env"])
    scopes_json TEXT NOT NULL,
    
    -- Dossier cible par défaut lors d'un `vault_create_and_fill`
    default_create_folder_id TEXT NULL REFERENCES folders(id) ON DELETE SET NULL,
    
    expires_at INTEGER NULL,                     -- TTL optionnel
    revoked_at INTEGER NULL,
    last_used_at INTEGER NULL,
    created_at INTEGER NOT NULL
);

-- Dossiers autorisés pour chaque jeton MCP (cloisonnement strict)
CREATE TABLE mcp_token_folders (
    token_id TEXT NOT NULL REFERENCES mcp_tokens(id) ON DELETE CASCADE,
    folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    PRIMARY KEY (token_id, folder_id)
);

-- Journal d'audit immuable des actions IA
CREATE TABLE mcp_audit_logs (
    id TEXT PRIMARY KEY,
    token_id TEXT NULL REFERENCES mcp_tokens(id) ON DELETE SET NULL,
    action TEXT NOT NULL,                        -- 'vault_search' | 'blind_autofill' | 'cdp_autofill' | 'create_and_fill' | 'exec_env' | 'access_denied'
    target_item_id TEXT NULL,
    target_folder_id TEXT NULL,
    target_origin TEXT NULL,                     -- URL ou commande exécutée
    status TEXT NOT NULL,                        -- 'success' | 'denied' | 'error'
    details TEXT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_audit_created_at ON mcp_audit_logs(created_at DESC);

-- ----------------------------------------------------------------------------
-- 9. CONFIGURATION DES SAUVEGARDES PROGRAMMÉES (Cron Local, S3, Google Drive)
-- ----------------------------------------------------------------------------
CREATE TABLE backup_targets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,                      -- 'local' | 's3' | 'gdrive'
    encrypted_config BLOB NOT NULL,              -- Chemin, bucket S3, clés API ou token OAuth chiffrés
    config_nonce BLOB NOT NULL,
    interval_minutes INTEGER NOT NULL DEFAULT 60,-- ex: 60 min (1h)
    retention_count INTEGER NOT NULL DEFAULT 24, -- Garde les 24 derniers backups
    enabled INTEGER NOT NULL DEFAULT 1,
    last_run_at INTEGER NULL,
    last_status TEXT NULL,
    created_at INTEGER NOT NULL
);

```

---

### 2. Schéma Supabase E2EE (`supabase/migrations/20261115_akooti_e2ee.sql`)

Sur Supabase, **zéro donnée sensible n'est visible** (même par un administrateur de la base PostgreSQL). Le client Rust sérialise chaque objet (dossier, entrée complète avec ses sélecteurs et ses `folder_ids`, ou passkey), le chiffre avec **XChaCha20-Poly1305** et pousse uniquement `ciphertext` + `nonce`.

```sql
-- ============================================================================
-- AKOOTI - SCHÉMA SUPABASE ZERO-KNOWLEDGE & REALTIME SYNC
-- ============================================================================

-- Extension pour UUID
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ----------------------------------------------------------------------------
-- 1. ENVELOPPE CRYPTOGRAPHIQUE DU COFFRE (1 par utilisateur)
-- ----------------------------------------------------------------------------
CREATE TABLE public.akooti_vault_envelopes (
    user_id UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    vault_id UUID NOT NULL,
    kdf_algorithm TEXT NOT NULL DEFAULT 'argon2id',
    kdf_salt TEXT NOT NULL,                      -- Base64
    kdf_memory_kib INTEGER NOT NULL DEFAULT 65536,
    kdf_iterations INTEGER NOT NULL DEFAULT 3,
    kdf_parallelism INTEGER NOT NULL DEFAULT 4,
    wrapped_master_key TEXT NOT NULL,            -- Base64 (chiffrée par le mot de passe maître)
    wrapped_key_nonce TEXT NOT NULL,             -- Base64
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ----------------------------------------------------------------------------
-- 2. ENREGISTREMENTS CHIFFRÉS DE BOUT EN BOUT (Entrées, Dossiers, Passkeys, Règles)
-- ----------------------------------------------------------------------------
-- Un modèle polymorphe chiffré évite même de révéler combien de dossiers ou de
-- passkeys possède l'utilisateur : tout est un enregistrement chiffré typé.
CREATE TABLE public.akooti_sync_records (
    id UUID NOT NULL,                            -- Même UUID que dans SQLCipher local
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    record_type TEXT NOT NULL,                   -- 'folder' | 'item' | 'passkey' | 'routing_rule'
    
    -- Charge utile 100 % chiffrée côté client (JSON sérialisé -> XChaCha20-Poly1305 -> Base64)
    -- Contient : titre, URLs, mot de passe, TOTP, sélecteurs CSS, liste des folder_ids, etc.
    ciphertext TEXT NOT NULL,
    nonce TEXT NOT NULL,                         -- Nonce 24 octets en Base64
    
    -- Horodatage logique pour résolution de conflits sans horloge biaisée
    version BIGINT NOT NULL DEFAULT 1,
    client_updated_at TIMESTAMPTZ NOT NULL,
    server_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,   -- Tombstone pour propager les suppressions
    
    PRIMARY KEY (user_id, id)
);

-- Index pour récupérer uniquement les deltas depuis la dernière synchro
CREATE INDEX idx_sync_records_delta 
    ON public.akooti_sync_records(user_id, server_updated_at);

-- Trigger automatique pour mettre à jour server_updated_at à chaque upsert
CREATE OR REPLACE FUNCTION public.update_server_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.server_updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_akooti_sync_timestamp
    BEFORE INSERT OR UPDATE ON public.akooti_sync_records
    FOR EACH ROW EXECUTE FUNCTION public.update_server_timestamp();

-- ----------------------------------------------------------------------------
-- 3. SÉCURITÉ ROW LEVEL SECURITY (RLS) STRICTE
-- ----------------------------------------------------------------------------
ALTER TABLE public.akooti_vault_envelopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.akooti_sync_records ENABLE ROW LEVEL SECURITY;

-- L'utilisateur ne peut lire/écrire que ses propres enveloppes et enregistrements
CREATE POLICY "Users manage own vault envelope"
    ON public.akooti_vault_envelopes
    FOR ALL
    USING (auth.uid() = user_id)
    WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users manage own sync records"
    ON public.akooti_sync_records
    FOR ALL
    USING (auth.uid() = user_id)
    WITH CHECK (auth.uid() = user_id);

-- ----------------------------------------------------------------------------
-- 4. ACTIVATION DE SUPABASE REALTIME
-- ----------------------------------------------------------------------------
-- Permet à l'application Desktop Akooti d'écouter en WebSocket les modifications
-- poussées par un autre appareil en temps réel.
ALTER PUBLICATION supabase_realtime ADD TABLE public.akooti_sync_records;

```

---

### Points clés de cette conception hybride

* **Pourquoi stocker les `folder_ids` à l'intérieur du `ciphertext` sur Supabase ?**
Dans `akooti_sync_records`, la relation `item_folders` est sérialisée directement dans le payload chiffré de l'entrée (`"folder_ids": ["0000...002", "uuid-travail"]`). Ainsi, non seulement la synchronisation atomique d'une entrée met à jour ses dossiers en une seule opération sans risque de désynchronisation entre deux tables réseau, mais **Supabase ne peut même pas deviner quels identifiants sont regroupés ensemble**.
* **Ce qui reste strictement local (jamais synchronisé sur Supabase) :**
Les tables `mcp_tokens`, `mcp_token_folders`, `mcp_audit_logs` et `backup_targets` restent uniquement dans **SQLCipher local**. Un jeton IA généré pour ton PC de développement local ne doit jamais être répliqué automatiquement sur un autre ordinateur.
