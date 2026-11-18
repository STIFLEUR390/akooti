# Changelog

Toutes les versions notables d'Akooti. Format inspiré de
[Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versionnage
[SemVer](https://semver.org/lang/fr/).

## [1.0.0] — 2026-11-18

Première version stable : coffre local-first, agents IA sans fuite de secret,
santé du coffre, synchronisation chiffrée et sauvegardes.

### Ajouté

- **Coffre** : SQLCipher + XChaCha20-Poly1305 par champ, dossiers N:N,
  historique des mots de passe, comptes éphémères (TTL), verrouillage auto 15 min.
- **Générateur** : mots de passe `OsRng`, passphrases Diceware EFF.
- **TOTP** (RFC 6238) et **passkeys** WebAuthn servies par le coffre (ES256,
  `fmt: "none"`, repli navigateur).
- **Import** : Bitwarden/Vaultwarden, 1Password, KeePass/KeePassXC, Proton Pass,
  Buttercup, CSV navigateurs, détection auto, doublons, effacement de la source.
- **Routage `Dev Local`** non destructif et rétroactif.
- **Pont IA** : CLI multi-modes, daemon IPC `0600`, serveur MCP stdio (6 outils),
  jetons cloisonnés (scopes, dossiers, TTL, révocation, audit), remplissage CDP,
  `vault_create_and_fill`, `vault_exec` masqué, `SKILL.md`.
- **Extension WXT** (Chrome/Edge/Brave/Firefox) : pont Native Messaging, popup,
  capture de sélecteurs, shim WebAuthn.
- **Santé du coffre** : score zxcvbn, réutilisation, ancienneté, HIBP
  _k-anonymity_ sur geste explicite, tableau de bord et export CSV.
- **Synchronisation E2EE** : schéma Supabase + RLS (`supabase/setup.sql`),
  codec chiffré, LWW + tombstones, Realtime avec reprise.
- **Sauvegardes** : cibles chiffrées local / S3 SigV4 / Google Drive,
  planificateur horaire, rétention, historique.
- **Réversibilité** : instantanés `.akooti`, restauration, export JSON en clair
  après confirmation du mot de passe maître.
- **Mises à jour signées** : canal Ed25519, manifeste `latest.json`, commande
  « Vérifier les mises à jour ».
- **Onboarding** : guide de démarrage au premier déverrouillage, rouvrable.
- **Docs** : guides d'installation, de configuration MCP, Supabase, S3, Drive,
  FAQ, modèle de menace, revue de sécurité, procédure de release.

### Sécurité

- CSP stricte (plus de `null`), permission presse-papiers en lecture retirée.
- Overrides de test Google Drive compilés uniquement en debug.
- Modèle de menace et revue 9.1 publiés ; tests de non-fuite exécutés.

### Limites connues

- Windows (named pipe, manifeste de registre) et Safari non livrés.
- Recette Google Drive sur un compte réel à faire (validé sur émulateur d'API v3).
- Passkeys non découvrables / non attestées / sans vérification utilisateur.
- Notarisation macOS et signature Windows à configurer dans le canal de release.
