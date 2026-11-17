# Stratégie de test

Akooti manipule des secrets : un test qui « passe » mais laisse fuiter une
valeur est un échec. Ce document décrit les niveaux de vérification, la
couverture des contrats critiques et les règles applicables aux fixtures. La
recette manuelle dans le binaire réel est dans
[desktop-tests.md](./desktop-tests.md) ; les contraintes de sécurité sont dans
[regles.md](./regles.md).

## 1. Trois niveaux

| Niveau | Ce qu'il prouve | Outils | Exécution |
| ------ | --------------- | ------ | --------- |
| **Unitaire / intégration Rust** | Le métier est correct et étanche : crypto, DAO, jetons, audit, importateurs | `cargo test` (115 tests) | local + CI |
| **Unitaire TypeScript** | Le formatage, les modèles partagés, le client IPC et le mode démo de l'UI | `bun test` (50 tests) | local + CI |
| **Recette manuelle pilotée** | L'application réelle se comporte comme annoncé, accessibilité comprise | pont MCP Tauri, `axe-core` | à chaque fin de phase |

Le cœur métier s'exécute **sans Tauri** : `cargo test -p akooti-core` suffit.
C'est la garantie que la logique du coffre reste testable et portable
(règle ARCH-1).

## 2. Commandes

```bash
# Rust : cœur, protocole, MCP, shell desktop
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check

# TypeScript : SPA, extension, contrats partagés
bun install --frozen-lockfile
bun run lint
bun run format:check
bun run typecheck
bun test
bun run build
```

Un test qui échoue de manière intermittente est un bug de test, pas une
tolérance : il est corrigé dans la même PR (voir §6).

## 3. Ce que la CI exécute

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) déclenche trois
travaux sur chaque PR et sur `main` :

| Travail | Étapes | Bloque la fusion ? |
| ------- | ------ | ------------------ |
| `rust` | `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`, `cargo build --workspace` | oui |
| `js` | `bun install --frozen-lockfile`, `lint`, `format:check`, `typecheck`, `bun test`, `build` | oui |
| `desktop` | `bun run tauri build --no-bundle` (Linux, après `rust` et `js`) | oui |

Deux conséquences pratiques :

- **`--locked`** : un `Cargo.lock` ou un `bun.lock` non mis à jour fait échouer
  la CI, pas la machine du développeur.
- **Prettier ignore `docs/`** ([`.prettierignore`](../.prettierignore)) : la
  forme des documents est relue en revue, pas vérifiée automatiquement.

## 4. Couverture par contrat

### 4.1 `akooti-core` — le métier

| Surface | Contrats couverts | Emplacement |
| ------- | ----------------- | ----------- |
| Crypto | dérivation Argon2id, nonce, AAD, mauvais mot de passe, ciphertext altéré | `src/crypto.rs`, `tests/vault_flow.rs` |
| En-tête d'amorçage | encodage 40 octets, magic, version, bornes des paramètres KDF, troncature | `src/header.rs` |
| SQLCipher | PRAGMAs, migration idempotente, seeds système, cascade, absence de clair sur disque | `src/db.rs`, `tests/vault_flow.rs` |
| Cycle du coffre | création, verrouillage, réouverture, inactivité | `src/session.rs`, `tests/vault_flow.rs` |
| DAO | multi-dossiers N:N, suppression logique, remplacement des métadonnées, historique chiffré | `tests/vault_flow.rs`, `tests/phase2_vault_test.rs` |
| Édition d'entrées | secrets, champs personnalisés, sélecteurs, historique | `tests/item_edit_test.rs` |
| Générateur | classes de caractères, exclusion des ambigus, Diceware EFF, entropie | `tests/generator_test.rs` |
| TOTP | vecteurs RFC 6238 (SHA-1, SHA-256), tolérance de dérive, secret chiffré | `tests/phase2_vault_test.rs` |
| Routage | suffixe, hôte exact, CIDR, regex, faux positifs, rétroactif non destructif | `tests/routing_test.rs` |
| Importateurs | Buttercup / Bitwarden chiffrés, Proton / 1Password (ZIP), KeePass KDBX, CSV navigateurs | `tests/importers_*_test.rs` |
| Pipeline d'import | aperçu, doublons, rollback, routage, effacement post-commit | `tests/import_pipeline_test.rs` |
| Jetons MCP | secret affiché une fois, refus inconnu / révoqué / expiré, cloisonnement par dossier | `src/mcp/token.rs`, `tests/mcp_access_test.rs` |
| Opérations MCP | plan d'injection, compte éphémère + TTL, `exec_env`, audit sans valeur | `tests/mcp_access_test.rs` |

### 4.2 `akooti-ipc` — le protocole local

| Contrat | Vérification | Emplacement |
| ------- | ------------ | ----------- |
| Cadrage | longueur 4 octets LE, trame vide refusée, trame > 1 Mio refusée, flux tronqué | `src/framing.rs` |
| Messages | sérialisation `Request`/`Response`, version de protocole, corps d'erreur | `src/protocol.rs` |
| Socket | permission `0600`, chemin par défaut, dossier privé | `src/socket.rs` |
| Client | connexion, aller-retour, erreur distante, reconnexion | `src/client.rs` |

