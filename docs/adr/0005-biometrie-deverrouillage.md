# 0005 — Biométrie : trait présent, fournisseur non branché

- **Statut :** acceptée
- **Date :** 2026-11-16

## Contexte

`docs/regles.md` §3.2 prévoit qu'un dossier critique puisse exiger une
confirmation biométrique (Windows Hello / Touch ID / Polkit) avant toute
injection IA, et la tâche 4.2 demande « une option biométrique si disponible »
sur l'écran de déverrouillage.

La vérification des sources officielles (`find-docs` / Context7) donne ceci :

| Plateforme | `tauri-plugin-biometric` | Alternative native |
| --- | --- | --- |
| macOS | oui (LocalAuthentication) | — |
| iOS / Android | oui | — |
| Linux | **non** | `fprintd` via D-Bus (interface `net.reactivated.Fprint`) |
| Windows | **non** | Windows Hello via WinRT `UserConsentVerifier` |

Le plugin ne couvre donc ni Linux (la cible de développement actuelle) ni
Windows. En outre, un déverrouillage biométrique réellement *passwordless*
exigerait une clé maître scellée dans le trousseau du système
(`keyring`/Credential Manager), donc une nouvelle API de cœur
`open_with_master_key` : hors du périmètre de la phase 4.

## Décision

- La **surface de commande existe** (`biometrics_status`) et renvoie une
  capacité typée : `available`, `provider` (`touchId` / `windowsHello` /
  `fprintd`) et une raison affichable.
- **Aucun fournisseur n'est branché en phase 4** : `available` vaut `false`
  partout, donc l'UI **masque** le bouton biométrique et affiche la raison.
  Aucun faux succès n'est possible : il n'existe aucune fonction `verify`
  exportée.
- Le mot de passe maître reste **le seul facteur de déverrouillage**.
- Linux (`fprintd`/D-Bus) et Windows (`UserConsentVerifier`) seront branchés
  plus tard, derrière la même commande, sans changer le contrat de l'UI.

## Alternatives écartées

- Déclarer `available: true` sur macOS dans l'espoir que le plugin fonctionne :
  il exige une *entitlement* et une signature de bundle ; en build non signé,
  l'appel échoue et l'utilisateur croit à une protection inexistante.
- Passer par `pkexec`/Polkit pour le déverrouillage : Polkit authentifie une
  *action système*, pas l'ouverture d'un coffre ; le mot de passe saisi serait
  celui de la session, pas le mot de passe maître.
- Embarquer `libfprint` en dépendance C : alourdit le binaire (< 20 Mo exigé) et
  casse la compilation sur les plateformes sans `fprintd`.

## Conséquences

- Le contrôle `biometrics_status` est couvert par un test qui vérifie qu'aucune
  plateforme ne se déclare disponible.
- L'écran de déverrouillage affiche la raison (`Aucun lecteur d'empreintes
  branché…`) au lieu d'un bouton inerte.
- Quand la fonctionnalité arrivera :
  1. sceller la clé maître dans le trousseau système ;
  2. ajouter `open_with_master_key` au cœur ;
  3. brancher `fprintd` (Linux) et `UserConsentVerifier` (Windows) derrière
     `biometrics_status` ;
  4. exiger la biométrie avant l'injection MCP sur les dossiers marqués
     `require_biometric_for_mcp`.
