# Architecture

Akooti est un coffre-fort de mots de passe _local-first_ : un binaire Rust
unique, une base SQLCipher locale, et une extension navigateur qui parle au
daemon par Native Messaging. Ce document décrit les processus, les frontières de
confiance, les flux de données et la carte du code. Le périmètre fonctionnel est
dans [fonctionalites.md](./fonctionalites.md), les règles inviolables dans
[regles.md](./regles.md).

## 1. Vue d'ensemble

Un seul binaire `akooti`, plusieurs modes ([ADR 0001](./adr/0001-monorepo-rust-bun.md)) :

| Invocation                    | Rôle                                                                   | Statut |
| ----------------------------- | ---------------------------------------------------------------------- | ------ |
| `akooti`                      | Interface graphique Tauri + icône de notification + **daemon** local    | ✅     |
| `akooti mcp --token <jeton>`  | Serveur MCP sur stdio, lancé par l'agent (Claude Code, Codex, Cursor…) | ✅     |
| `akooti native-host`          | Pont Native Messaging pour l'extension navigateur                      | ✅     |
| `akooti native-register`      | Installe les manifestes Native Messaging par navigateur                | ✅     |
| `akooti help` / `version`     | Aide et version, aucun démarrage                                       | ✅     |

Principe directeur : **un seul processus détient la clé maître**. L'interface,
les sous-processus MCP et le pont navigateur n'y ont pas accès ; ils demandent au
daemon d'agir, et le daemon décide.

```text
+------------------------------------------------------------------------------------+
|                              SURFACES UTILISATEUR                                  |
|                                                                                    |
|   [ App desktop Tauri ]        [ Agent IA via MCP stdio ]      [ Extension WXT ]   |
|    React + TypeScript          Claude Code, Codex, Cursor…      Chrome/Edge/FF     |
+---------+----------------------------+-------------------------------+-------------+
          | Tauri IPC                  | JSON-RPC sur stdio            | Native Msg
          | (capabilities)             | (`akooti mcp --token …`)      | (`org.akooti.host`)
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
                    |   Local : `vault.meta` + `vault.db`    |
                    +----------------------------------------+
```

Ce qui **n'existe pas encore** : transport IPC Windows, packaging et mises à
jour. Ils sont décrits comme cible et marqués 🧭.

## 2. Processus et détention de la clé

| Processus                 | Détient le mot de passe maître ? | Peut lire un secret ?            | Peut écrire dans le coffre ? |
| ------------------------- | -------------------------------- | -------------------------------- | ---------------------------- |
| Binaire `akooti` (GUI)    | oui, tant que le coffre est ouvert | oui, via la fenêtre de révélation | oui                          |
| Serveur MCP (stdout)      | **non**                          | **non** — il ne reçoit que des issues de remplissage | non  |
| Pont Native Messaging     | **non** (même contrat que MCP)   | **non** — il recopie des trames sans les désérialiser | non                          |
| Extension navigateur      | non                              | reçoit les valeurs qu'elle injecte, sans les conserver | une ligne `passkeys` par cérémonie WebAuthn, jamais un item |
| Navigateur cible (CDP)    | non                              | reçoit les valeurs dans le DOM   | —                            |
| Processus enfant (`vault_exec`) | non                        | reçoit des variables d'environnement, jamais sur son stdout | — |

Conséquences directes :

- un agent IA compromis ne peut pas extraire un mot de passe : il ne dispose que
  d'identifiants de métadonnées et de verbes d'action ;
- arrêter le binaire principal suffit à couper tout accès : le socket disparaît
  et les jetons deviennent inutilisables (réponse `daemon_unavailable`) ;
- seul le pont peut recevoir des valeurs, et seulement pour les remettre à une
  page : `vaultExtensionFill` refuse toute connexion qui ne s'est pas annoncée
  `native-host` ([ADR 0008](./adr/0008-extension-native-messaging.md)).

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
  `unavailable` (côté agent : le pont natif ne lui ouvre pas ce verbe, voir
  §4.5).

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

### 4.5 Remplissage piloté par l'extension (`vaultExtensionFill`)

```text
popup -> service worker -> pont natif -> socket -> daemon
   -> jeton, portée `blind_fill`, dossiers autorisés, origine de l'onglet vérifiée
   -> `FillPayload` (RAM, `Zeroizing`) : username, password, totp, sélecteurs
   -> service worker -> content script (ISOLATED) -> page
   -> compte-rendu : champs remplis, soumission — aucune valeur
```

- Le verbe n'accepte que le client `native-host` ; une connexion `mcp` reçoit
  `forbidden` et une ligne `access_denied`, avant tout déchiffrement.
