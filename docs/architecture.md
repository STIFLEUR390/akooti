### 1. Architecture Système (Flux & Frontières de Confiance)

Akooti repose sur un **binaire unique compilé en Rust (`akooti`)** accompagné d'une **extension multi-navigateurs WXT**. Le processus principal agit comme un **daemon local sécurisé** qui détient la clé maître déchiffrée en RAM (`zeroize`) et arbitre tous les accès :

```text
+-----------------------------------------------------------------------------------+
|                        INTERFACE UTILISATEUR & NAVIGATEURS                        |
|                                                                                   |
|  [ App Desktop Tauri ]      [ Extension WXT ]           [ Navigateurs IA / CLI ]  |
|   (React + TypeScript)       (Chrome/FF/Edge)            (agent-browser, Obscura) |
+------------+------------------------+-------------------------------+-------------+
             | Tauri IPC              | Native Messaging              | CDP (9222)
             | (Capabilities)         | (4-byte length JSON)          | ou Process Env
             v                        v                               |
+------------+------------------------+-------------------------------+-------------+
|                        BINAIRE UNIQUE RUST (`akooti`)                             |
|                                                                                   |
|  [ GUI / Tray Daemon ]  <--- IPC Local (interprocess) --->  [ CLI / Serveur MCP ] |
|        |                     (Unix Socket / Named Pipe)     (`akooti mcp --token`)|
|        |                                                             |            |
|        +-----------------------------+-------------------------------+            |
|                                      v                                            |
|                     +---------------------------------+                           |
|                     |      `akooti-core` (Rust)       |                           |
|                     | - RBAC & Scoped Folder Tokens   |                           |
|                     | - Crypto (Argon2id + XChaCha20) |                           |
|                     | - Routage Auto `Dev Local`      |                           |
|                     | - Audit (zxcvbn + HIBP k-Anon)  |                           |
|                     +----------------+----------------+                           |
+--------------------------------------|--------------------------------------------+
                                       |
             +-------------------------+-------------------------+
             v                                                   v
+-------------------------+                         +-------------------------------+
|    SQLCipher Local      |                         |  Sync & Backups (Zero-Know.)  |
| - Dossiers (N:N)        |                         | - Supabase Realtime (E2EE)    |
| - Entrées & Sélecteurs  |                         | - Cron 1h : Local / S3 / Drive|
| - Passkeys & TOTP       |                         +-------------------------------+
+-------------------------+

```

#### Principes architecturaux clés :

1. **Un seul binaire multi-modes (`tauri-plugin-cli`) :**
* `akooti` $\rightarrow$ Lance l'interface graphique et le daemon en arrière-plan (*System Tray*).
* `akooti native-host` $\rightarrow$ Sous-processus lancé par le navigateur pour l'extension WXT.
* `akooti mcp --token <clé>` $\rightarrow$ Sous-processus *headless* lancé par Codex, Claude Code, Cursor ou Antigravity.


2. **Communication Inter-Processus Isolée (`ipc_daemon.rs`) :**
Les sous-processus `native-host` et `mcp` n'ont pas accès directement à la clé maître : ils interrogent le daemon principal via un socket local restreint à l'utilisateur OS (`chmod 600` / ACL Windows), qui vérifie les permissions du jeton avant d'exécuter l'injection aveugle.
3. **Zero-Leak garantit :**
Les mots de passe ne traversent jamais la frontière MCP vers le LLM : ils vont directement de la RAM de `akooti` vers l'extension WXT, le port CDP du navigateur headless, ou l'environnement mémoire d'un processus enfant (`vault_exec`).

4. **Amorçage local du coffre :**
Le shell Tauri résout uniquement `app_data_dir` et transmet ce chemin au cœur.
`akooti-core` gère ensuite `vault.meta` (sel et paramètres Argon2id publics) et
`vault.db` (SQLCipher), puis applique le verrouillage d'inactivité et les
événements de reprise sans dépendance Tauri dans le crate métier.

---

### 2. Structure Complète du Monorepo (`akooti/`)

