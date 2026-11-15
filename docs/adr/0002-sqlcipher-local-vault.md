# 0002 — Coffre local SQLCipher + chiffrement de champ XChaCha20

- **Statut :** acceptée
- **Date :** 2026-11-15

## Contexte

Le coffre doit fonctionner hors ligne, survivre à la copie du fichier de base
et ne jamais exposer de secret à un service tiers. Un chiffrement uniquement
applicatif laisse les métadonnées en clair ; un chiffrement uniquement fichier
ne protège pas les secrets contre une lecture mémoire ou un dump partiel.

## Décision

- **SQLCipher** comme stockage primaire (AES-256 au niveau fichier, WAL).
- **Argon2id** (m = 64 MiB, t = 3, p = 4) pour dériver la KEK du mot de passe
  maître ; la clé maître est encapsulée (`wrapped_master_key`) hors ligne.
- **XChaCha20-Poly1305** côté application pour chaque champ secret
  (mot de passe, TOTP, notes, champs personnalisés, clés de passkey), avec un
  nonce de 24 octets stocké séparément.
- **`zeroize`** sur toute variable détenant une clé ou un secret.

## Alternatives écartées

- SQLite en clair + chiffrement applicatif : métadonnées et structure lisibles.
- `age`/fichier chiffré unique : pas de requêtes, pas de multi-dossiers, pas de
  synchronisation granulaire.
- Trousseau OS seul : dépendant de la plateforme, insuffisant pour les
  sauvegardes chiffrées et la synchronisation.

## Conséquences

- Le binaire doit embarquer SQLCipher (`rusqlite` avec `bundled-sqlcipher`).
- Les migrations sont versionnées (`user_version`) et doivent rester
  idempotentes.
- Toute rotation de clé ou changement de paramètres KDF doit conserver la
  capacité d'ouvrir les coffres existants.
- Les tests négatifs (mauvais mot de passe, ciphertext altéré) sont
  obligatoires.
