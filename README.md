# Akooti

> **Le coffre-fort local-first pensé pour les agents IA.**
> Un binaire Rust unique — coffre chiffré, extension multi-navigateurs, CLI et serveur MCP — où l'IA peut agir sans jamais voir un secret.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Tauri v2](https://img.shields.io/badge/Tauri-v2-24c8db?logo=tauri)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.90%2B-orange?logo=rust)](https://www.rust-lang.org)
[![WXT](https://img.shields.io/badge/Extension-WXT%20MV3-67d55e)](https://wxt.dev)

---

## Sommaire

- [Pourquoi Akooti](#pourquoi-akooti)
- [Fonctionnalités](#fonctionnalités)
- [Architecture](#architecture)
- [Stack technique](#stack-technique)
- [Structure du dépôt](#structure-du-dépôt)
- [Modèle de sécurité](#modèle-de-sécurité)
- [Serveur MCP & intégration IA](#serveur-mcp--intégration-ia)
- [Démarrage rapide](#démarrage-rapide)
- [Développement](#développement)
- [Feuille de route](#feuille-de-route)
- [Documentation](#documentation)
- [Contribuer](#contribuer)
- [Licence](#licence)

---

## Pourquoi Akooti

Les agents de codage (Claude Code, Codex, Cursor, Antigravity) et les navigateurs autonomes ont besoin de se connecter à des services : bases de données locales, back-offices, comptes de test. Aujourd'hui, cela se termine trop souvent par des secrets collés dans un prompt, un `.env` en clair ou un mot de passe dans l'historique du shell.

Akooti renverse le modèle :

- **Local-First** — 100 % fonctionnel hors ligne. Le coffre SQLCipher local est la source de vérité, aucune infrastructure propriétaire n'est requise.
- **Zero-Knowledge** — toute la crypto (dérivation, chiffrement, déchiffrement) se fait côté client. Un serveur Supabase, un bucket S3 ou un Drive ne voient que des blobs chiffrés.
- **Zero-Leak** — aucun outil MCP ne retourne un mot de passe, une clé API ou un code TOTP. Les secrets vont de la RAM du binaire Rust directement vers leur cible : extension navigateur, port CDP ou environnement d'un processus enfant.

## Fonctionnalités

### Coffre & organisation

- **Multi-dossiers (N:N)** — une entrée appartient à plusieurs dossiers sans duplication ; toute modification est répercutée partout (table de liaison `item_folders`).
- **Dossiers système protégés** — `Personnel`, `Dev Local` et `Agents & MCP` sont créés automatiquement, non supprimables et non renommables.
- **Comptes éphémères (TTL)** — les comptes de test créés par un agent expirent et sont purgés automatiquement.
- **Historique des mots de passe** — versions précédentes conservées chiffrées sur chaque fiche.
- **URIs multiples et champs personnalisés** — avec politiques de correspondance (`domain`, `host`, `starts_with`, `exact`, `regex`).
- **Presse-papiers sûr** — effacement automatique 30 s après copie d'un secret ou d'un code TOTP.

### Authentification moderne

- **TOTP intégré** — génération et vérification des codes 6 chiffres, remplissage aveugle via l'extension ou le MCP.
- **Passkeys (WebAuthn / FIDO2)** — interception de `navigator.credentials.create()` / `get()` en `world: MAIN`, signature P-256 / Ed25519, clés privées stockées chiffrées.
- **Biométrie native** — Windows Hello, Touch ID, Linux Polkit / FIDO2 ; verrouillage automatique après inactivité ou mise en veille.

### Routage & import universel

- **Import** — Buttercup (`.bcup`, `.csv`), Bitwarden / Vaultwarden (`.json` chiffré ou clair), Proton Pass (`.json`, `.csv`), KeePass / KeePassXC (`.kdbx` v3/v4, `.xml`, `.csv`), 1Password (`.1pux`, `.csv`), CSV navigateurs (Chrome, Brave, Edge, Firefox).
- **Routage `Dev Local`** — détection automatique de `localhost`, `127.0.0.1`, `.local`, `.localhost`, `.test` et de suffixes personnalisés (`*.docker`, `*.internal`…), à l'import comme en rétroactif, **sans retirer** l'entrée de son dossier d'origine.
- **Hygiène post-import** — aperçu, détection de doublons, effacement sécurisé du fichier source en clair après validation de la transaction.

### Intégration IA

- **Serveur MCP stdio** — `akooti mcp --token <clé>` exposé aux agents, sans accès à la clé maître.
- **Jetons cloisonnés par dossier (RBAC)** — permissions granulaires (`read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`), TTL, révocation immédiate, journal d'audit local.
- **Injection aveugle** — par sélecteurs CSS/JS via l'extension WXT (Native Messaging) ou directement par Chrome DevTools Protocol.
- **`vault_exec`** — secrets injectés dans la mémoire d'un processus enfant (`docker compose up`, scripts de test), avec masquage automatique dans `stdout` / `stderr`.
- **Agent Skill embarqué** — `skills/akooti/SKILL.md` guide les agents sur l'usage sûr du MCP et de la CLI.

### Sync, sauvegardes & santé

- **Sync temps réel Supabase (BYO)** — Zero-Knowledge, Last-Write-Wins, tombstones, un enregistrement chiffré par objet.
- **Sauvegardes programmées toutes les 1 h** — snapshots `.akooti` chiffrés vers dossier local / NAS, S3 (AWS, R2, MinIO, Wasabi) et Google Drive (OAuth2), avec rétention configurable.
- **Santé du coffre (offline)** — score `zxcvbn`, détection des réutilisations et de l'ancienneté.
- **Fuites sans fuite** — vérification HIBP par _k-Anonymity_ : seuls les 5 premiers caractères du SHA-1 quittent la machine.

## Architecture

Un **binaire unique Rust (`akooti`)** embarque l'interface GUI, le daemon, le pont navigateur et le serveur MCP. Les sous-processus n'ont jamais la clé maître : ils passent par un socket local restreint à l'utilisateur OS.

```text
+-----------------------------------------------------------------------------------+
|                        INTERFACE UTILISATEUR & NAVIGATEURS                        |
|                                                                                   |
|  [ App Desktop Tauri ]      [ Extension WXT ]           [ Navigateurs IA / CLI ]  |
|   (React + TypeScript)       (Chrome/FF/Edge)            (agent-browser, Obscura) |
+------------+------------------------+-------------------------------+-------------+
             | Tauri IPC              | Native Messaging              | CDP (9222)
             | (Capabilities)         | (4-byte length JSON)          | ou env de process
             v                        v                               |
+------------+------------------------+-------------------------------+-------------+
|                        BINAIRE UNIQUE RUST (`akooti`)                             |
|                                                                                   |
|  [ GUI / Tray Daemon ]  <--- IPC Local (interprocess) --->  [ CLI / Serveur MCP ] |
|        |                     (Unix Socket / Named Pipe)     (`akooti mcp --token`)|
|        +-----------------------------+------------------------------+             |
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

**Principes structurants :**

1. **Cœur métier isolé** — `akooti-core` n'importe aucune dépendance Tauri, WebView ou React (testable, portable vers le mobile).
2. **Un binaire, trois modes** — `akooti` (GUI + tray), `akooti native-host` (pont WXT), `akooti mcp --token` (serveur MCP headless).
3. **Secret jamais dans le contexte LLM** — la frontière MCP ne transporte que des métadonnées et des résultats d'action.
4. **Socket local privé** — `chmod 600` / ACL Windows, aucun serveur HTTP local non authentifié.

## Stack technique

| Domaine             | Choix                                                                                                |
| ------------------- | ---------------------------------------------------------------------------------------------------- |
| Application desktop | Tauri v2 (Rust + React + TypeScript + Tailwind CSS)                                                  |
| CLI / daemon        | `tauri-plugin-cli`, `interprocess` (Unix Socket / Named Pipe), tray                                  |
| Extension           | WXT (TypeScript / React), Manifest V3 & V2, Chrome / Brave / Edge / Firefox / Safari                 |
| Gestion des paquets | Bun 1.4+ (workspaces natifs, `bun.lock` commité)                                                     |
| Base locale         | SQLCipher (via `rusqlite`) + migrations SQL versionnées                                              |
| Cryptographie       | Argon2id (KDF), XChaCha20-Poly1305 (AEAD), `zeroize`, `OsRng`                                        |
| Audit               | `zxcvbn` (offline) + HIBP _k-Anonymity_                                                              |
| Sync                | Supabase Realtime (WebSocket) + RLS stricte, payloads E2EE                                           |
| Sauvegardes         | Cron interne, local / S3 (S3-compatible) / Google Drive OAuth2                                       |
| MCP                 | SDK Rust `rmcp` (transport stdio)                                                                    |
| Tests app Tauri     | [MCP Server Tauri](https://github.com/hypothesi/mcp-server-tauri) (plugin `tauri-plugin-mcp-bridge`) |

## Structure du dépôt

```text
akooti/
├── Cargo.toml                     # Workspace Rust (crates/* + src-tauri)
├── package.json                   # Workspace Bun (apps/*, packages/*)
├── bun.lock                       # Lockfile Bun (commité)
├── docs/                          # Architecture, fonctionnalités, règles, schémas SQL
├── skills/akooti/SKILL.md         # Guide d'usage MCP/CLI pour les agents
│
├── crates/
│   ├── akooti-core/               # Crypto, SQLCipher, routage, audit, sync, imports
│   │   ├── migrations/001_init.sql
│   │   └── src/{crypto,db,routing,generator,health,totp,passkeys,rbac,sync_supabase}.rs
│   └── akooti-mcp/                # Serveur MCP + injection (CDP, env_exec)
│
├── src-tauri/                     # GUI, CLI, daemon IPC, tray, biométrie
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── src/{main,ipc_daemon,native_host,native_register,biometrics,tray}.rs
│
├── packages/shared/               # Contrats TypeScript partagés (modèles + protocole)
│
└── apps/
    ├── desktop/                   # Interface React (coffre, MCP, sync, routing, import)
    └── extension/                 # Extension WXT (background, content, webauthn, popup)
```

## Modèle de sécurité

| Menace                                         | Contre-mesure                                                                  |
| ---------------------------------------------- | ------------------------------------------------------------------------------ |
| LLM qui lit un secret                          | Les tools MCP ne renvoient que des métadonnées ; l'injection est aveugle       |
| Secret en RAM après usage                      | `zeroize::Zeroizing<T>` systématique + verrouillage auto (15 min / veille)     |
| Lecture du socket IPC par un autre utilisateur | Socket `chmod 600` / ACL Windows, pas de HTTP local                            |
| Exfiltration côté cloud                        | E2EE client XChaCha20-Poly1305 ; le serveur ne voit que `ciphertext` + `nonce` |
| Jeton MCP trop permissif                       | Scopes granulaires + liste blanche de `folder_id` + TTL + audit                |
| Fichier d'import en clair laissé sur le disque | Proposition d'effacement sécurisé après commit SQLCipher                       |
| Presse-papiers qui fuit                        | Effacement automatique après 30 s                                              |

Règles détaillées : [`docs/regles.md`](./docs/regles.md).

## Serveur MCP & intégration IA

Chaque jeton est limité à un ou plusieurs dossiers et à des permissions explicites.

| Outil                   | Rôle                                                                                    | Permission        |
| ----------------------- | --------------------------------------------------------------------------------------- | ----------------- |
| `vault_status`          | Décrit le jeton courant : nom, portées, dossiers autorisés et expiration                | —                 |
| `vault_search`          | Recherche par titre / URL / identifiant — **métadonnées seules**                        | `read_metadata`   |
| `vault_blind_autofill`  | Remplit un formulaire via l'extension WXT avec des sélecteurs CSS/JS fournis            | `blind_fill`      |
| `vault_cdp_autofill`    | Remplit un navigateur piloté par IA via Chrome DevTools Protocol                        | `blind_fill`      |
| `vault_create_and_fill` | Génère un mot de passe, l'enregistre (dossier `Agents & MCP`) et remplit l'inscription  | `create_and_fill` |
| `vault_exec`            | Lance une commande avec les secrets injectés en variables d'environnement, logs masqués | `exec_env`        |

Exemple de configuration client :

```json
{
  "mcpServers": {
    "akooti": {
      "command": "/usr/local/bin/akooti",
      "args": ["mcp", "--token", "ak_mcp_xxxxxxxx"]
    }
  }
}
```

> Le jeton est visible dans l'application desktop (page **Accès IA**), avec ses dossiers autorisés, ses scopes et son expiration. Le secret n'est affiché qu'une fois, à la création.

**État d'implémentation (phase 5)**

- Le serveur MCP stdio (`rmcp`), les jetons cloisonnés, l'audit et le daemon
  IPC local sont opérationnels : `akooti mcp --token …` parle au daemon par un
  socket Unix `0600` (`$XDG_RUNTIME_DIR/akooti/daemon.sock`).
- `vault_cdp_autofill`, `vault_create_and_fill` et `vault_exec` sont branchés.
- `vault_blind_autofill` répond `unavailable` : le pont Native Messaging de
  l'extension arrive à la phase suivante — utiliser `vault_cdp_autofill` en
  attendant. Le transport Windows (named pipe) est également différé
  (`docs/adr/0006-transport-ipc-socket-unix.md`).

## Démarrage rapide

### Prérequis

- Rust 1.90+ et Cargo
- Bun 1.4+ (runtime, gestionnaire de paquets et workspaces)
- Dépendances système Tauri v2 ([guide officiel](https://tauri.app/start/prerequisites/))
- Une toolchain SQLCipher (ou l'option `bundled-sqlcipher` du crate `rusqlite`)

### Installation

```bash
git clone https://github.com/STIFLEUR390/akooti.git
cd akooti
bun install
bun run tauri dev
```

### Commandes usuelles

```bash
bun run tauri dev              # Application desktop en mode développement
bun run tauri build            # Binaire + installeurs
bun run --filter @akooti/desktop dev     # Frontend seul (navigateur)
bun run --filter @akooti/extension dev   # Extension WXT (HMR)
bun run --filter @akooti/extension build # Build multi-navigateurs
cargo test --workspace         # Tests unitaires du cœur métier
cargo clippy --workspace --all-targets
```

### Modes du binaire

```bash
akooti                          # GUI + daemon (tray)
akooti native-host              # Pont Native Messaging lancé par le navigateur
akooti mcp --token ak_mcp_...   # Serveur MCP stdio pour les agents
```

## Développement

### Tester et déboguer l'application Tauri

Le dépôt est prévu pour être piloté par le [MCP Server Tauri](https://github.com/hypothesi/mcp-server-tauri). Le plugin de bridge s'active **uniquement en build debug** et n'a aucun impact en production.

1. Ajouter le plugin côté Rust :

   ```toml
   # src-tauri/Cargo.toml
   [dependencies]
   tauri-plugin-mcp-bridge = "0.13"
   ```

   ```rust
   // src-tauri/src/main.rs
   #[cfg(debug_assertions)]
   {
       builder = builder.plugin(tauri_plugin_mcp_bridge::init());
   }
   ```

2. Activer `withGlobalTauri` dans `src-tauri/tauri.conf.json` et ajouter `"mcp-bridge:default"` aux permissions de `src-tauri/capabilities/default.json`.
3. Lancer `bun run tauri dev`, puis connecter une session (`driver_session`, port `9223`).

Une fois connecté, l'agent peut prendre des screenshots du webview, inspecter le DOM, exécuter des commandes IPC, suivre le trafic IPC et lire les logs console.

### Coffre et jetons de test locaux

Les tests de bout en bout se jouent dans un coffre **jetable**, jamais dans le
coffre réel : l'application est lancée avec un profil XDG dédié
(`XDG_DATA_HOME=/tmp/opencode/akooti-data`) et un mot de passe maître de test.
Les identifiants, les jetons MCP et la recette vivent dans deux fichiers **non
versionnés** (`.gitignore` : `*.local.txt`, `opencode.json`) :

- `TEST-VAULT.local.txt` — coffre de test, jetons MCP, recette et procédure de
  nettoyage ;
- `opencode.json` — serveur MCP `akooti` local au projet, pointé sur
  `target/debug/akooti mcp --token …`.

Deux points d'entretien, à traiter avant de relancer la recette :

1. **Le coffre de test vit dans `/tmp`, effacé à chaque redémarrage.** S'il a
   disparu, l'application propose « Nouveau coffre » : le recréer avec le mot de
   passe maître de `TEST-VAULT.local.txt`, puis recréer l'entrée de test et les
   jetons (le secret d'un jeton n'est affiché qu'une fois).
2. **Les jetons MCP test expirent** (1 jour pour les jetons restreints, 30 jours
   pour le jeton principal). Passé ce délai, tout appel répond
   `invalid_credentials` : réémettre un jeton dans **Accès IA** et mettre à jour
   `TEST-VAULT.local.txt` **et** `opencode.json`, qui doivent rester alignés.

### Qualité

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`
- `bun run lint`, `bun run typecheck`, `bun run test` (ou `bun --filter '*' test`)
- Les règles de sécurité de `docs/regles.md` sont **non négociables** : toute PR qui expose un secret dans une réponse MCP est refusée.

## Feuille de route

| Phase                        | Contenu                                                                                                                                                                  |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **0 — Fondations**           | Monorepo, workspace Rust/TS, CI, conventions, schémas SQL, bridge MCP Tauri                                                                                              |
| **1 — Noyau & coffre local** | `akooti-core` (Argon2id, XChaCha20, SQLCipher, multi-dossiers, générateur, TOTP), UI desktop minimale, importateurs Buttercup / Bitwarden / Proton + routage `Dev Local` |
| **2 — Pont IA, CLI & MCP**   | `tauri-plugin-cli`, daemon IPC `interprocess`, serveur MCP, jetons cloisonnés, audit, `SKILL.md`, CDP & `vault_exec`                                                     |
| **3 — Extension & audit**    | Native Messaging, injection aveugle par sélecteurs, passkeys `MAIN` world, tableau de bord Santé du Coffre (zxcvbn + HIBP)                                               |
| **4 — Sync & réversibilité** | Supabase Realtime E2EE, sauvegardes Cron (Local / S3 / Drive), export d'urgence, durcissement et packaging                                                               |

Le détail exécutable vit dans le projet Kaneo **AKO — Akooti**.

## Documentation

- [`docs/architecture.md`](./docs/architecture.md) — flux, frontières de confiance, structure du monorepo
- [`docs/fonctionalites.md`](./docs/fonctionalites.md) — périmètre fonctionnel complet
- [`docs/regles.md`](./docs/regles.md) — règles de sécurité, d'architecture et de phasage
- [`docs/schema.md`](./docs/schema.md) — schéma SQLCipher local et schéma Supabase E2EE
- [`docs/adr/`](./docs/adr/) — décisions d'architecture (monorepo, stockage, transport MCP)

## Contribuer

1. Ouvrir une issue ou reprendre une tâche du tableau Kaneo public :
   [kaneo.t2ol.org/public-project/ql4kabfti5m3hlbofdlb5bek](https://kaneo.t2ol.org/public-project/ql4kabfti5m3hlbofdlb5bek).
2. Créer une branche (`feat/…`, `fix/…`, `chore/…`).
3. Respecter `docs/regles.md` — en particulier les invariants Zero-Leak et `zeroize`.
4. Ajouter les tests du cœur métier dans `crates/akooti-core`.
5. Ouvrir une Pull Request avec une description du risque sécurité.

Le détail du processus (prérequis, branches, commits, checklist sécurité) est
dans [`CONTRIBUTING.md`](./CONTRIBUTING.md). Les vulnérabilités se signalent en
privé via les GitHub Security Advisories, jamais en issue publique.

## Licence

[MIT](./LICENSE) © 2026 DEV MASTER.
