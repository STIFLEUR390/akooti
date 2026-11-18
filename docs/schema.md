# Schémas de données

Akooti manipule deux schémas :

1. **Local (SQLCipher)** — ✅ implémenté. C'est la source de vérité du produit :
   une base chiffrée au niveau fichier (AES-256 via SQLCipher) **et** un
   chiffrement applicatif XChaCha20-Poly1305 sur chaque champ sensible, en RAM.
2. **Supabase (E2EE)** — 🧭 conception cible, non implémentée. Le serveur ne voit
   que des blobs chiffrés côté client, plus les métadonnées techniques
   nécessaires à la synchronisation temps réel.

> **Source de vérité du schéma local :**
> [`crates/akooti-core/migrations/001_init.sql`](../crates/akooti-core/migrations/001_init.sql).
> Ce document explique les choix, l'ordre des colonnes et les invariants ; en cas
> de divergence, la migration gagne.

---

## 1. Amorçage du coffre

Un coffre est un **couple de fichiers** dans le dossier de données de
l'application (`VaultPaths::under_app_data`) :

| Fichier      | Contenu                                                       | Sensible ? |
| ------------ | ------------------------------------------------------------- | ---------- |
| `vault.meta` | En-tête public d'amorçage : magic, version, algorithme, sel, paramètres Argon2id | non (par conception) |
| `vault.db`   | Base SQLCipher (WAL)                                           | oui — chiffrée |

L'en-tête fait **40 octets** et n'est écrit qu'une fois, en `create_new` :

| Offset | Taille | Champ                                                     |
| ------ | ------ | --------------------------------------------------------- |
| 0…7    | 8      | magic `AKOOTI01`                                          |
| 8      | 1      | version de format (`1`)                                   |
| 9      | 1      | algorithme KDF (`1` = Argon2id)                           |
| 10…11  | 2      | réservé                                                   |
| 12…27  | 16     | sel Argon2id                                              |
| 28…31  | 4      | `memory_kib` (LE, défaut 65 536 = 64 Mio)                 |
| 32…35  | 4      | `iterations` (LE, défaut 3)                               |
| 36…39  | 4      | `parallelism` (LE, défaut 4)                              |

Contraintes validées à la lecture (`KdfParams::validate`) : mémoire entre 8 Mio
et 1 Gio, itérations entre 1 et 100, parallélisme entre 1 et 64. Un en-tête
tronqué, un magic inattendu ou une version inconnue lèvent `CorruptVault` — le
coffre n'est jamais ouvert « au mieux ». Voir
[ADR 0004](./adr/0004-vault-bootstrap-header.md).

Les paramètres KDF sont **dupliqués** dans `vault_meta` après ouverture, puis
vérifiés contre l'en-tête public : une incohérence est traitée comme une
corruption, pas comme une migration silencieuse.

### Chiffrement des champs

- Algorithme : XChaCha20-Poly1305, nonce de 24 octets, clé de 32 octets.
- Chaque valeur chiffrée est stockée en **deux colonnes** : `encrypted_*`
  (ciphertext + tag) et `*_nonce`.
- L'**AAD** lie le chiffré à sa position logique, ce qui empêche de déplacer un
  blob d'une entrée à l'autre :

```text
akooti:v1:item:{item_id}:field:{password|totp|notes}
akooti:v1:item:{item_id}:custom:{custom_field_id}
```

---

## 2. Schéma local

### 2.1 `vault_meta` — métadonnées du coffre (1 ligne)

`id = 1` par contrainte. Porte `vault_id`, les paramètres KDF dupliqués depuis
l'en-tête (`kdf_salt`, `kdf_memory_kib`, `kdf_iterations`, `kdf_parallelism`),
la clé maître encapsulée (`wrapped_master_key` + `wrapped_key_nonce`),
`biometrics_enabled` (trait présent, fournisseur non branché) et
`auto_lock_minutes` (défaut 15).

### 2.2 `folders` — dossiers

Arborescence optionnelle (`parent_id`, cascade), `slug` stable pour les dossiers
système, `icon`/`color` pour l'affichage, `is_system = 1` pour
`Personnel` / `Dev Local` / `Agents & MCP` (non supprimables, non renommables),
`require_biometric_for_mcp` comme garde-fou par dossier, `deleted_at` pour la
suppression logique.

Trois dossiers sont insérés par la migration, avec des UUID fixes :

| Dossier        | `id`                                   | `slug`       |
| -------------- | -------------------------------------- | ------------ |
| Personnel      | `00000000-0000-4000-8000-000000000001` | `personal`   |
| Dev Local      | `00000000-0000-4000-8000-000000000002` | `dev-local`  |
| Agents & MCP   | `00000000-0000-4000-8000-000000000003` | `agents-mcp` |

Ces identifiants sont des constantes publiques de `akooti-core`
(`PERSONAL_FOLDER_ID`, `DEV_LOCAL_FOLDER_ID`, `AGENTS_MCP_FOLDER_ID`) : les
jetons MCP, les règles de routage et les tests s'y réfèrent.

### 2.3 `vault_items` — entrées

