# 0004 — En-tête public d'amorçage du coffre

- **Statut :** acceptée
- **Date :** 2026-11-15

## Contexte

La KEK SQLCipher est dérivée avec Argon2id et un sel aléatoire. Ce sel ne peut
pas être lu dans `vault_meta` avant l'ouverture du fichier chiffré, ce qui crée
un cycle impossible à résoudre avec un fichier unique.

## Décision

- Le coffre utilise `vault.meta` et `vault.db` dans `app_data_dir`.
- `vault.meta` est un en-tête binaire public versionné contenant uniquement le
  magic, la version, l'algorithme, le sel de 16 octets et les paramètres
  Argon2id (`m=65536 KiB`, `t=3`, `p=4`).
- La KEK est dérivée depuis le mot de passe et cet en-tête, puis utilisée comme
  clé brute SQLCipher.
- La clé maître aléatoire reste dans `vault_meta`, encapsulée par la KEK avec
  XChaCha20-Poly1305. Les paramètres KDF y sont dupliqués et comparés à l'en-tête
  à chaque ouverture.
- L'en-tête ne contient aucune clé, aucun secret et aucune donnée utilisateur.

## Conséquences

- Une copie du coffre comprend les deux fichiers.
- Un en-tête modifié ou incohérent produit une erreur de corruption explicite.
- La rotation du mot de passe devra réencrypter la base SQLCipher et réécrire
  l'enveloppe de clé maître.
