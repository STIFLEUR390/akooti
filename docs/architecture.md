# Architecture

Akooti est un coffre-fort de mots de passe _local-first_ : un binaire Rust
unique, une base SQLCipher locale, et — à terme — une extension navigateur. Ce
document décrit les processus, les frontières de confiance, les flux de données
et la carte du code. Le périmètre fonctionnel est dans
[fonctionalites.md](./fonctionalites.md), les règles inviolables dans
[regles.md](./regles.md).

## 1. Vue d'ensemble

Un seul binaire `akooti`, plusieurs modes ([ADR 0001](./adr/0001-monorepo-rust-bun.md)) :

| Invocation                    | Rôle                                                                   | Statut |
| ----------------------------- | ---------------------------------------------------------------------- | ------ |
| `akooti`                      | Interface graphique Tauri + icône de notification + **daemon** local    | ✅     |
| `akooti mcp --token <jeton>`  | Serveur MCP sur stdio, lancé par l'agent (Claude Code, Codex, Cursor…) | ✅     |
| `akooti native-host`          | Pont Native Messaging pour l'extension navigateur                      | 🧭 phase 6 |
| `akooti help` / `version`     | Aide et version, aucun démarrage                                       | ✅     |

Principe directeur : **un seul processus détient la clé maître**. L'interface,
les sous-processus MCP et (demain) le pont navigateur n'y ont pas accès ; ils
demandent au daemon d'agir, et le daemon décide.

```text
+------------------------------------------------------------------------------------+
|                              SURFACES UTILISATEUR                                  |
|                                                                                    |
|   [ App desktop Tauri ]        [ Agent IA via MCP stdio ]      [ Extension WXT ]   |
|    React + TypeScript          Claude Code, Codex, Cursor…      Chrome/Edge/FF     |
+---------+----------------------------+-------------------------------+-------------+
          | Tauri IPC                  | JSON-RPC sur stdio            | Native Msg
          | (capabilities)             | (`akooti mcp --token …`)      | (phase 6)
          v                            v                               v
+---------+----------------------------+-------------------------------+-------------+
|                        BINAIRE UNIQUE RUST (`akooti`)                              |
|                                                                                    |
|   [ Interface + tray ]  <--- IPC local (socket Unix 0600) --->  [ Serveur MCP ]    |
|            |                        version 1, 1 Mio max             (sans clé)    |
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
                    |   Local : `vault.meta` + `vault.db`    |
                    +----------------------------------------+
```

Ce qui **n'existe pas encore** : synchronisation Supabase, sauvegardes
planifiées (S3 / Drive / local), audit de santé du coffre, passkeys, pont
Native Messaging et transport IPC Windows. Ils sont décrits comme cible et
marqués 🧭.

## 2. Processus et détention de la clé

| Processus                 | Détient le mot de passe maître ? | Peut lire un secret ?            | Peut écrire dans le coffre ? |
| ------------------------- | -------------------------------- | -------------------------------- | ---------------------------- |
| Binaire `akooti` (GUI)    | oui, tant que le coffre est ouvert | oui, via la fenêtre de révélation | oui                          |
| Serveur MCP (stdout)      | **non**                          | **non** — il ne reçoit que des issues de remplissage | non  |
| Pont Native Messaging     | **non** (même contrat que MCP)   | non                              | non                          |
| Navigateur cible (CDP)    | non                              | reçoit les valeurs dans le DOM   | —                            |
| Processus enfant (`vault_exec`) | non                        | reçoit des variables d'environnement, jamais sur son stdout | — |

Conséquences directes :

- un agent IA compromis ne peut pas extraire un mot de passe : il ne dispose que
  d'identifiants de métadonnées et de verbes d'action ;
- arrêter le binaire principal suffit à couper tout accès : le socket disparaît
  et les jetons deviennent inutilisables (réponse `daemon_unavailable`).

## 3. Cycle de vie du coffre

```text
création        `vault.meta` (sel + paramètres Argon2id) + `vault.db` (SQLCipher)
déverrouillage  mot de passe -> Argon2id -> clé de chiffrement SQLCipher
                l'en-tête public est relu et comparé aux paramètres stockés
session         la clé vit en RAM (`Zeroizing`), partagée par `Arc<VaultRuntime>`
verrouillage    inactivité 15 min, mise en veille, ou action explicite
                la clé est effacée ; les commandes répondent `locked`
```

