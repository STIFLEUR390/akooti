# 0008 — Extension navigateur : pont Native Messaging, remplissage piloté par l'extension

- **Statut :** acceptée
- **Date :** 2026-11-17
- **Contexte de découverte :** phase 6 — extension WXT multi-navigateurs

## Contexte

La phase 6 branche le coffre au navigateur : l'utilisateur veut remplir une page
sans passer par un agent, copier un secret depuis une popup, et à terme faire
servir ses passkeys par le coffre.

Trois contraintes déjà écrites encadrent ce branchement :

1. **SEC-2** — un secret ne circule que de la RAM du binaire vers la cible
   finale, ici la page ou le presse-papiers.
2. **SEC-1** — aucun outil MCP ne retourne de valeur secrète ; l'agent ne reçoit
   qu'une issue de remplissage.
3. **SEC-6** — le socket local est le seul canal entre le daemon et ses
   sous-processus : pas de serveur HTTP local, pas de port en écoute.

Il fallait donc faire traverser des valeurs à un nouveau processus (l'extension)
sans ouvrir un canal que l'agent pourrait emprunter, et sans donner au pont
natif la capacité de lire ce qu'il transporte.

## Décision

### 1. Le pont natif est une pompe de trames, pas un interprète

`akooti native-host` relie stdin/stdout (Native Messaging, longueur 4 octets LE
en tête, plafond 1 Mio par trame) au socket du daemon. Il **ne désérialise pas**
les trames : il les recopie. Il n'écrit ni ne lit aucun secret, ne journalise
rien, et son arrêt correspond à la fermeture du port par le navigateur. Sa
surface d'attaque est celle d'un `copy` binaire.

Conséquence directe : le contrat de l'extension est celui du daemon
(`crates/akooti-ipc`), pas un protocole intermédiaire inventé pour la circonstance.
La version de protocole passe de `1` à `2` et tous les champs sont en
`camelCase` des deux côtés ; un désaccord de version rend le pont « collant »
(erreur `protocol` jusqu'à `reset`), ce qui évite de faire tourner une extension
et un daemon incompatibles.

### 2. Un verbe dédié, `vaultExtensionFill`, réservé au pont

`Request::VaultExtensionFill { token, itemId, origin, selectors, target }` est le
**seul** message qui retourne des valeurs (`ResponseBody::ExtensionFill`). Il est
verrouillé par quatre conditions cumulatives :

- **client** : la connexion doit s'être annoncée `native-host` au handshake ; une
  connexion `mcp` est refusée (`forbidden`) et auditée `access_denied` **avant**
  tout déchiffrement ;
- **portée** : `blind_fill` ;
- **dossiers** : la liste autorisée du jeton ;
- **origine** : la page doit correspondre à une URI revendiquée par l'entrée,
  même règle que `vault_cdp_autofill`.

Chaque libération écrit **une** ligne d'audit `blind_autofill` portant le nombre
de valeurs et le geste demandé (`fill` ou `clipboard`), jamais une valeur.

### 3. Le serveur MCP ne sait pas lire cette réponse

`akooti-mcp` reçoit un jour `ExtensionFill` ; il le traduit en **erreur d'outil
`forbidden`** sans jamais exposer le corps. C'est le garde-fou de dernier recours
si un agent parvenait à invoquer le verbe : le contrat de lecture de l'agent ne
contient aucune branche capable de restituer une valeur.

`vault_blind_autofill` reste annoncé et répond `unavailable` : un agent reçoit
une réponse vraie (« le pont n'est pas branché de ce côté-ci ») au lieu d'une
capacité qui disparaît silencieusement.

### 4. L'appairage est un jeton MCP ordinaire

L'utilisateur crée un jeton dans **Accès IA** (portées `read_metadata` et
`blind_fill`, restreint aux dossiers que l'extension peut lire) et le colle dans
la popup. Ce jeton est le seul identifiant de l'extension :

- il est stocké dans `storage.local` (jamais `storage.sync`, qui partirait dans
  le cloud du navigateur), affiché masqué, effacé par « Dissocier » ;
- il est vérifié (`vaultStatus`) **avant** d'être enregistré : un jeton refusé
  n'est jamais persisté ;
- révoquer le jeton dans l'application coupe l'extension immédiatement, sans
  désinstallation.

### 5. Les passkeys passent par un shim en monde MAIN, nonce à l'appui

`navigator.credentials` ne peut être remplacé que dans le monde de la page.
`webauthn.content.ts` (ISOLATED) injecte donc `webauthn-main.js` comme ressource
web-accessible, lui transmet un nonce de page et relaie chaque cérémonie vers le
daemon. Le nonce empêche la page de fabriquer une réponse, et le `rpId` demandé
est vérifié contre l'hôte de l'origine avant tout relais.

Toute cérémonie que le daemon ne sait pas servir retombe sur l'authentificateur
du navigateur : activer Akooti ne casse aucun site.

### 6. Ce que l'extension ne fait jamais

- **Écrire dans le coffre** : la capture de sélecteurs s'affiche dans la popup et
  se colle dans l'entrée ; un seul écrivain, l'application desktop.
- **Parler HTTP** : tout passe par Native Messaging.
- **Conserver une valeur** : la popup ne reçoit qu'un compte-rendu
  (`fieldsFilled`, `submitted`) ou un nom de champ ; le secret va du daemon à la
  page, ou au presse-papiers effacé après 30 s.

### 7. L'enregistrement des manifestes est une commande

`akooti native-register` écrit le manifeste que le navigateur lit avant de lancer
le pont : un chemin par navigateur et par OS, `allowed_origins` pour Chromium,
`allowed_extensions` pour Firefox. La commande sait **planifier** (`--check`),
réparer (manifeste obsolète), installer et supprimer (`--uninstall`), et refuse
un identifiant d'extension mal formé avant la moindre écriture.

## Alternatives écartées

- **Un petit serveur HTTP local pour l'extension** : interdit par SEC-6, et
  surtout vérifiable par n'importe quel processus local, donc par un agent.
- **Laisser `vault_blind_autofill` retourner les valeurs à l'agent** pour que
  l'extension les récupère par le MCP : viole SEC-1 au premier appel.
- **Faire du pont natif un client du protocole** (désérialiser, filtrer,
  réécrire) : il deviendrait un second interpréteur à maintenir, et le seul
  endroit où un secret existerait dans un processus sans clé.
- **Laisser l'extension écrire les sélecteurs capturés dans le coffre** : deux
  écrivains, deux schémas de validation, et un chemin d'écriture supplémentaire
  à défendre.
- **Stocker le jeton dans `storage.sync`** : pratique, mais le secret part chez
  le navigateur, ce qui contredit SEC-9 et AI-5.
- **Déclarer le shim WebAuthn en `world: "MAIN"`** dans le manifeste : plus
  simple, mais `world` n'existe pas dans `content_scripts` avant Firefox 128,
  alors que le module cible 115 ; l'injection par ressource web-accessible passe
  partout.

## Conséquences

- Le protocole passe en version `2` : une extension et un daemon de versions
  différentes refusent de travailler ensemble au lieu de se tromper.
- `crates/akooti-ipc/src/protocol.rs`, `crates/akooti-core/src/vault_mcp.rs` et
  les tests de `akooti-mcp` couvrent la non-fuite : `FillPayload` ne s'affiche
  jamais (`Zeroizing`, `Debug` masqué) et le MCP répond `forbidden`.
- L'extension est un quatrième consommateur du contrat : ses formes de messages
  sont épinglées par des tests de trames dorées, en face de celles de Rust.
- **Non couvert par cette décision** : l'authentificateur WebAuthn côté daemon
  (génération et signature de credentials) n'existe pas encore. Le shim sait
  recevoir un credential sérialisé et le reconstruire
  (`apps/extension/lib/webauthn/credential.ts`), le daemon répond
  `handled: false`, donc le repli navigateur est le seul chemin exercé
  aujourd'hui. C'est la tâche 6.4 restante, tracée dans
  [architecture.md](../architecture.md) §8.
- Windows reste hors périmètre : le manifeste Native Messaging y vit dans le
  registre, que [ADR 0006](./0006-transport-ipc-socket-unix.md) diffère déjà avec
  le named pipe.