### 4.3 `akooti-mcp` — la façade agent

| Contrat | Vérification | Emplacement |
| ------- | ------------ | ----------- |
| Catalogue | six outils annoncés, schémas d'entrée valides | `src/lib.rs` |
| Refus | une erreur d'outil porte un code stable (`forbidden`, `locked`, …) | `src/lib.rs` |
| Étanchéité | aucun résultat de recherche ne contient mot de passe, graine ou champ secret | `src/lib.rs` |
| Daemon absent | réponse `daemon_unavailable`, jamais un succès silencieux | `src/lib.rs` |

### 4.4 `src-tauri` — le shell

| Contrat | Vérification | Emplacement |
| ------- | ------------ | ----------- |
| Routage des modes | `help`/`version` sans fenêtre, `mcp` sans jeton → code 2, `native-host` → code 3 | `src/cli.rs` |
| Modes headless | aucun webview démarré, sortie propre | `src/headless.rs` |
| Masquage `vault_exec` | valeur brute, URL-encodée et JSON remplacées par `***` dans `stdout`/`stderr` | `src/daemon/exec.rs` |
| Injection CDP | origine imposée, sélecteurs, statuts `filled` / `partial` / `failed`, pont absent → `unavailable` | `src/daemon/inject.rs` |
| Daemon | handshake et capacités annoncées, correspondance erreur → code stable | `src/daemon/mod.rs` |
| DTO | sérialisation camelCase attendue par le frontend | `src/dto.rs` |
| Biométrie | état exposé, aucun fournisseur branché | `src/biometrics.rs` |

### 4.5 TypeScript

| Surface | Contrats couverts | Emplacement |
| ------- | ----------------- | ----------- |
| Modèles partagés | sérialisation des entités et du protocole | `packages/shared/src/models.test.ts` |
| Client IPC | nommage des commandes, conversion des erreurs, mode démo hors Tauri | `apps/desktop/src/lib/ipc.test.ts` |
| Mode démo | jeu de données cohérent pour le développement navigateur | `apps/desktop/src/lib/mock.test.ts` |
| Formatage | dates, tailles, libellés, cas limites | `apps/desktop/src/lib/format.test.ts` |
| Force des mots de passe | bornes, classes, estimation d'entropie | `apps/desktop/src/lib/strength.test.ts` |

## 5. Règles applicables aux fixtures

1. **Aucun secret réel.** Les fixtures utilisent des valeurs inventées et
   reconnaissables (`s3cret-de-test`, `JBSWY3DPEHPK3PXP`, `example.test`). Une
   fixture commitée ne doit jamais contenir un identifiant d'un service réel
   (règle SEC-9).
2. **Coffres jetables uniquement.** Les tests créent leur coffre dans un
   répertoire temporaire (`tempfile`). Ils ne touchent jamais
   `~/.local/share/com.akooti.desktop`.
3. **Aucune dépendance réseau.** Les tests n'ouvrent pas de connexion : le CDP et
   les imports sont simulés (fichiers locaux, daemon factice, socket de test).
4. **Non-régression de l'étanchéité.** Toute voie nouvelle qui manipule un
   secret arrive avec un test qui vérifie **l'absence** de la valeur dans la
   sortie (règle ARCH-6).

## 6. Ajouter un test

Checklist de revue :

- [ ] Le contrat testé est nommé dans une phrase (`fn a_revoked_token_cannot_search`),
      pas `test_1`.
- [ ] Le test échoue si l'on casse le comportement (vérifié en le sabotant une
      fois).
- [ ] Pour une surface sensible, la sortie est inspectée pour l'absence de
      secret, pas seulement le code de retour.
- [ ] Aucun `sleep` de synchronisation : un test temporisé est un test flaky.
- [ ] Si le test dépend d'une donnée externe (liste EFF, vecteurs RFC), la
      donnée est citée et sa tolérance justifiée.

Un exemple vécu : `generator_test` supposait que les 7 776 entrées de la liste
EFF Diceware tenaient en trois lettres. Quatre entrées contiennent un tiret
(`drop-down`, `felt-tip`, `t-shirt`, `yo-yo`) ; le test a été corrigé, pas la
liste.

## 7. Ce qui n'est pas couvert automatiquement

| Sujet | Raison | Où c'est vérifié |
| ----- | ------ | ---------------- |
| Interface graphique réelle | nécessite un webview et un compositeur | [desktop-tests.md](./desktop-tests.md), pont MCP |
| Accessibilité | audit `axe-core` sur un écran rendu | [desktop-tests.md](./desktop-tests.md) |
| Purge effective du presse-papiers | dépend du gestionnaire de presse-papiers de l'OS | recette manuelle |
| Pont Native Messaging / extension | livré en phase 6 | `vault_blind_autofill` répond `unavailable` |
| Biométrie | aucun fournisseur branché ([ADR 0005](./adr/0005-biometrie-deverrouillage.md)) | recette manuelle |
| Sync Supabase et sauvegardes | non implémentés | conception seulement ([schema.md](./schema.md)) |
