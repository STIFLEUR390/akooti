# Règles du projet

Ces règles sont **normatives**. Une contribution qui les viole est refusée en
revue, même si elle fonctionne. Elles s'appliquent au code, aux tests, aux
fixtures, à la documentation et aux messages de commit.

## Comment lire ce document

- Chaque règle porte un identifiant stable (`SEC-1`, `ARCH-3`, …) réutilisable en
  revue, dans une issue ou dans un commentaire de PR.
- « **doit** » / « **ne doit jamais** » sont des obligations ; « devrait » est
  une recommandation argumentée.
- La colonne « application » de la dernière section indique **comment** la règle
  est tenue aujourd'hui : test, revue, ou les deux. Une règle sans application
  est une dette assumée, pas une intention décorative.

---

## 1. Sécurité et cryptographie

| #     | Règle                                                                                                                                  |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------- |
| SEC-1 | **Zero-leak envers les modèles.** Aucun outil MCP (`vault_status`, `vault_search`, `vault_blind_autofill`, `vault_cdp_autofill`, `vault_create_and_fill`, `vault_exec`) ne retourne un mot de passe, une clé API, une graine ou un code TOTP. Les issues sont : statut, champs remplis, sortie masquée. |
| SEC-2 | **Trajet des secrets.** Un secret ne circule que de la RAM du binaire vers la cible finale : extension via Native Messaging, page via CDP, environnement d'un processus enfant. Il ne transite jamais par stdout, un fichier temporaire ou le réseau. |
| SEC-3 | **Mémoire vive.** Toute valeur sensible en Rust est encapsulée dans `zeroize::Zeroizing<T>` et effacée à la sortie de portée : clé maître, clé dérivée, secret déchiffré, jeton. |
| SEC-4 | **Verrouillage.** La clé quitte la RAM après 15 minutes d'inactivité, à la mise en veille ou sur demande. Tant que le coffre est verrouillé, toute commande répond `locked` — jamais une valeur périmée, jamais un succès silencieux. |
| SEC-5 | **Zero-knowledge côté cloud.** Tout chiffrement et déchiffrement se fait côté client. Aucune clé, aucun mot de passe et aucun texte en clair ne quitte l'appareil, y compris vers un Supabase administré par l'utilisateur. |
| SEC-6 | **Isolation IPC.** Le socket local (`0600`, dossier réservé à l'utilisateur) est le seul canal entre le daemon et les sous-processus. Aucun serveur HTTP local non authentifié, aucun port en écoute supplémentaire. |
| SEC-7 | **Jetons opaques.** Un secret de jeton n'est stocké qu'en SHA-256, comparé en temps constant, et affiché une seule fois à la création. La liste ne montre que le préfixe. |
| SEC-8 | **Nettoyage post-import.** L'effacement du fichier source en clair est proposé après chaque import, exécuté seulement si la transaction est validée, et n'annule jamais un import déjà commité. |
| SEC-9 | **Pas de secret en clair hors du coffre.** Pas de secret dans un log, un message d'erreur, une capture, une fixture commitée, un ticket ou une PR. |
| SEC-10 | **Exception documentée.** Un secret TOTP de moins de 128 bits est accepté (les services réels en émettent) ; le décodage Base32 reste le seul refus. Voir [ADR 0007](./adr/0007-fills-et-secrets-totp-courts.md). |

---

## 2. Données et intégrité

| #      | Règle                                                                                                                                 |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| DATA-1 | **Multi-appartenance.** Le partage d'une entrée entre dossiers passe **uniquement** par la table `item_folders`. Ne jamais dupliquer une entrée pour la ranger dans deux dossiers. |
| DATA-2 | **Dossiers système.** `Personnel`, `Dev Local`, `Agents & MCP` sont créés à l'initialisation et protégés (`is_system = 1`) : non supprimables, non renommables. |
| DATA-3 | **Routage non destructif.** Une règle de routage **ajoute** une appartenance (`Dev Local`) ; elle ne retire jamais un dossier existant. |
| DATA-4 | **Suppression logique.** Toute suppression d'entrée ou de dossier pose `deleted_at` (tombstone) pour rester propagable plus tard ; la purge physique reste une opération de maintenance. |
| DATA-5 | **Historique.** L'historique des mots de passe n'enregistre que les changements réels : réenregistrer la même valeur n'ajoute pas de version. |
| DATA-6 | **Comptes éphémères.** Une entrée créée par un agent porte `created_by_agent` et `expires_at` ; la purge des comptes expirés est automatique. |
| DATA-7 | **Migrations.** Le schéma évolue par migrations idempotentes ajoutées, jamais par modification d'une migration déjà livrée ([schema.md](./schema.md)). |

---

## 3. Architecture et code

| #      | Règle                                                                                                                                  |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| ARCH-1 | **Séparation stricte.** `akooti-core` n'importe ni Tauri, ni WebView, ni React. Il reste testable seul et portable (futur mobile).      |
| ARCH-2 | **Frontières de processus.** Seul le binaire principal détient la clé. Les sous-processus `mcp` et `native-host` n'ont accès qu'au socket et à leur jeton. |
| ARCH-3 | **Un seul binaire multi-modes.** Les modes sont routés avant toute création de fenêtre (`src-tauri/src/cli.rs`) ; un mode headless ne doit jamais démarrer de webview. |
| ARCH-4 | **Contrats partagés.** Le protocole IPC (`crates/akooti-ipc`) et les types TypeScript (`packages/shared`) sont les seules définitions de contrat ; aucune structure ad hoc dupliquée dans les applications. |
| ARCH-5 | **Documentation synchronisée.** La documentation décrit le code livré. Un changement de comportement met à jour le document correspondant dans la même PR ; un élément non implémenté est explicitement marqué 🧭. |
| ARCH-6 | **Tests d'abord pour la sécurité.** Toute nouvelle voie qui manipule un secret, un jeton ou une destination d'injection arrive avec un test de non-fuite (voir [testing.md](./testing.md)). |
| ARCH-7 | **Conventional Commits.** `type(scope): résumé` en anglais, types `feat`, `fix`, `docs`, `chore`, `refactor`, `test` ; branches `feat/`, `fix/`, `chore/`, `docs/`, `refactor/`, `test/`. |

---

## 4. Intelligence artificielle et jetons MCP

| #    | Règle                                                                                                                                  |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------- |
| AI-1 | **Moindre privilège.** Un jeton est restreint à une liste explicite de dossiers et aux seules portées utiles : `read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`. |
| AI-2 | **Expiration et révocation.** Tout jeton porte un TTL (ou une décision explicite d'illimité) et peut être révoqué en un clic ; un jeton révoqué ne peut plus rien, mais reste visible dans l'audit. |
| AI-3 | **Audit systématique.** Chaque action IA écrit une ligne `mcp_audit_logs` : `vault_search`, `cdp_autofill`, `create_and_fill`, `exec_env`, et `access_denied` pour les refus. Une action = une ligne. |
| AI-4 | **Audit honnête.** Le statut suit la réalité : un remplissage qui ne remplit aucun champ est une `erreur`, pas un succès. Le détail d'un refus porte le nom de l'action demandée, pas d'un helper interne. |
| AI-5 | **Local seulement.** Jetons, journaux d'audit et configuration de sauvegarde ne sont jamais synchronisés ni exportés. |
| AI-6 | **Origine imposée.** Un remplissage exige une origine explicite ; toute page dont l'URL ne correspond pas est refusée avant le moindre script. |
| AI-7 | **Comptes d'agent.** Par défaut, un compte créé par un agent va dans le dossier autorisé du jeton (à défaut `Agents & MCP`) et reçoit un TTL. |
| AI-8 | **Environnement minimal.** `vault_exec` n'expose que `AKOOTI_USERNAME`, `AKOOTI_PASSWORD` et un `AKOOTI_<CHAMP>` par champ personnalisé. Pas de mot de passe maître, pas de TOTP, pas de variable préexistante réécrite silencieusement. |

---

## 5. Interface et accessibilité

| #    | Règle                                                                                                                                   |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------- |
| UX-1 | **Clavier d'abord.** Recherche rapide, création d'entrée, copie de l'identifiant, copie du code TOTP et ouverture du générateur sont accessibles au clavier, avec un raccourci affiché. |
| UX-2 | **Thème sombre par défaut**, thème clair disponible, préférence conservée localement.                                                     |
| UX-3 | **Masquage.** Un mot de passe est masqué par défaut ; sa révélation est temporaire et compte à rebours (30 s) avant re-masquage.         |
| UX-4 | **Presse-papiers.** La copie d'un secret ou d'un code TOTP programme l'effacement du presse-papiers après 30 secondes, avec retour visuel. |
| UX-5 | **Lisibilité des dossiers.** Les dossiers système sont distingués visuellement des dossiers personnels, et un dossier couvert par un jeton MCP actif porte un badge explicite. |
| UX-6 | **Accessibilité.** Aucune violation `axe-core` critique ou sérieuse sur les écrans livrés ; tout contrôle non textuel a un nom accessible (voir [desktop-tests.md](./desktop-tests.md)). |
| UX-7 | **Erreurs utiles, sans fuite.** Un message dit quoi faire ; il ne divulgue ni valeur, ni détail cryptographique exploitable.             |
| UX-8 | **Confirmation destructive.** Suppression de coffre, d'entrée ou de jeton : action explicite, irréversible annoncée avant exécution.     |

---

## 6. Périmètre (garde-fous anti-dispersion)

| #       | Règle                                                                                                                    |
| ------- | ------------------------------------------------------------------------------------------------------------------------ |
| SCOPE-1 | **Pas de backend propriétaire.** Akooti n'héberge rien : disque local, projet Supabase de l'utilisateur, son bucket, son Drive. |
| SCOPE-2 | **Pas de modèle d'IA embarqué.** Le binaire reste léger et déterministe ; l'intelligence reste chez l'agent, guidé par `skills/akooti/SKILL.md`. |
| SCOPE-3 | **Pas de stockage lourd.** Secrets textuels, TOTP, sélecteurs, passkeys — pas de pièces jointes volumineuses.            |
| SCOPE-4 | **Réversibilité.** Un export d'urgence reste toujours possible : un utilisateur ne doit jamais être prisonnier du produit. |
| SCOPE-5 | **Pas de fonctionnalité non documentée.** Toute fonctionnalité livrée est décrite dans [fonctionalites.md](./fonctionalites.md) avec son statut. |

---

## 7. Phasage

Les phases suivies dans Kaneo ; une phase n'est déclarée terminée que si son
document de recette ([desktop-tests.md](./desktop-tests.md),
[testing.md](./testing.md)) est à jour.

| Phase | Contenu                                                                                     | Statut |
| ----- | ------------------------------------------------------------------------------------------- | ------ |
| 0     | Fondations : monorepo Rust + Bun, CI, conventions, schémas, bridge MCP Tauri                 | ✅     |
| 1     | Cryptographie et stockage chiffré : Argon2id, XChaCha20, SQLCipher, dossiers, session         | ✅     |
| 2     | Fonctions du coffre : générateur, TOTP, routage `Dev Local`                                  | ✅     |
| 3     | Importateurs universels et hygiène post-import                                               | ✅     |
| 4     | Application desktop Tauri v2 : interface du coffre, accessibilité                            | ✅     |
| 5     | Pont IA : CLI multi-modes, daemon IPC, serveur MCP, jetons cloisonnés, audit, `SKILL.md`      | ✅     |
| 6     | Extension WXT multi-navigateurs, Native Messaging, passkeys                                   | ⚠️ livrée sans authenticateur WebAuthn (tâche 6.4) |
| 7+    | Synchronisation Supabase E2EE, sauvegardes planifiées, santé du coffre                        | 🧭     |

---

## 8. Application des règles

| Règle        | Comment elle est tenue aujourd'hui                                                                 |
| ------------ | -------------------------------------------------------------------------------------------------- |
| SEC-1, AI-3  | Tests d'étanchéité : `crates/akooti-core/tests/mcp_access_test.rs`, tests in-file de `akooti-mcp`, `src-tauri/src/daemon` |
| SEC-2        | `src-tauri/src/native_host.rs` (pompe de trames, aucun désassemblage) et `apps/extension/lib` (une valeur ne remonte jamais vers la popup) |
| SEC-3, SEC-4 | `Zeroizing` imposé en revue ; tests de verrouillage dans `tests/vault_flow.rs`                       |
| SEC-5        | Aucun code réseau dans `akooti-core` : à vérifier en revue lors de l'ajout de la sync               |
| SEC-6, ARCH-4 | `crates/akooti-ipc` : tests de cadrage, de plafond et de socket ; trames dorées côté TypeScript      |
| SEC-7        | `crates/akooti-core/src/mcp/token.rs` (SHA-256, comparaison en temps constant, préfixe d'affichage) |
| DATA-1..DATA-6 | Tests DAO : `tests/vault_flow.rs`, `tests/phase2_vault_test.rs`, `tests/routing_test.rs`          |
| DATA-7       | `src/db.rs` : migration idempotente vérifiée par test                                               |
| ARCH-1       | Absence de dépendance Tauri dans `crates/akooti-core/Cargo.toml` + `cargo test -p akooti-core` seul |
| ARCH-2       | `vaultExtensionFill` refusé à toute connexion non `native-host`, et refus rejoué depuis `akooti-mcp` |
| UX-*         | `bun test` (hooks et formatage) et audit `axe-core` documenté dans [desktop-tests.md](./desktop-tests.md) |
| ARCH-7       | Revue + `cargo fmt` / ESLint / Prettier en CI                                                       |

Une règle qui ne peut pas être vérifiée automatiquement doit au minimum être
citée dans la checklist de revue de la PR.
