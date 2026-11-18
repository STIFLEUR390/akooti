# Modèle de menace

Ce document décrit ce qu'Akooti protège, contre qui, et où passent les
frontières de confiance. La revue associée, avec ses constats et leurs
correctifs, est dans [`security-review.md`](./security-review.md).

## 1. Actifs

| Actif | Où il vit | Sensibilité |
| ----- | --------- | ----------- |
| Mots de passe, graines TOTP, notes, champs personnalisés | `vault.db` (SQLCipher) + champs XChaCha20 | critique |
| Clé maître | RAM du binaire GUI/daemon, `Zeroizing` | critique |
| Mot de passe maître | jamais stocké (Argon2id → clé de base) | critique |
| Jetons MCP | SHA-256 en base, secret affiché une fois | élevé |
| Clés privées de passkeys | chiffrées, AAD sur le credential id | élevé |
| Configuration de sauvegarde (clé S3, refresh token Drive) | chiffrée par la clé maître | élevé |
| Jetons Supabase | chiffrés dans `sync_config` | élevé |
| Blobs synchronisés | serveur Supabase de l'utilisateur | chiffré, métadonnées visibles |
| Journal d'audit IA | local uniquement | moyen |
| Snapshots `.akooti` | cible de sauvegarde | critique (même chiffrement que le coffre) |

## 2. Adversaires

| Adversaire | Capacité | Objectif |
| ---------- | -------- | -------- |
| Agent IA / LLM | appelle les outils MCP, lit les réponses | obtenir un secret en clair |
| Processus local non privilégié | lit les fichiers de l'utilisateur, le socket s'il est mal protégé | lire le coffre, voler un jeton |
| Administrateur du serveur de sync | lit la base Supabase, les sauvegardes | retrouver des titres, URLs, secrets |
| Réseau | observe le trafic HIBP, Supabase, S3/Drive | identifier des secrets |
| Extension compromise / page hostile | exécute du JS dans le navigateur | détourner un remplissage, voler un secret |
| Site web ciblé | reçoit les valeurs injectées | — (destinataire légitime) |
| Tiers malveillant qui vole un backup | lit le fichier `.akooti` | déchiffrer le coffre |
| Utilisateur lui-même (erreur) | supprime, écrase, oublie le mot de passe | perte de données |

## 3. Frontières de confiance

```text
[agent MCP] --stdio--> [akooti-mcp] --socket 0600--> [daemon/GUI : clé maître]
                                                        |
                                     +------------------+------------------+
                                     |                  |                  |
                                [CDP page]      [processus enfant]   [Supabase/S3/Drive]
                                 secrets             env vars          blobs chiffrés
```

- La clé maître ne quitte **jamais** le processus GUI/daemon (SEC-2, ARCH-2).
- Le serveur MCP et le pont Native Messaging ne désérialisent pas de secret :
  ils recopient des trames, ou reçoivent un compte-rendu de remplissage.
- Le serveur de sync ne reçoit que `ciphertext`/`nonce` + métadonnées (SEC-12).

## 4. Menaces et garde-fous

| # | Menace | Garde-fou | Vérification |
| - | ------ | --------- | ------------ |
| T1 | L'agent extrait un mot de passe via un outil MCP | aucun outil ne renvoie de valeur ; remplissage aveugle ; `vault_exec` masque stdout/stderr | `tests/mcp_access_test.rs`, tests in-file `akooti-mcp`, `src/daemon/exec.rs` |
| T2 | Un jeton MCP dépasse ses droits | portées + dossiers + TTL + révocation, vérifiés avant toute lecture ; audit `access_denied` | `mcp_access_test.rs` (scopes, dossiers, TTL) |
| T3 | Un processus local lit le coffre | SQLCipher (AES-256), champs XChaCha20, clé en RAM, socket `0600` | `tests/vault_flow.rs`, `akooti-ipc` tests |
| T4 | Un backup volé est déchiffré | snapshot = `vault.db` chiffré + `vault.meta` public, aucune seconde clé | `tests/backup_test.rs` |
| T5 | L'administrateur Supabase lit les données | payload par enregistrement chiffré, enveloppe seule en clair | `tests/sync_test.rs` (opacité), test RLS réel (2 utilisateurs) |
| T6 | Rejeu / réordre d'un enregistrement | AAD liée au type + id, LWW, tombstones | `tests/sync_test.rs` |
| T7 | HIBP apprend un mot de passe | seuls 5 caractères du SHA-1, suffixes comparés localement, action explicite, cache 24 h | `tests/health_test.rs`, SEC-11 |
| T8 | Une page hostile détourne un remplissage | origine exacte exigée, refus avant script, audit | `tests/mcp_access_test.rs`, `daemon::tests` |
| T9 | Une passkey émise ailleurs est rejouée | clé liée au credential (AAD), `rpId` rattaché à l'origine, compteur croissant | `tests/passkey_test.rs` |
| T10 | Un secret fuit dans un log ou une erreur | messages statiques, `Zeroizing`, masquage `vault_exec`, revue des `tracing::` | revue 9.1, `exec.rs` |
| T11 | Le presse-papiers garde un secret | copie puis effacement à 30 s si la valeur est inchangée | `tests`/`useSecretReveal`, `clipboard.rs` |
| T12 | Le webview charge du code distant | CSP stricte (`default-src 'self'`), pas de `eval` applicatif | `tauri.conf.json`, revue 9.1 |
| T13 | L'auto-lock est contourné | verrou 15 min côté daemon, verrouillage à la veille/exit, `locked` sur chaque commande | `tests/vault_flow.rs`, `vault_runtime.rs` |
| T14 | Le canal de mise à jour pousse un binaire piégé | manifeste signé Ed25519, clé publique dans le binaire, HTTPS | `docs/release.md`, `commands/updates.rs` |
| T15 | Perte du mot de passe maître | avertissement explicite, export `.akooti`/JSON, restauration | `desktop-tests.md` §8 |

## 5. Risques résiduels assumés

1. **OS compromis** : un malware avec la session de l'utilisateur peut lire la
   RAM du processus, les frappes clavier ou attendre que le coffre soit ouvert.
   Aucun coffre local ne s'en protège.
2. **Métadonnées de sync** : le serveur voit le nombre d'enregistrements, leur
   type et leurs horodatages de modification (pas les titres). C'est le prix
   d'un delta et d'un Realtime efficaces.
3. **Disponibilité du serveur de sync** : le coffre local reste la source de
   vérité ; une panne Supabase ne bloque rien.
4. **HIBP** : la requête révèle qu'un préfixe de hash est vérifié, et à quel
   moment. L'utilisateur déclenche lui-même l'appel.
5. **Hôte de mise à jour** : GitHub Releases et la clé de signature sont des
   dépendances de confiance ; leur compromission est traitée comme un incident.
6. **Sauvegardes non testées** : un snapshot jamais restauré n'est pas une
   sauvegarde. La recette demande un essai de restauration.

## 6. Hors périmètre

- Attaques physiques (cold boot, DMA) et canaux auxiliaires matériels.
- Sécurité du navigateur hôte et de ses extensions autres qu'Akooti.
- Sécurité de l'infrastructure Supabase / S3 / Google au-delà de nos requêtes.
- Windows (named pipe, manifeste de registre) : non livré, hors modèle actuel.