- Identité : `id`, `item_type` (`login`, `api_key`, `note`, `env_set`), `title`,
  `primary_url`, `username`.
- Secrets (chiffrés) : `encrypted_password`/`password_nonce`,
  `encrypted_totp_secret`/`totp_nonce`, `encrypted_notes`/`notes_nonce`.
- Sélecteurs mémorisés : `username_selector`, `password_selector`,
  `totp_selector`, `submit_selector` — réutilisés par l'injection aveugle.
- Hygiène : `zxcvbn_score`, `password_sha1_prefix` (5 caractères, pour HIBP),
  `is_compromised`, `password_updated_at`. Remplies par l'audit de santé
  (`crates/akooti-core/src/health.rs`) ; le suffixe SHA-1 n'est jamais stocké,
  la comparaison avec une réponse HIBP se fait à la volée.
- Traçabilité IA : `created_by_agent`, `expires_at` (TTL des comptes de test).
- Divers : `favorite`, `created_at`, `updated_at`, `deleted_at`.
- Index : `idx_items_primary_url` (filtré sur `deleted_at IS NULL`) et
  `idx_items_updated_at`.

### 2.4 Relations

| Table                | Rôle                                                                                   | Invariant |
| -------------------- | -------------------------------------------------------------------------------------- | --------- |
| `item_folders`       | Appartenance **N:N** d'une entrée à ses dossiers                                       | Clé primaire `(item_id, folder_id)` ; cascade des deux côtés ; jamais de duplication d'entrée |
| `item_uris`          | URI multiples avec `match_type` (`domain`, `host`, `starts_with`, `exact`, `regex`)     | — |
| `item_custom_fields` | Champs personnalisés, chiffrés, avec `sort_order` et `is_secret`                        | AAD `…:custom:{id}` |
| `password_history`   | Versions précédentes du mot de passe, chiffrées                                        | Écrit uniquement en cas de changement réel |

### 2.5 `passkeys` — WebAuthn servi par le coffre

`credential_id` unique, `rp_id`/`rp_name`, `user_handle`, clé privée chiffrée
(`encrypted_private_key` + nonce), `sign_count`, `last_used_at`. Une ligne par
credential, écrite par le coffre seul (`crates/akooti-core/src/passkeys.rs`) :
la migration de la phase 1 avait déjà créé la table, si bien que la phase 6 n'a
demandé aucun changement de schéma.

- `encrypted_private_key` ne contient que le scalaire P-256, chiffré en
  XChaCha20-Poly1305 sous l'AAD `akooti:v1:passkey:<credential id>` : une ligne
  recopiée sur un autre credential cesse de s'authentifier.
- La clé publique n'est **pas** stockée : elle se redérive du scalaire. Un seul
  secret à protéger, au lieu de deux qui pourraient diverger.
- `item_id` reste `NULL` : un passkey appartient à une relying party, pas à une
  entrée, et il n'a donc pas de dossier — le cloisonnement par dossier des
  jetons MCP ne s'y applique pas (voir [architecture.md](./architecture.md)
  §4.6).
- `sign_count` avance à chaque assertion, pour qu'une relying party puisse
  repérer un clone ; `last_used_at` date la dernière cérémonie.

### 2.6 `local_routing_rules` — routage `Dev Local`

`pattern` unique, `pattern_type` (`suffix`, `exact_host`, `cidr`, `regex`),
`target_folder_id`, `is_default`, `enabled`. Cinq règles par défaut visent
`Dev Local` : `.local`, `.test`, `.localhost` (suffixes), `localhost` et
`127.0.0.1` (hôtes exacts).

### 2.7 `mcp_tokens`, `mcp_token_folders`, `mcp_audit_logs` — sécurité IA

