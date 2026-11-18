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

Statuts : ✅ livré et testé · 🧭 prévu (conception arrêtée) · ⚠️ limite connue.
Le détail par fonction est dans [`docs/fonctionalites.md`](./docs/fonctionalites.md).

### Coffre & organisation

- ✅ **Multi-dossiers (N:N)** — une entrée appartient à plusieurs dossiers sans duplication ; toute modification est répercutée partout (table de liaison `item_folders`).
- ✅ **Dossiers système protégés** — `Personnel`, `Dev Local` et `Agents & MCP` sont créés automatiquement, non supprimables et non renommables.
- ✅ **Comptes éphémères (TTL)** — les comptes de test créés par un agent expirent et sont purgés automatiquement.
- ✅ **Historique des mots de passe** — versions précédentes conservées chiffrées sur chaque fiche.
- ✅ **URIs multiples et champs personnalisés** — avec politiques de correspondance (`domain`, `host`, `starts_with`, `exact`, `regex`).
- ✅ **Presse-papiers sûr** — effacement automatique 30 s après copie d'un secret ou d'un code TOTP.

### Authentification moderne

- ✅ **TOTP intégré** — codes à 6 chiffres, SHA-1 / SHA-256, affichage temps réel, copie rapide et remplissage aveugle par le MCP (CDP).
- ✅ **Passkeys (WebAuthn / FIDO2)** — le coffre crée et signe les credentials lui-même : clé P-256 chiffrée sous la clé maître, signature ES256, `clientDataJSON` reconstruit à partir de l'origine réelle de la page. Une cérémonie qu'il ne peut pas servir retombe sur le navigateur (voir [`docs/adr/0008`](./docs/adr/0008-extension-native-messaging.md)).
- ⚠️ **Ce que ces passkeys ne font pas** — aucune attestation (`fmt: "none"`), aucune vérification de l'utilisateur (drapeau `UP` seul) et aucun credential découvrable : un site qui exige `userVerification: "required"` ou `residentKey: "required"` reçoit l'authentificateur du navigateur (voir [`docs/architecture.md`](./docs/architecture.md) §4.6).
- ⚠️ **Biométrie native** — trait d'union présent (Windows Hello, Touch ID, Linux Polkit / FIDO2), aucun fournisseur branché : l'interface masque le contrôle (voir [`docs/adr/0005`](./docs/adr/0005-biometrie-deverrouillage.md)).

### Routage & import universel

- ✅ **Import** — Buttercup (`.bcup`, `.csv`), Bitwarden / Vaultwarden (`.json` chiffré ou clair), Proton Pass (`.json`, `.zip`), KeePass / KeePassXC (`.kdbx` v3/v4, `.xml`, `.csv`), 1Password (`.1pux`, `.csv`), CSV navigateurs (Chrome, Brave, Edge, Firefox) — 10 formats plus la détection automatique.
- ✅ **Routage `Dev Local`** — détection automatique de `localhost`, `127.0.0.1`, `.local`, `.localhost`, `.test` et de suffixes personnalisés (`*.docker`, `*.internal`…), à l'import comme en rétroactif, **sans retirer** l'entrée de son dossier d'origine.
- ✅ **Hygiène post-import** — aperçu, détection de doublons, effacement sécurisé du fichier source en clair après validation de la transaction.

### Intégration IA

- ✅ **Serveur MCP stdio** — `akooti mcp --token <clé>` exposé aux agents, sans accès à la clé maître.
- ✅ **Jetons cloisonnés par dossier (RBAC)** — permissions granulaires (`read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`), TTL, révocation immédiate, journal d'audit local.
- ✅ **Injection aveugle par CDP** — remplissage d'une page pilotée par IA via Chrome DevTools Protocol, origine vérifiée strictement, aucune valeur retournée.
- ✅ **Pont navigateur (Native Messaging)** — l'hôte natif recopie les trames entre l'extension et le daemon **sans jamais les décoder** ; la popup remplit, copie et capture les sélecteurs sans passer par un agent ([`docs/adr/0008`](./docs/adr/0008-extension-native-messaging.md)).
- ⚠️ **Injection pilotée par un agent via l'extension** — le pont ne sert que les remplissages initiés par l'extension : `vault_blind_autofill` répond `unavailable`, utiliser `vault_cdp_autofill`.
- ✅ **`vault_exec`** — secrets injectés dans la mémoire d'un processus enfant (`docker compose up`, scripts de test), avec masquage automatique dans `stdout` / `stderr`.
- ✅ **Agent Skill embarqué** — `skills/akooti/SKILL.md` guide les agents sur l'usage sûr du MCP et de la CLI.