- `target` distingue `fill` (injection dans la page) et `clipboard` (copie
  éphémère, effacée après 30 s côté content script) ; il n'apparaît dans l'audit
  que comme un mot.
- Une origine non concordante est refusée comme pour `vault_cdp_autofill` :
  l'extension ne peut pas vider un coffre page par page.
- Le serveur MCP traduit une réponse `extensionFill` en erreur d'outil
  `forbidden` : le contrat de l'agent ne sait pas lire une valeur
  ([ADR 0008](./adr/0008-extension-native-messaging.md)).

### 4.6 Cérémonie WebAuthn servie par le coffre (`vaultPasskeyRegister`, `vaultPasskeyAssert`)

```text
page -> shim (MAIN) -> content script (ISOLATED) -> service worker -> pont -> daemon
   -> jeton, portée `blind_fill`, client `native-host`, origine sérialisée vérifiée
   -> relying party rattachée à l'origine, challenge >= 16 octets
   -> clé privée P-256 déchiffrée (AAD `akooti:v1:passkey:<id>`), signature ES256
   -> credential / signature : données publiques, rien d'autre
```

- Le verbe n'existe que pour le client `native-host` : une connexion `mcp`
  reçoit `forbidden` et une ligne `access_denied`. Un agent ne peut donc pas
  transformer le coffre en oracle de signature, et aucun outil MCP n'expose le
  verbe.
- L'authentificateur est **honnête sur ce qu'il est** : `fmt: "none"` (aucune
  attestation revendiquée), drapeau `UP` seul — jamais `UV`, puisque le coffre
  s'ouvre avec un mot de passe et n'est pas un capteur — et aucun credential
  découvrable, faute de sélecteur de compte côté daemon. Une cérémonie qui exige
  `userVerification: "required"` ou `residentKey: "required"` est rendue au
  navigateur par l'extension, avant tout aller-retour ; le daemon, lui, ne pose
  jamais `UV`, donc un site qui regarde les drapeaux voit la vérité.
- La clé privée ne se stocke pas telle quelle : elle est chiffrée en
  XChaCha20-Poly1305 sous une AAD qui contient l'identifiant du credential, si
  bien qu'une ligne déplacée sur un autre credential ne s'authentifie plus. La
  clé publique n'est pas stockée : elle se redérive de la clé privée, donc il n'y
  a qu'un secret à protéger au lieu de deux qui pourraient diverger.
- Rien de ce qui traverse le fil n'est secret : `clientDataJSON`, `attestationObject`,
  `authenticatorData` et la signature sont exactement ce que le site s'attend à
  recevoir.

### 4.7 Audit

Toutes les actions IA passent par `mcp_audit_logs` : `vault_search`,
`cdp_autofill`, `blind_autofill`, `create_and_fill`, `exec_env`, `passkey_use`
pour les cérémonies WebAuthn servies à l'extension, et `access_denied` pour les
refus. Une ligne ne contient jamais de valeur secrète,
seulement une origine, un identifiant technique et un diagnostic court.

### 4.8 Santé du coffre

```text
UI « Analyser le coffre » -> commande `audit_health` -> akooti-core
   -> déchiffrement en RAM, score zxcvbn (pseudo + titre en entrées utilisateur)
   -> SHA-1 tronqué à 5 caractères, HMAC-SHA256 de la clé maître pour la réutilisation
   -> zxcvbn_score / password_sha1_prefix persistés, rapport sans aucun secret

UI « Vérifier les fuites (HIBP) » -> commande `check_breaches`
   -> shell : GET api.pwnedpasswords.com/range/{préfixe} (Add-Padding, cache mémoire 24 h)
   -> suffixes renvoyés au cœur : comparaison locale, `is_compromised` mis à jour
```

- Aucun code réseau dans `akooti-core` (règle SEC-5) : le cœur ne manipule que
  des préfixes qu'il a calculés et des suffixes reçus. Une requête qui échoue
  laisse le drapeau inchangé et compte « inconnu » — un poste hors ligne n'est
  jamais bloqué.
- Les deux étapes sont des gestes explicites : elles déchiffrent chaque mot de
  passe, et la seconde envoie un préfixe à un tiers (règle SEC-11).
- L'audit signale sa progression par entrée (`on_progress`) et tourne sur un
  thread de travail : la webview affiche l'avancement au lieu de geler.
- Le seuil d'ancienneté est une préférence d'interface (`localStorage`), pas une
  donnée du coffre ; le rapport renvoie `age_days` et laisse l'UI filtrer.