- `mcp_tokens` : `token_hash` (SHA-256 du secret, **jamais** le secret),
  `token_prefix` (8 caractères pour l'affichage), `scopes_json`, dossier de
  création par défaut, `expires_at`, `revoked_at`, `last_used_at`.
- `mcp_token_folders` : cloisonnement strict, un jeton ne voit que ces dossiers.
- `mcp_audit_logs` : `action` (`vault_search`, `cdp_autofill`, `blind_autofill`,
  `create_and_fill`, `exec_env`, `passkey_use`, `access_denied`), `status` (`success`,
  `denied`, `error`), `target_item_id`, `target_origin`, `details` — jamais de
  valeur secrète. Index `idx_audit_created_at`.

Ces trois tables **ne quittent jamais la machine** : elles ne sont pas
synchronisées (règle AI-5).

### 2.8 `backup_targets` et `backup_runs` — sauvegardes

`provider` (`local`, `s3`, `gdrive`), configuration chiffrée
(`encrypted_config` + nonce, AAD `akooti:v1:backup-target:<id>`),
`interval_minutes` (60 minimum), `retention_count`, `last_run_at`,
`last_status`. Les identifiants (clé S3, refresh token Drive) ne sortent jamais
du cœur et ne sont **jamais** renvoyés à l'interface.

`backup_runs` journalise chaque tentative : `target_id`, `started_at`,
`finished_at`, `status` (`success` / `error`), `detail` sans secret,
`size_bytes`, `snapshot_name`. La rétention supprime les instantanés
`akooti-<epoch>.akooti` au-delà du nombre conservé.

### 2.9 `sync_config` — état local de la synchronisation

Une seule ligne : `enabled`, `supabase_url`, `anon_key` (publique), `user_id`,
`device_id`, `cursor` (dernier `server_updated_at` reçu), `last_sync_at`,
`last_status`, `last_error`, et la session Supabase **chiffrée**
(`encrypted_session` + nonce, AAD `akooti:v1:sync-session`). Cette table est
locale uniquement : elle n'est ni synchronisée ni exportée (AI-5).

### 2.10 PRAGMA

```sql
PRAGMA foreign_keys = ON;   -- intégrité référentielle
PRAGMA journal_mode = WAL;  -- lectures concurrentes, récupération après crash
```

Le mode WAL explique la présence de `vault.db-wal` et `vault.db-shm` à côté de
`vault.db` : ce sont des fichiers d'exploitation normaux, à sauvegarder avec la
base (ou à faire disparaître par un checkpoint avant copie).

---

## 3. Schéma Supabase (livré : `supabase/setup.sql`)

Objectif : un administrateur de la base PostgreSQL ne doit pouvoir lire **ni un
titre, ni une URL, ni un nom de dossier, ni même compter** les dossiers ou
passkeys d'un utilisateur. Le client Rust sérialise chaque objet, le chiffre en
XChaCha20-Poly1305 avec la clé dérivée localement, et ne pousse que
`ciphertext` + `nonce`.

Le fichier appliqué au projet de l'utilisateur est la source de vérité ; il est
reproduit ici pour la revue.

```sql
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Enveloppe cryptographique du coffre (une par utilisateur)
CREATE TABLE public.akooti_vault_envelopes (
    user_id UUID PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    vault_id UUID NOT NULL,
    kdf_algorithm TEXT NOT NULL DEFAULT 'argon2id',
    kdf_salt TEXT NOT NULL,              -- Base64
    kdf_memory_kib INTEGER NOT NULL DEFAULT 65536,
    kdf_iterations INTEGER NOT NULL DEFAULT 3,
    kdf_parallelism INTEGER NOT NULL DEFAULT 4,
    wrapped_master_key TEXT NOT NULL,    -- Base64
    wrapped_key_nonce TEXT NOT NULL,     -- Base64
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Enregistrements chiffrés de bout en bout (entrées, dossiers, passkeys, règles)
CREATE TABLE public.akooti_sync_records (
    id UUID NOT NULL,                    -- même UUID que dans SQLCipher
    user_id UUID NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    record_type TEXT NOT NULL,           -- 'folder' | 'item' | 'passkey' | 'routing_rule'
    ciphertext TEXT NOT NULL,            -- JSON sérialisé -> XChaCha20 -> Base64
    nonce TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    client_updated_at TIMESTAMPTZ NOT NULL,
    server_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (user_id, id)
);

CREATE INDEX idx_sync_records_delta
    ON public.akooti_sync_records(user_id, server_updated_at);

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

ALTER TABLE public.akooti_vault_envelopes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.akooti_sync_records ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users manage own vault envelope"
    ON public.akooti_vault_envelopes FOR ALL
    USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users manage own sync records"
    ON public.akooti_sync_records FOR ALL
    USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

ALTER PUBLICATION supabase_realtime ADD TABLE public.akooti_sync_records;
```

### Pourquoi ces choix

- **Modèle polymorphe** (`record_type` + `ciphertext`) : le serveur ne peut pas
  déduire la structure du coffre, ni le nombre de dossiers ou de passkeys.
- **`folder_ids` dans le chiffré** : l'appartenance multi-dossiers est sérialisée
  avec l'entrée, donc une écriture met à jour l'entrée et ses dossiers
  atomiquement, et le serveur ne peut pas regrouper des identifiants.
- **`is_deleted` (tombstone)** : une suppression doit se propager sans révéler
  ce qui a été supprimé.
- **RLS sur `auth.uid()`** : même avec la clé anon, un utilisateur ne lit que ses
  lignes ; l'API ne sert que de transport de blobs.
- **Ce qui n'est jamais synchronisé** : `mcp_tokens`, `mcp_token_folders`,
  `mcp_audit_logs`, `backup_targets`. Un jeton lié à une machine de
  développement ne doit pas se répliquer ailleurs.

---

## 4. Invariants à respecter lors d'une évolution

1. Toute nouvelle colonne secrète suit le couple `encrypted_*` + `*_nonce` et un
   AAD qui contient l'identifiant de l'entrée et le nom du champ.
2. Une nouvelle table qui porte des données utilisateur reçoit `deleted_at` si sa
   suppression doit être propagée plus tard.
3. Une migration livrée n'est jamais modifiée : on en ajoute une nouvelle, et
   `db.rs` vérifie qu'elle reste idempotente.
4. Une table contenant des jetons, des journaux d'audit ou des configurations de
   sauvegarde est déclarée « locale uniquement » et n'apparaît pas dans le schéma
   Supabase.
5. Le schéma local et ce document évoluent dans la même PR (règle ARCH-5).