### Santé du coffre

- ✅ **Audit local (offline)** — score `zxcvbn` par entrée (dictionnaires, dates, suites clavier, pseudo et titre comme entrées utilisateur), réutilisation entre comptes et ancienneté paramétrable, sans aucune requête réseau.
- ✅ **Fuites sans fuite** — vérification HIBP par _k-Anonymity_ sur action explicite : seuls les 5 premiers caractères du SHA-1 quittent la machine, la comparaison des suffixes reste locale ; hors ligne, le résultat reste « inconnu ».
- ✅ **Tableau de bord** — synthèse, filtres, accès direct à l'entrée, remédiation par le générateur et l'historique existants, export CSV sans secret.

### Sync & sauvegardes

- ✅ **Sync temps réel Supabase (BYO)** — guide de configuration : [`docs/guides/supabase.md`](./docs/guides/supabase.md) ; chaque folder, entrée, passkey et règle de routage est sérialisé puis chiffré en XChaCha20 côté client ; le serveur ne voit que `ciphertext`, `nonce` et des métadonnées techniques. RLS par utilisateur, Realtime avec reconnexion et resynchronisation.
- ✅ **Conflits explicites** — LWW sur l'horodatage client, tombstones pour les suppressions, donc rejouer un lot est idempotent.
- ✅ **Sauvegardes programmées** — guides : [`S3`](./docs/guides/s3.md), [`Google Drive`](./docs/guides/google-drive.md) ; instantanés `.akooti` (les fichiers du coffre, déjà chiffrés) vers dossier local / NAS, S3 compatible (AWS, R2, MinIO, Wasabi) ou Google Drive (refresh token), rétention et historique des exécutions.
- ✅ **Réversibilité** — export d'urgence JSON en clair après confirmation du mot de passe maître, et restauration d'un instantané `.akooti` depuis l'écran de déverrouillage.
- ⚠️ **Recette externe** — Supabase est vérifié de bout en bout sur un projet réel (push, pull, tombstone, Realtime, RLS) ; S3 est vérifié sur un MinIO local (envoi, rétention, téléchargement) ; Google Drive reste couvert hors ligne tant qu'il n'est pas testé sur un compte réel.

## Architecture

Un **binaire unique Rust (`akooti`)** embarque l'interface GUI, le daemon, le pont navigateur et le serveur MCP. Les sous-processus n'ont jamais la clé maître : ils passent par un socket local restreint à l'utilisateur OS.

```text
+------------------------------------------------------------------------------------+
|                              SURFACES UTILISATEUR                                  |
|                                                                                    |
|   [ App desktop Tauri ]        [ Agent IA via MCP stdio ]      [ Extension WXT ]   |
|    React + TypeScript          Claude Code, Codex, Cursor…      Chrome/Edge/FF     |
+---------+----------------------------+-------------------------------+-------------+
          | Tauri IPC                  | JSON-RPC sur stdio            | Native Msg
          | (capabilities)             | (`akooti mcp --token …`)      | (hôte natif)
          v                            v                               v
+---------+----------------------------+-------------------------------+-------------+
|                        BINAIRE UNIQUE RUST (`akooti`)                              |
|                                                                                    |
|   [ Interface + tray ]  <--- IPC local (socket Unix 0600) --->  [ Serveur MCP ]    |
|            |                        version 2, 1 Mio max             (sans clé)    |
|            +------------------------------+-----------------------------+          |
|                                           v                                        |
|                        +------------------------------------+                      |
|                        |   Daemon : seul détenteur de la clé |                     |
|                        |   `Arc<VaultRuntime>` partagé       |                     |
|                        |   verrou auto après 15 min          |                     |
|                        +----------------+-------------------+                      |
+-----------------------------------------|------------------------------------------+
                                          v
                         +----------------------------------+
                         |          `akooti-core`           |
                         | Argon2id + XChaCha20-Poly1305    |
                         | SQLCipher, dossiers N:N, TOTP    |
                         | jetons MCP, audit, routage       |
                         +----------------+-----------------+
                                          v
                    +----------------------------------------+
                    |            Local (source de vérité)    |
                    |      `vault.meta` + `vault.db`         |
                    +----------------------------------------+
```