### 4.9 Synchronisation et sauvegardes

```text
UI « Synchroniser » -> commande sync_now -> akooti-core : sérialise et chiffre
   chaque folder/item/passkey/routing_rule (XChaCha20, AAD type+id)
   -> shell : POST /rest/v1/akooti_sync_records (on_conflict user_id,id)
   -> GET deltas depuis sync_config.cursor -> apply_sync_records (LWW + tombstones)

Realtime -> wss://<projet>/realtime/v1/websocket -> événement postgres_changes
   -> pull du delta (reconnexion avec backoff, resynchronisation après coupure)

Planificateur (thread, tick 60 s) -> cibles dues -> write_snapshot(.akooti)
   -> pilote local / S3 / Drive -> rétention -> backup_runs + événement
```

- Le serveur ne reçoit que des blobs : ni titre, ni URL, ni nom de dossier, ni
  nombre d'objets (règle SEC-12). L'enveloppe publique (sel Argon2id + clé
  encapsulée) permet à un autre appareil de dériver la même clé.
- Le conflit se résout sur `client_updated_at` : un enregistrement strictement
  plus ancien est ignoré, une égalité laisse gagner l'arrivant, donc rejouer un
  lot est idempotent et une suppression de la même seconde se propage.
- Les suppressions voyagent en tombstones (`is_deleted`), sans révéler ce qui a
  été supprimé.
- Une sauvegarde ne chiffre rien de neuf : le fichier `.akooti` contient
  `vault.meta` et `vault.db` déjà chiffrés, après un `wal_checkpoint`.
- La restauration exige un coffre verrouillé : remplacer les fichiers sous une
  connexion SQLCipher ouverte laisserait l'application avec une clé orpheline.
- S3 est signé en SigV4 (path-style) et Drive utilise un refresh token fourni
  par l'utilisateur : aucun secret OAuth n'est stocké ailleurs que dans la base
  locale, chiffré par la clé maître.


## 5. Contrats techniques