- L'en-tête `vault.meta` est **public par conception** : il ne contient que ce
  qu'il faut pour dériver la clé ([ADR 0004](./adr/0004-vault-bootstrap-header.md)).
- Le shell Tauri ne fait que résoudre `app_data_dir` et le transmettre au cœur :
  aucun chemin n'est codé en dur dans `akooti-core`.
- Le verrouillage est piloté par le daemon, pas par l'interface : fermer la
  fenêtre ne déverrouille ni ne verrouille rien.

## 4. Flux de données

### 4.1 Recherche (`vault_search`)

```text
agent -> serveur MCP -> socket -> daemon -> akooti-core (mcp_search)
                                     -> métadonnées seulement (jamais de secret)
```

Chaque requête porte le jeton ; le daemon vérifie signature, expiration,
révocation, portées et dossiers autorisés **avant** toute lecture, puis écrit la
ligne d'audit.

### 4.2 Remplissage aveugle (`vault_cdp_autofill`)

```text
agent (itemId, origin, selectors?) -> daemon -> plan de remplissage (RAM)
   -> sélection de la cible CDP dont l'origine correspond EXACTEMENT
   -> programme JS injecté dans la page (username, password, totp, submit)
   -> compte-rendu : statut, champs remplis, soumission — aucune valeur
```

- `origin` non concordante : refus, audit `error`, aucune tentative.
- Sélecteurs stockés sur l'entrée, surchargés par ceux de l'agent quand il en
  fournit (`selectors`).
- Statuts : `filled`, `partial` (aucun sélecteur trouvé), `failed`,
  `unavailable` (extension non branchée).

### 4.3 Compte éphémère (`vault_create_and_fill`)

```text
agent -> daemon -> akooti-core : mot de passe généré (OsRng) + entrée créée
                -> remplissage du formulaire d'inscription (même moteur CDP)
                -> UNE ligne d'audit, écrite une fois l'issue connue
```

L'entrée porte `created_by_agent` et `expires_at` (TTL de 60 s à 30 jours) ; la
purge périodique supprime les comptes de test expirés.

### 4.4 Injection CLI (`vault_exec`)

```text
agent (itemId, argv[]) -> daemon -> variables d'environnement du processus enfant
   AKOOTI_USERNAME, AKOOTI_PASSWORD, AKOOTI_<CHAMP> pour chaque champ personnalisé
   -> stdout/stderr filtrés : chaque valeur injectée devient `***`
```

Aucun shell n'est interposé : `argv[0]` est le programme. Une variable n'est
développée que si l'agent demande explicitement un shell. Il n'existe
volontairement **pas** de `AKOOTI_TOTP`.

### 4.5 Audit

Toutes les actions IA passent par `mcp_audit_logs` : `vault_search`,
`cdp_autofill`, `create_and_fill`, `exec_env`, et `access_denied` pour les
refus. Une ligne ne contient jamais de valeur secrète, seulement une origine, un
identifiant technique et un diagnostic court.

## 5. Contrats techniques

| Contrat            | Règle                                                                                   |
| ------------------ | --------------------------------------------------------------------------------------- |
| Transport IPC      | socket Unix, permission `0600`, dossier réservé à l'utilisateur ([ADR 0006](./adr/0006-transport-ipc-socket-unix.md)) |
| Emplacement        | `AKOOTI_SOCKET` sinon `$XDG_RUNTIME_DIR/akooti/daemon.sock`                              |
| Cadrage            | JSON préfixé par une longueur de 4 octets (LE), plafond **1 Mio** par trame             |
| Version de protocole | `1` ; incrémentée dès qu'un champ change de sens                                       |
| Erreurs            | codes stables : `forbidden`, `locked`, `not_found`, `invalid_credentials`, `invalid_input`, `internal`, `daemon_unavailable` |
| Jetons MCP         | `ak_mcp_` + 64 caractères hexadécimaux ; stockés en SHA-256, comparés en temps constant  |
| Portées            | `read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`                             |
| Secrets en Rust    | `zeroize::Zeroizing` pour toute clé, dérivée ou valeur déchiffrée                        |

## 6. Carte du code

