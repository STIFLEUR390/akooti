# Périmètre fonctionnel

Ce document décrit ce que fait Akooti, fonction par fonction, avec son statut.
Il fait foi pour arbitrer « est-ce déjà là ? » ; la technique est dans
[architecture.md](./architecture.md), les contraintes dans
[regles.md](./regles.md).

**Statuts** : ✅ implémenté (couvert par des tests) · 🧭 prévu (décidé, non
écrit) · ⚠️ limite connue.

## 1. Positionnement

| Élément           | Choix                                                                                 |
| ----------------- | ------------------------------------------------------------------------------------- |
| Nom               | **Akooti**, exécutable unique `akooti`                                                  |
| Philosophie       | _Local-first_ (100 % hors ligne), _zero-knowledge_ (chiffrement de bout en bout), _zero-leak_ (aucun secret exposé à un modèle d'IA) |
| Desktop & CLI     | Tauri v2 (Rust + React/TypeScript), un binaire pour l'interface, le daemon, le pont navigateur et le serveur MCP |
| Extension         | WXT (TypeScript) compilée vers Chrome, Edge, Brave, Firefox et Safari (MV3 / MV2) 🧭   |
| Stockage          | SQLCipher local + Argon2id + XChaCha20-Poly1305 + effacement mémoire (`zeroize`)       |
| Hors périmètre    | Serveur propriétaire, modèles d'IA embarqués, pièces jointes volumineuses              |

## 2. Coffre et organisation

| Fonction                          | Comportement attendu                                                                                     | Statut |
| --------------------------------- | -------------------------------------------------------------------------------------------------------- | ------ |
| Multi-dossiers (N:N)              | Une entrée appartient à plusieurs dossiers sans duplication ; toute modification est visible partout       | ✅     |
| Dossiers système                  | `Personnel`, `Dev Local`, `Agents & MCP` créés à l'initialisation, non supprimables et non renommables     | ✅     |
| Champs d'une entrée               | Titre, URL principale, identifiant, mot de passe, graine TOTP, notes, champs personnalisés, URI multiples, favori | ✅ |
| Sélecteurs DOM mémorisés          | `username`, `password`, `totp`, `submit` stockés sur l'entrée et réutilisés par l'injection                | ✅     |
| Historique des mots de passe      | Versions précédentes conservées chiffrées, écrites seulement en cas de changement réel                     | ✅     |
| Comptes de test éphémères (TTL)   | `expires_at` sur l'entrée, purge automatique des comptes créés par un agent                                | ✅     |
| Double authentification (TOTP)    | Codes à 6 chiffres, pas de 30 s, SHA-1/SHA-256, affichage temps réel et copie rapide                       | ✅     |
| Déverrouillage biométrique        | Trait d'union présent, aucun fournisseur branché ; l'interface masque le contrôle                          | ⚠️ [ADR 0005](./adr/0005-biometrie-deverrouillage.md) |
| Verrouillage automatique          | Après 15 minutes d'inactivité, mise en veille ou action explicite ; la clé est effacée                     | ✅     |
| Passkeys (WebAuthn / FIDO2)       | Interception de `navigator.credentials`, signature locale, clé privée stockée dans le coffre                | 🧭 phase 6 |

## 3. Routage intelligent

| Fonction                       | Comportement attendu                                                                                       | Statut |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------- | ------ |
| Règles par défaut              | `.local`, `.test`, `.localhost` (suffixe), `localhost`, `127.0.0.1` (hôte exact) → `Dev Local`             | ✅     |
| Types de règle                 | `suffix`, `exact_host`, `cidr`, `regex`                                                                    | ✅     |
| Règles personnalisées          | Ajout, activation / désactivation, suppression depuis la page **Routage**                                   | ✅     |
| Routage à l'import             | Case activée par défaut : les URLs locales sont **ajoutées** au dossier `Dev Local`                        | ✅     |
| Application rétroactive        | Analyse des entrées existantes et ajout d'appartenance en un clic, sans jamais retirer un dossier existant  | ✅     |
| Non-destructivité              | Le routage ajoute une appartenance et n'en retire jamais                                                     | ✅     |

## 4. Importation universelle

| Source                        | Formats acceptés                                                     | Statut |
| ----------------------------- | -------------------------------------------------------------------- | ------ |
| Buttercup                     | `.bcup` chiffré (AES-256-CBC + HMAC-SHA256), export `.csv`            | ✅     |
| Bitwarden / Vaultwarden       | `.json` chiffré ou en clair : dossiers, URI multiples, TOTP, champs perso | ✅  |
| Proton Pass                   | `.json` et `.zip` non chiffré                                        | ✅     |
| KeePass / KeePassXC           | `.kdbx` (v3 et v4), `.xml`, `.csv`                                    | ✅     |
| 1Password                     | `.1pux`, `.csv`                                                      | ✅     |
| Navigateurs                   | `.csv` Chrome, Brave, Edge, Firefox                                   | ✅     |
| Détection automatique         | Le format est déduit du contenu quand l'utilisateur ne l'impose pas   | ✅     |

Hygiène post-import, commune à tous les formats :

- **aperçu avant écriture** : entrées, dossiers, doublons détectés ;
- **politique de doublons** : ignorer ou importer malgré tout ;
- **effacement sécurisé de la source** : optionnelle, exécutée uniquement après
  validation de la transaction SQLCipher, avec écrasement avant suppression ;
- **rapport d'import** : compteurs (importés, ignorés routés, erreurs) et état
  du nettoyage.

## 5. Intégration IA (MCP)

Le serveur MCP (`akooti mcp --token …`) expose six outils. Aucun ne retourne de
mot de passe, de graine TOTP ou de champ personnalisé.

| Outil                   | Permission        | Comportement                                                                                  | Statut |
| ----------------------- | ----------------- | --------------------------------------------------------------------------------------------- | ------ |
| `vault_status`          | —                 | Décrit le jeton : nom, portées, dossiers autorisés, expiration                                | ✅     |
| `vault_search`          | `read_metadata`   | Recherche titre / identifiant / URL, métadonnées seules                                        | ✅     |
| `vault_cdp_autofill`    | `blind_fill`      | Remplit une page via Chrome DevTools Protocol, origine vérifiée strictement                    | ✅     |
| `vault_blind_autofill`  | `blind_fill`      | Même remplissage via l'extension et Native Messaging                                          | 🧭 phase 6 (répond `unavailable`) |
| `vault_create_and_fill` | `create_and_fill` | Crée un compte avec mot de passe généré et TTL, puis remplit le formulaire d'inscription       | ✅     |
| `vault_exec`            | `exec_env`        | Lance une commande avec les secrets en variables d'environnement, sortie masquée               | ✅     |

Garde-fous :

- **jetons cloisonnés** : un ou plusieurs dossiers, quatre portées, TTL,
  révocation immédiate, secret affiché une seule fois ;
- **origine imposée** : un remplissage dont l'URL ne correspond pas exactement à
  l'origine demandée est refusé et audité ;
- **audit complet** : chaque appel écrit une ligne (`succès`, `refusé`,
  `erreur`) avec l'origine et un diagnostic sans valeur secrète ;
- **aucun TOTP dans l'environnement** : `vault_exec` n'expose pas `AKOOTI_TOTP`.

Un skill destiné aux agents ([`skills/akooti/SKILL.md`](../skills/akooti/SKILL.md))
documente le mode d'emploi : quel outil pour quel besoin, ce qu'un refus
signifie, comment réagir sans redemander un mot de passe à l'utilisateur.

## 6. Génération de secrets

| Fonction                     | Comportement attendu                                                        | Statut |
| ---------------------------- | --------------------------------------------------------------------------- | ------ |
| Mots de passe aléatoires     | `OsRng`, classes de caractères configurables, exclusion des ambigus `0/O`, `1/l/I`, entropie affichée | ✅ |
| Passphrases mémorisables     | Listes Diceware EFF, séparateur et capitalisation configurables              | ✅     |
| Disponibilité                | Interface desktop et `vault_create_and_fill` (génération interne)            | ✅     |

## 7. Synchronisation et sauvegardes

| Fonction                          | Comportement attendu                                                                 | Statut |
| --------------------------------- | ------------------------------------------------------------------------------------ | ------ |
| Base locale primaire              | Le coffre SQLCipher est la source de vérité, utilisable hors ligne                     | ✅     |
| Sync temps réel (Supabase)        | Projet Supabase propre à l'utilisateur, enregistrements chiffrés côté client, Realtime | 🧭     |
| Résolution de conflits            | Horodatage logique par enregistrement, suppressions propagées par tombstone           | 🧭     |
| Sauvegardes planifiées            | Instantanés `.akooti` chiffrés vers dossier local/NAS, S3 (AWS, R2, MinIO) ou Google Drive (OAuth2), rétention paramétrable | 🧭 |
| Export d'urgence                  | Export JSON, chiffré ou en clair après confirmation du mot de passe maître            | 🧭     |

## 8. Santé du coffre

| Fonction                        | Comportement attendu                                                                    | Statut |
| ------------------------------- | --------------------------------------------------------------------------------------- | ------ |
| Détection de faiblesse          | Score `zxcvbn` local (dictionnaire, dates, suites clavier)                               | 🧭     |
| Réutilisation et ancienneté     | Repérage des mots de passe partagés entre comptes et des secrets trop anciens            | 🧭     |
| Vérification de fuite (HIBP)    | `k-Anonymity` : seuls les 5 premiers caractères du SHA-1 sont envoyés                     | 🧭     |

## 9. Récapitulatif

| Domaine                    | Implémenté                                           | Cible                                     |
| -------------------------- | ---------------------------------------------------- | ----------------------------------------- |
| Coffre                     | multi-dossiers, historique, TTL, TOTP, verrouillage auto | biométrie, passkeys                   |
| Routage                    | règles complètes + rétroactif                        | —                                         |
| Import                     | 10 formats + détection automatique + hygiène          | —                                         |
| IA                         | 5 outils sur 6, jetons cloisonnés, audit              | `vault_blind_autofill` (extension)        |
| Génération                 | mots de passe + passphrases                          | —                                         |
| Sync / sauvegardes         | base locale                                          | Supabase E2EE, Cron S3/Drive/local, export |
| Santé                      | —                                                    | zxcvbn, réutilisation, HIBP k-Anonymity   |