La synchronisation Supabase et les sauvegardes planifiées sont **conçues mais
non implémentées** (🧭) : aucune connexion réseau n'est ouverte aujourd'hui,
tout reste sur l'appareil.

**Principes structurants :**

1. **Cœur métier isolé** — `akooti-core` n'importe aucune dépendance Tauri, WebView ou React (testable, portable vers le mobile).
2. **Un binaire, plusieurs modes** — `akooti` (GUI + daemon), `akooti mcp --token` (serveur MCP headless), `akooti native-host` (pont WXT), `akooti native-register` (manifestes Native Messaging), `akooti help` / `version`.
3. **Une seule clé** — le daemon la détient ; l'interface, le serveur MCP et les sous-processus n'y ont jamais accès.
4. **Secret jamais dans le contexte LLM** — la frontière MCP ne transporte que des métadonnées et des résultats d'action.
5. **Socket local privé** — `0600` dans un dossier réservé à l'utilisateur, aucun port TCP, aucun serveur HTTP local.

## Stack technique

| Domaine             | Choix                                                                                                |
| ------------------- | ---------------------------------------------------------------------------------------------------- |
| Application desktop | Tauri v2 (Rust + React + TypeScript + Tailwind CSS)                                                  |
| CLI / daemon        | `tauri-plugin-cli`, daemon IPC (socket Unix `0600`, named pipe Windows à venir), tray                |
| Extension           | WXT (TypeScript / React), MV3 (Chrome / Brave / Edge) et MV2 (Firefox), shim WebAuthn en monde MAIN  |
| Gestion des paquets | Bun 1.4+ (workspaces natifs, `bun.lock` commité)                                                     |
| Base locale         | SQLCipher (via `rusqlite`) + migrations SQL versionnées                                              |
| Cryptographie       | Argon2id (KDF), XChaCha20-Poly1305 (AEAD), `zeroize`, `OsRng`                                        |
| Audit               | `zxcvbn` (offline), réutilisation, ancienneté, HIBP _k-Anonymity_                                    |
| Sync                | Supabase Realtime (WebSocket) + RLS stricte, payloads E2EE, LWW + tombstones                         |
| Sauvegardes         | Planificateur interne (60 s de tick), local / S3 SigV4 / Google Drive OAuth2, rétention `.akooti`    |
| MCP                 | SDK Rust `rmcp` (transport stdio)                                                                    |
| Tests app Tauri     | [MCP Server Tauri](https://github.com/hypothesi/mcp-server-tauri) (plugin `tauri-plugin-mcp-bridge`) |

## Structure du dépôt

```text
akooti/
├── Cargo.toml                     # Workspace Rust (crates/* + src-tauri)
├── package.json                   # Workspace Bun (apps/*, packages/*)
├── bun.lock                       # Lockfile Bun (commité)
├── docs/                          # Architecture, fonctionnalités, règles, schémas, tests, ADR
├── skills/akooti/SKILL.md         # Guide d'usage MCP/CLI pour les agents
│
├── crates/
│   ├── akooti-core/               # Métier pur : crypto, SQLCipher, dossiers, items, routage,
│   │   │                          # générateur, TOTP, importateurs, jetons MCP, audit
│   │   ├── migrations/001_init.sql
│   │   └── src/
│   ├── akooti-ipc/                # Protocole local : framing, Request/Response, socket, client
│   └── akooti-mcp/                # Serveur MCP stdio (rmcp) : six outils, zéro clé
│
├── src-tauri/                     # GUI, CLI multi-modes, daemon IPC, tray, biométrie
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   └── src/
│       ├── cli.rs, headless.rs    # routage des modes, modes sans fenêtre
│       ├── native_host.rs         # pont Native Messaging : pompe à trames, zéro décodage
│       ├── native_register.rs     # manifestes par navigateur (install, check, uninstall)
│       ├── vault_runtime.rs       # Arc<VaultRuntime>, verrouillage auto
│       ├── daemon/                # serveur IPC + inject.rs (CDP) + exec.rs (masquage)
│       └── commands/              # surface IPC typée
│
├── packages/shared/               # Contrats TypeScript partagés (modèles + protocole)
│
└── apps/
    ├── desktop/                   # Interface React (coffre, générateur, import, routage, Accès IA)
    └── extension/                 # Extension WXT : service worker, popup, script de contenu, shim WebAuthn
```

## Modèle de sécurité

| Menace                                               | Contre-mesure                                                                                                                              |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| LLM qui lit un secret                                | Les tools MCP ne renvoient que des métadonnées ; l'injection est aveugle                                                                   |
| Secret en RAM après usage                            | `zeroize::Zeroizing<T>` systématique + verrouillage auto (15 min / veille)                                                                 |
| Lecture du socket IPC par un autre utilisateur       | Socket `0600` dans un dossier privé ; named pipe Windows à venir ; aucun port TCP                                                          |
| Exfiltration côté cloud (🧭, quand la sync existera) | E2EE client XChaCha20-Poly1305 ; le serveur ne voit que `ciphertext` + `nonce`                                                             |
| Jeton MCP trop permissif                             | Scopes granulaires + liste blanche de `folder_id` + TTL + audit                                                                            |
| Fichier d'import en clair laissé sur le disque       | Proposition d'effacement sécurisé après commit SQLCipher                                                                                   |
| Presse-papiers qui fuit                              | Effacement automatique après 30 s                                                                                                          |
| Site tiers qui se fait passer pour l'extension       | Manifeste `allowed_origins` limité à l'id exact de l'extension ; toute connexion qui ne s'annonce pas `native-host` reçoit `access_denied` |
| Page hostile qui déclenche une cérémonie WebAuthn    | Shim injecté sous nonce, garde de `rpId`, aucune signature possible côté coffre aujourd'hui                                                |

Règles détaillées : [`docs/regles.md`](./docs/regles.md).

## Serveur MCP & intégration IA

Chaque jeton est limité à un ou plusieurs dossiers et à des permissions explicites.

| Outil                   | Rôle                                                                                            | Permission        |
| ----------------------- | ----------------------------------------------------------------------------------------------- | ----------------- |
| `vault_status`          | Décrit le jeton courant : nom, portées, dossiers autorisés et expiration                        | —                 |
| `vault_search`          | Recherche par titre / URL / identifiant — **métadonnées seules**                                | `read_metadata`   |
| `vault_blind_autofill`  | Remplissage par l'extension WXT, réservé au pont natif : un agent reçoit `unavailable`          | `blind_fill`      |
| `vault_cdp_autofill`    | Remplit un navigateur piloté par IA via Chrome DevTools Protocol                                | `blind_fill`      |
| `vault_create_and_fill` | Génère un mot de passe, crée l'entrée (dossier autorisé du jeton, TTL) et remplit l'inscription | `create_and_fill` |
| `vault_exec`            | Lance une commande avec les secrets injectés en variables d'environnement, logs masqués         | `exec_env`        |

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

**État d'implémentation (phases 5 et 6)**

- ✅ Le serveur MCP stdio (`rmcp`), les jetons cloisonnés, l'audit et le daemon
  IPC local sont opérationnels : `akooti mcp --token …` parle au daemon par un
  socket Unix `0600` (`$XDG_RUNTIME_DIR/akooti/daemon.sock`).
- ✅ `vault_cdp_autofill`, `vault_create_and_fill` et `vault_exec` sont branchés.
- ✅ Le pont navigateur est livré : `akooti native-host` recopie les trames entre
  l'extension et le daemon sans les décoder, `akooti native-register` écrit le
  manifeste de chaque navigateur, et le verbe `vaultExtensionFill` sert les
  remplissages initiés par la popup
  ([`docs/adr/0008`](./docs/adr/0008-extension-native-messaging.md)).
- ⚠️ `vault_blind_autofill` répond toujours `unavailable` à un agent : le pont
  ne s'ouvre qu'à une connexion qui s'est annoncée `native-host`. Un agent passe
  par `vault_cdp_autofill`.
- ⚠️ Les passkeys sont servies par le coffre, mais **sans attestation ni
  vérification de l'utilisateur** (`fmt: "none"`, drapeau `UP` seul) et jamais
  comme credentials découvrables : une cérémonie qui exige `UV` ou
  `residentKey: "required"` est rendue au navigateur
  ([`docs/architecture.md`](./docs/architecture.md) §4.6).

## Démarrage rapide

### Prérequis

- Rust 1.90+ et Cargo
- Bun 1.4+ (runtime, gestionnaire de paquets et workspaces)
- Dépendances système Tauri v2 ([guide officiel](https://tauri.app/start/prerequisites/))
- SQLCipher compilé par `rusqlite` (`features = ["bundled-sqlcipher"]`) : prévoir
  `build-essential`, `pkg-config` et `libssl-dev`, mais aucune bibliothèque
  SQLCipher à installer à la main

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
akooti mcp --token ak_mcp_...   # Serveur MCP stdio pour les agents
akooti native-host              # Pont Native Messaging (lancé par le navigateur)
akooti native-register --browser chrome --extension-id <id>   # manifeste de l'hôte
akooti help | version           # Aide et version, sans démarrer l'interface
```

## Développement

### Tester et déboguer l'application Tauri

Le pont est **déjà câblé** dans ce dépôt et ne s'active qu'en build debug : il
n'existe pas dans un binaire de production.

| Élément                                                | Emplacement                                                                    |
| ------------------------------------------------------ | ------------------------------------------------------------------------------ |
| Dépendance (`tauri-plugin-mcp-bridge = "0.13"`)        | [`src-tauri/Cargo.toml`](./src-tauri/Cargo.toml)                               |
| Plugin, `#[cfg(debug_assertions)]`, écoute `127.0.0.1` | [`src-tauri/src/lib.rs`](./src-tauri/src/lib.rs)                               |
| Permission `mcp-bridge:default`                        | [`src-tauri/capabilities/default.json`](./src-tauri/capabilities/default.json) |
| `withGlobalTauri`                                      | [`src-tauri/tauri.conf.json`](./src-tauri/tauri.conf.json)                     |

Mise en route :

```bash
bun run tauri dev            # ouvre l'app avec le pont sur 127.0.0.1:9223
```

Puis, depuis un client connecté au pont : `driver_session(action="start")`,
`webview_dom_snapshot(windowId="main")`, `webview_screenshot(windowId="main")`.
L'agent peut alors capturer le webview, inspecter le DOM (y compris les libellés
accessibles), appeler des commandes IPC et lire les logs console. La méthode
complète de recette est dans [`docs/desktop-tests.md`](./docs/desktop-tests.md).

### Recette dans un coffre jetable

Les tests de bout en bout se jouent dans un coffre **jetable**, jamais dans le
coffre réel : l'application est lancée avec un profil XDG dédié
(`XDG_DATA_HOME=/tmp/…`) et un mot de passe maître de test. Les coffres de test,
les jetons MCP et les configurations locales d'agents restent **hors du dépôt**
(`.gitignore` : `*.local.txt`, `opencode.json`) — un jeton poussé sur GitHub est
un incident, même sur un coffre jetable.

Chacun entretient son propre coffre de test : mot de passe, jetons et recette
vivent dans un fichier local non versionné, jamais dans ce README.

### Qualité

- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`
- `bun run lint`, `bun run typecheck`, `bun run test` (ou `bun --filter '*' test`)
- Les règles de sécurité de `docs/regles.md` sont **non négociables** : toute PR qui expose un secret dans une réponse MCP est refusée.
- Stratégie et couverture : [`docs/testing.md`](./docs/testing.md) ; recette manuelle dans le binaire réel : [`docs/desktop-tests.md`](./docs/desktop-tests.md).

## Feuille de route

| Phase                            | Contenu                                                                                                                                                                                             | État |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| **0 — Fondations**               | Monorepo Cargo + Bun, CI (format, lint, tests, build desktop), conventions, schémas SQL, bridge MCP Tauri                                                                                           | ✅   |
| **1 — Cryptographie & stockage** | `akooti-core` : Argon2id, XChaCha20-Poly1305, SQLCipher, en-tête d'amorçage, dossiers système, session et verrouillage automatique                                                                  | ✅   |
| **2 — Fonctions du coffre**      | Générateur (mots de passe, passphrases EFF), TOTP, routage `Dev Local`, historique, comptes éphémères                                                                                               | ✅   |
| **3 — Import universel**         | Buttercup, Bitwarden / Vaultwarden, Proton Pass, KeePass / KeePassXC, 1Password, CSV navigateurs ; aperçu, doublons, effacement de la source                                                        | ✅   |
| **4 — Application desktop**      | Interface Tauri v2 : déverrouillage, coffre, générateur, import, routage, page « Accès IA » ; accessibilité `axe-core`                                                                              | ✅   |
| **5 — Pont IA**                  | CLI multi-modes, daemon IPC `0600`, serveur MCP stdio (6 outils), jetons cloisonnés, audit, `SKILL.md`, CDP & `vault_exec`                                                                          | ✅   |
| **6 — Extension & passkeys**     | Extension WXT multi-navigateurs, pont Native Messaging, `vaultExtensionFill`, popup (remplissage, copie, capture), shim WebAuthn `world: MAIN`, authentificateur côté coffre (ES256, `fmt: "none"`) | ✅   |
| **7 — Santé du coffre**          | Audit local `zxcvbn` (score, réutilisation, ancienneté), vérification HIBP _k-Anonymity_ sur action explicite, tableau de bord et export CSV                                                        | ✅   |
| **8+ — Sync & réversibilité**    | Sync Supabase E2EE, sauvegardes planifiées (local / S3 / Drive), export d'urgence                                                                                                                   | 🧭   |

Le détail exécutable vit dans le projet Kaneo **AKO — Akooti** ; le périmètre
fonctionnel avec les statuts est dans
[`docs/fonctionalites.md`](./docs/fonctionalites.md).

## Documentation

- [`docs/README.md`](./docs/README.md) — index de la documentation et conventions d'écriture
- [`docs/architecture.md`](./docs/architecture.md) — processus, frontières de confiance, flux, carte du code
- [`docs/fonctionalites.md`](./docs/fonctionalites.md) — périmètre fonctionnel, fonction par fonction, avec statut
- [`docs/regles.md`](./docs/regles.md) — règles normatives : sécurité, données, architecture, IA, UX, périmètre
- [`docs/schema.md`](./docs/schema.md) — schéma SQLCipher local et conception Supabase E2EE
- [`docs/testing.md`](./docs/testing.md) — stratégie de test, couverture des contrats critiques, CI
- [`docs/desktop-tests.md`](./docs/desktop-tests.md) — plan de recette manuelle piloté par le pont MCP Tauri
- [`docs/adr/`](./docs/adr/) — décisions d'architecture datées (monorepo, stockage, transport, remplissage, pont navigateur)

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