```text
akooti/
├── crates/
│   ├── akooti-core/            # métier pur : aucune dépendance Tauri / React
│   │   ├── migrations/001_init.sql
│   │   └── src/
│   │       ├── crypto.rs           Argon2id, XChaCha20-Poly1305, AAD, zeroize
│   │       ├── db.rs               ouverture SQLCipher, migrations, seeds
│   │       ├── header.rs           en-tête public `vault.meta`
│   │       ├── session.rs          cycle verrouillage / inactivité
│   │       ├── vault.rs            API métier du coffre
│   │       ├── folders.rs          dossiers système et personnels
│   │       ├── items/              CRUD, révélation, historique, maintenance
│   │       ├── routing/            règles `Dev Local` (suffixe, hôte, CIDR, regex)
│   │       ├── generator.rs        mots de passe OsRng, passphrases EFF
│   │       ├── totp.rs             codes TOTP (RFC 6238)
│   │       ├── importers/          Buttercup, Bitwarden, Proton, KeePass, 1Password, CSV
│   │       ├── mcp/                jetons (SHA-256, portées), store, audit
│   │       └── vault_mcp.rs        opérations cloisonnées + plans d'injection
│   ├── akooti-ipc/             # protocole local : framing, `Request`/`Response`, socket, client
│   └── akooti-mcp/             # serveur MCP stdio (rmcp) : six outils, zéro clé
├── src-tauri/                  # shell desktop : CLI, tray, daemon, commandes IPC
│   └── src/
│       ├── cli.rs                  routage des modes avant toute fenêtre
│       ├── headless.rs             modes `mcp` et `native-host`
│       ├── vault_runtime.rs        `Arc<VaultRuntime>`, verrou auto 15 min
│       ├── daemon/                 serveur IPC + inject.rs (CDP) + exec.rs (masquage)
│       └── commands/               surface IPC typée (vault, items, folders, generator,
│                                   routing, import/export, clipboard, mcp, system)
├── apps/
│   ├── desktop/                # React + Vite : unlock, coffre, générateur, import,
│   │                           # routage, page « Accès IA »
│   └── extension/              # WXT : service worker seul (phase 6)
└── packages/shared/            # types et protocoles TypeScript partagés
```

Les modules suivants sont **ciblés mais absents** et ne doivent pas être
supposés présents : `health.rs`, `passkeys.rs`, `sync_supabase.rs`, `backup/`,
`content.ts` / `webauthn.content.ts` de l'extension. Voir
[fonctionalites.md](./fonctionalites.md) pour leur statut.

## 7. Décisions structurantes

| ADR                                                     | Décision                                                    |
| ------------------------------------------------------- | ----------------------------------------------------------- |
| [0001](./adr/0001-monorepo-rust-bun.md)                 | Monorepo Cargo + Bun, un binaire multi-modes                |
| [0002](./adr/0002-sqlcipher-local-vault.md)             | Coffre local SQLCipher + chiffrement de champ XChaCha20     |
| [0003](./adr/0003-mcp-stdio-blind-injection.md)         | MCP stdio, daemon IPC et injection aveugle                  |
| [0004](./adr/0004-vault-bootstrap-header.md)            | En-tête public d'amorçage du coffre                         |
| [0005](./adr/0005-biometrie-deverrouillage.md)          | Biométrie : trait d'union présent, fournisseur non branché  |
| [0006](./adr/0006-transport-ipc-socket-unix.md)         | Socket Unix natif, named pipe Windows différé               |
| [0007](./adr/0007-fills-et-secrets-totp-courts.md)      | Remplissage d'un compte neuf, secrets TOTP courts, audit fidèle |

## 8. Limites connues

| Limite                                                        | Référence                          |
| ------------------------------------------------------------- | ---------------------------------- |
| `vault_blind_autofill` répond `unavailable` : l'extension n'est pas branchée | phase 6              |
| Pas de transport IPC Windows (named pipe)                     | [ADR 0006](./adr/0006-transport-ipc-socket-unix.md) |
| Aucun fournisseur biométrique branché ; l'interface masque le contrôle | [ADR 0005](./adr/0005-biometrie-deverrouillage.md) |
| Les secrets TOTP de moins de 128 bits sont acceptés (services réels) | [ADR 0007](./adr/0007-fills-et-secrets-totp-courts.md) |
| `SelectorOverride.submit: null` ne désactive pas la soumission (indistinguable d'« absent ») | `daemon/inject.rs` |
| Synchronisation, sauvegardes, audit de santé, passkeys : non implémentés | phases ultérieures |