| Contrat            | Règle                                                                                   |
| ------------------ | --------------------------------------------------------------------------------------- |
| Transport IPC      | socket Unix, permission `0600`, dossier réservé à l'utilisateur ([ADR 0006](./adr/0006-transport-ipc-socket-unix.md)) |
| Emplacement        | `AKOOTI_SOCKET` sinon `$XDG_RUNTIME_DIR/akooti/daemon.sock`                              |
| Cadrage            | JSON préfixé par une longueur de 4 octets (LE), plafond **1 Mio** par trame             |
| Version de protocole | `2` ; incrémentée dès qu'un champ change de sens, `camelCase` sur tous les champs      |
| Erreurs            | codes stables : `forbidden`, `locked`, `not_found`, `invalid_credentials`, `invalid_input`, `internal`, `daemon_unavailable`, `version_mismatch` |
| Jetons MCP         | `ak_mcp_` + 64 caractères hexadécimaux ; stockés en SHA-256, comparés en temps constant  |
| Portées            | `read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`                             |
| Secrets en Rust    | `zeroize::Zeroizing` pour toute clé, dérivée ou valeur déchiffrée                        |
| Pont navigateur    | hôte `org.akooti.host`, stdin/stdout en trames Native Messaging, **aucun désérialisation** ; manifeste par navigateur écrit par `akooti native-register` ([ADR 0008](./adr/0008-extension-native-messaging.md)) |
| Extension          | `@wxt-dev/module-react`, service worker seul client du pont, contenu ISOLATED pour écrire dans la page, shim WebAuthn en monde MAIN injecté avec un nonce |
| Authentificateur WebAuthn | ES256 (P-256) seulement, `fmt: "none"`, drapeau `UP` seul, aucune attestation ; clé privée chiffrée sous l'AAD `akooti:v1:passkey:<credential id>` ; champs binaires en base64url sur le fil |

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
│   │       ├── health.rs           audit zxcvbn, réutilisation, ancienneté, HIBP local
│   │       ├── sync.rs             codec E2EE, LWW, tombstones, enveloppe
│   │       ├── snapshot.rs         format `.akooti`, export/restauration
│   │       ├── backup.rs           cibles chiffrées, rétention, historique
│   │       ├── mcp/                jetons (SHA-256, portées), store, audit
│   │       ├── passkeys.rs         authentificateur WebAuthn : ES256, CBOR, COSE
│   │       └── vault_mcp.rs        opérations cloisonnées + plans d'injection
│   ├── akooti-ipc/             # protocole local : framing, `Request`/`Response`, socket, client
│   └── akooti-mcp/             # serveur MCP stdio (rmcp) : six outils, zéro clé
├── src-tauri/                  # shell desktop : CLI, tray, daemon, commandes IPC
│   └── src/
│       ├── cli.rs                  routage des modes avant toute fenêtre
│       ├── headless.rs             modes `mcp`, `native-host` et `native-register`
│       ├── native_host.rs          pompe de trames stdin/stdout <-> socket du daemon
│       ├── native_register.rs      manifestes Native Messaging par navigateur et par OS
│       ├── vault_runtime.rs        `Arc<VaultRuntime>`, verrou auto 15 min
│       ├── daemon/                 serveur IPC + inject.rs (CDP) + exec.rs (masquage)
│       ├── sync/                    client Supabase REST + Realtime (backoff)
│       ├── backup/                  pilotes local / S3 SigV4 / Drive + planificateur
│       └── commands/               surface IPC typée (vault, items, folders, generator,
│                                   routing, import/export, health, sync, backup, export,
│                                   clipboard, mcp, system)
├── apps/
│   ├── desktop/                # React + Vite : unlock, coffre, générateur, import,
│   │                           # routage, « Accès IA », santé, sync & sauvegardes
│   └── extension/              # WXT : un source, Chrome/Edge/Brave (MV3) et Firefox (MV2)
│       ├── wxt.config.ts           manifeste, permissions, ressource web-accessible
│       ├── entrypoints/
│       │   ├── background.ts       service worker : seul client du pont natif
│       │   ├── content.ts          ISOLATED : détection, remplissage, capture, copie
│       │   ├── webauthn.content.ts ISOLATED : injecte le shim et relaie les cérémonies
│       │   ├── webauthn-main.ts    script non listé, monde MAIN : remplace `navigator.credentials`
│       │   └── popup/              React : appairage, recherche, remplissage, copie
│       └── lib/
│           ├── protocol.ts         miroir TypeScript du protocole du daemon
│           ├── runtime.ts          client du bus interne (popup/content -> worker)
│           ├── native/bridge.ts    port Native Messaging, sérialisé, erreurs typées
│           ├── native/pairing.ts   jeton dans `storage.local`, forme et masquage
│           ├── dom/                heuristiques et injection, pures donc testables
│           └── webauthn/           relais nonce, lecture des options, reconstruction
└── packages/shared/            # types et protocoles TypeScript partagés
```

Les modules suivants sont **ciblés mais absents** et ne doivent pas être
supposés présents : `sync_supabase.rs`, `backup/`.
Voir [fonctionalites.md](./fonctionalites.md) pour leur statut.

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
| [0008](./adr/0008-extension-native-messaging.md)        | Pont Native Messaging, `vaultExtensionFill`, shim WebAuthn   |

## 8. Limites connues

| Limite                                                        | Référence                          |
| ------------------------------------------------------------- | ---------------------------------- |
| `vault_blind_autofill` (côté agent) répond `unavailable` : le verbe qui libère des valeurs n'est ouvert qu'au pont natif | [ADR 0008](./adr/0008-extension-native-messaging.md) |
| L'authentificateur n'est ni découvrable ni vérifiant l'utilisateur : une cérémonie qui l'exige est rendue au navigateur | `crates/akooti-core/src/passkeys.rs` |
| Aucune attestation : `fmt: "none"`, donc pas d'authentificateur certifié aux yeux d'un site qui l'exigerait | `crates/akooti-core/src/passkeys.rs` |
| ES256 seulement, pas d'Ed25519 : ES256 est le seul algorithme qu'une relying party est tenue de supporter, et un second algorithme demanderait de mémoriser la courbe de chaque credential — donc un changement de schéma | `crates/akooti-core/src/passkeys.rs` |
| Un credential reconstruit côté extension n'est pas une instance de `PublicKeyCredential` (objet compatible seulement) | `apps/extension/lib/webauthn/credential.ts` |
| Pas de transport IPC Windows (named pipe) ni de manifeste Native Messaging dans le registre | [ADR 0006](./adr/0006-transport-ipc-socket-unix.md) |
| Aucun fournisseur biométrique branché ; l'interface masque le contrôle | [ADR 0005](./adr/0005-biometrie-deverrouillage.md) |
| Les secrets TOTP de moins de 128 bits sont acceptés (services réels) | [ADR 0007](./adr/0007-fills-et-secrets-totp-courts.md) |
| `SelectorOverride.submit: null` ne désactive pas la soumission (indistinguable d'« absent ») | `daemon/inject.rs` |
| Google Drive vérifié uniquement hors ligne (conversions, parseurs) | compte Google + refresh token à brancher |