```text
akooti/
├── Cargo.toml                           # Workspace Rust global (crates/* + src-tauri)
├── package.json                         # Workspace Bun ("apps/*", "packages/*")
├── bun.lock                             # Lockfile Bun (commité)
├── .gitignore
├── LICENSE                              # Licence Open Source (MIT / AGPLv3)
│
├── skills/
│   └── akooti/
│       └── SKILL.md                     # Guide d'utilisation MCP/CLI pour Codex, Claude Code, Antigravity
│
├── crates/                              # Cœur métier Rust (0 dépendance UI / Tauri)
│   ├── akooti-core/                     # Cryptographie, SQLCipher, routage, audit, sync & imports
│   │   ├── Cargo.toml
│   │   ├── migrations/
│   │   │   └── 001_init.sql             # Schéma SQLCipher (dossiers système, item_folders N:N, règles)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── crypto.rs                # Argon2id, XChaCha20-Poly1305, gestion RAM zeroize
│   │       ├── db.rs                    # Requêtes SQLCipher, multi-dossiers, historique, TTL tests
│   │       ├── header.rs                # En-tête public d'amorçage du coffre
│   │       ├── session.rs               # Verrouillage et compteur d'inactivité
│   │       ├── vault.rs                 # Cycle de vie et API métier du coffre
│   │       ├── routing.rs               # Moteur de règles auto vers `Dev Local` (.local, .test, etc.)
│   │       ├── generator.rs             # Générateur aléatoire (OsRng) & Passphrases Diceware
│   │       ├── health.rs                # Audit offline (zxcvbn, doublons, ancienneté) + HIBP k-Anonymity
│   │       ├── totp.rs                  # Génération et vérification des codes 2FA TOTP
│   │       ├── passkeys.rs              # Signatures WebAuthn / FIDO2 (P-256 / Ed25519)
│   │       ├── rbac.rs                  # Validation des clés API cloisonnées par dossier (TTL, scopes)
│   │       ├── sync_supabase.rs         # Synchronisation temps réel E2EE via Supabase Realtime
│   │       ├── backup/
│   │       │   ├── mod.rs               # Planificateur Cron (toutes les 1h) & rotation
│   │       │   ├── local.rs             # Sauvegarde dossier local / NAS
│   │       │   ├── s3.rs                # Sauvegarde S3 (AWS, Cloudflare R2, MinIO)
│   │       │   └── gdrive.rs            # Sauvegarde Google Drive (OAuth2)
│   │       └── importers/
│   │           ├── mod.rs               # Modèle normalisé ImportBundle + routage Dev Local + dédup
│   │           ├── buttercup.rs         # Import natif .bcup (AES-256-CBC/HMAC) & .csv
│   │           ├── bitwarden.rs         # Import .json (chiffré ou clair)
│   │           ├── protonpass.rs        # Import .json / .csv
│   │           ├── keepass.rs           # Import natif .kdbx (v3/v4) & .xml
│   │           ├── onepassword.rs       # Import .1pux & .csv
│   │           └── browser_csv.rs       # Import CSV Chrome, Brave, Edge, Firefox
│   │
│   └── akooti-mcp/                      # Serveur MCP & moteurs d'injection Zero-Leak
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── server.rs                # Serveur MCP stdio (search, blind_autofill, create_and_fill, exec)
│           ├── cdp_injector.rs          # Injection directe via Chrome DevTools Protocol (agent-browser, Obscura)
│           └── env_exec.rs              # Exécution CLI avec injection env et masquage des logs
│
├── src-tauri/                           # Application Desktop Tauri v2 (GUI + CLI + Daemon IPC)
│   ├── Cargo.toml
│   ├── tauri.conf.json                  # Config Tauri, permissions, sous-commandes CLI (`mcp`, `native-host`)
│   ├── capabilities/
│   │   └── default.json                 # Permissions strictes entre le frontend React et Rust
│   ├── icons/
│   └── src/
│       ├── main.rs                      # Point d'entrée : route vers GUI, `akooti mcp` ou `akooti native-host`
│       ├── ipc_daemon.rs                # Serveur IPC local (interprocess : Unix Socket / Named Pipe)
│       ├── native_host.rs               # Protocole stdin/stdout 4-bytes length-prefixed pour WXT
│       ├── native_register.rs           # Enregistrement auto des manifestes Native Messaging (Linux/macOS/Win)
│       ├── biometrics.rs                # Déverrouillage Windows Hello, Touch ID, Linux Polkit
│       ├── tray.rs                      # Icône barre des tâches (maintient le daemon actif)
│       └── commands/                    # Commandes IPC exposées à l'interface Desktop
│           ├── vault.rs                 # CRUD dossiers (multi-appartenance), entrées, sélecteurs CSS
│           ├── routing.rs               # Gestion des règles `Dev Local` & tri rétroactif
│           ├── mcp_tokens.rs            # Création/révocation des clés par dossier & logs d'audit
│           ├── sync_backup.rs           # Configuration Supabase, S3, Google Drive, Local
│           └── import_export.rs         # Importation multi-formats & exportation d'urgence
│
├── packages/
│   └── shared/                          # Contrats TypeScript partagés (Desktop <-> Extension)
│       ├── package.json
│       └── src/
│           ├── models.ts                # Types Folder, VaultItem, RoutingRule, HealthReport
│           └── protocol.ts              # Schémas des messages Native Messaging & WebAuthn
│
└── apps/
    ├── desktop/                         # Interface graphique Tauri (React + TypeScript + Tailwind)
    │   ├── package.json
    │   └── src/
    │       ├── App.tsx
    │       ├── components/
    │       │   ├── SidebarFolders.tsx   # Dossiers système (`Personnel`, `Dev Local`, `Agents & MCP`) + perso
    │       │   ├── ItemDetail.tsx       # Vue entrée, multi-dossiers, TOTP live, sélecteurs CSS enregistrés
    │       │   ├── GeneratorModal.tsx   # Générateur mots de passe & passphrases Diceware
    │       │   └── HealthDashboard.tsx  # Audit zxcvbn, doublons et fuites HIBP
    │       └── pages/
    │           ├── VaultPage.tsx        # Vue principale du coffre
    │           ├── McpAccessPage.tsx    # Gestion des jetons IA par dossier & journal d'audit
    │           ├── SyncBackupPage.tsx   # Configuration Supabase temps réel & Cron S3/Drive/Local
    │           ├── RoutingRulesPage.tsx # Règles de sous-domaines locaux & tri rétroactif
    │           └── ImportPage.tsx       # Assistant d'importation (case `Dev Local` cochée par défaut)
    │
    └── extension/                       # Extension multi-navigateurs WXT
        ├── package.json
        ├── wxt.config.ts                # Config Manifest V3/V2 (Chrome, Firefox, Edge, Brave, Safari)
        └── entrypoints/
            ├── background.ts            # Service Worker (pont Native Messaging <-> binaire Akooti)
            ├── content.ts               # ISOLATED world : détection formulaires, capture & injection sélecteurs
            ├── webauthn.content.ts      # MAIN world : interception Passkeys (navigator.credentials)
            └── popup/                   # Interface rapide de l'extension
                ├── index.html
                └── App.tsx

```
