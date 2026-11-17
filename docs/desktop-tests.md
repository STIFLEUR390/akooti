# Recette de l'application desktop

Ce document est le **plan de recette** du binaire réel : ce que l'on vérifie,
comment le rejouer, et ce que l'on a observé. Il couvre la phase 4 (interface
du coffre), la phase 5 (jetons, daemon, serveur MCP) et la phase 6 (extension
navigateur et pont Native Messaging). La couverture automatisée est dans
[testing.md](./testing.md).

Deux niveaux, complémentaires :

1. **Automatisé** — `cargo test`, `bun test`, `clippy`, ESLint, `tsc`,
   `axe-core` : rapide, rejouable en CI, mais aveugle à l'application réelle.
2. **Manuel piloté** — parcours dans le binaire Tauri, conduit par le pont MCP
   (`tauri-plugin-mcp-bridge`, port `9223`), avec instantanés DOM et captures.
   C'est la preuve attendue par la tâche 4.2 (« tests manuels documentés : MCP
   Tauri screenshot + DOM »).

## 1. Mise en place

```bash
bun install
bun run --filter @akooti/desktop dev   # Vite sur :1420, le binaire de dev y pointe
cargo build -p akooti
./target/debug/akooti
```

Recommandations :

- **Utiliser un profil XDG jetable** pour ne jamais toucher le coffre réel :
  `XDG_DATA_HOME=/tmp/akooti-recette ./target/debug/akooti`. Un coffre créé
  dans `/tmp` disparaît au redémarrage : c'est voulu.
- Le pont **n'existe qu'en build debug** : il n'est pas compilé en production.
- Vite doit pouvoir prendre `:1420` (mode `--strictPort`) ; sinon libérer le port
  avant de relancer.

## 2. Piloter la session

Depuis un client connecté au pont :

```text
driver_session(action="start")
webview_dom_snapshot(windowId="main", type="accessibility")
webview_screenshot(windowId="main")
ipc_execute_command({ command: "list_mcp_audit_log", args: { limit: 20 } })
```

- `curl http://127.0.0.1:9223/` renvoie `000` même quand le pont est sain : ce
  n'est pas une sonde fiable.
- Les commandes IPC s'appellent **après** déverrouillage ; sinon elles répondent
  `locked`.
- Un instantané DOM après chaque action vaut mieux qu'une capture : il donne les
  libellés accessibles, donc l'état réel des contrôles.

## 3. Phase 4 — parcours de l'interface

Vérifié le 2026-11-16 (Linux, Fedora 45, WebKitGTK 2.54).

| #  | Vérification | Résultat observé |
| -- | ------------ | ---------------- |
| 1  | Création : mot de passe faible, confirmation différente | Bouton « Créer le coffre » désactivé, messages explicites, avertissement d'irréversibilité affiché |
| 2  | Création d'un coffre (12+ caractères) | Trois dossiers système dans la sidebar, badge « système » et cadenas |
| 3  | Création d'une entrée (`Serveur local`, `http://localhost:4321`) | Entrée listée et sélectionnée, mot de passe masqué (`••••••••••••`) |
| 4  | Routage automatique à la création | Chip « Dev Local » ajouté **sans** retirer « Agents & MCP » (routage additif) |
| 5  | Révélation du mot de passe | Valeur affichée avec compte à rebours « Masqué dans 30 s », puis re-masquée |
| 6  | Copie d'un secret | Toast « Mot de passe copié — effacement du presse-papiers dans 30 s » + badge de décompte |
| 7  | Purge du presse-papiers | `xclip -o` renvoie le secret à T+0 (24 octets) et **rien** à T+32 s |
| 8  | Secret TOTP | Graine saisie via « Secrets », code live (`139 244`), barre de validité et « Renouvelé dans 20 s » |
| 9  | Raccourcis clavier | `Ctrl N` ouvre « Nouvelle entrée », `Échap` referme, `Ctrl B` copie l'identifiant, `Ctrl T` copie le code TOTP, `/` focalise la recherche |
| 10 | Thème | Bascule sombre → clair : `data-theme="light"`, `color-scheme: light`, préférence conservée |
| 11 | Page Routage | 5 règles par défaut, désactivation persistée, ajout d'une règle `.internal`, tri rétroactif « 0 appartenance(s) ajoutée(s) » |
| 12 | Page Import | 11 choix proposés par le backend, bouton « Analyser » désactivé sans fichier |
| 13 | Import Bitwarden (JSON, 2 entrées) | Aperçu 2 entrées / 1 dossier / 0 doublon ; import : 2 importées, 1 routée `Dev Local`, 0 ignorée |
| 14 | Import avec effacement de la source | Rapport `cleanup: completed`, fichier absent du disque ensuite |
| 15 | Import avec doublons autorisés | Rapport `imported: 2`, `skipped: 0`, doublons visibles dans la liste |
| 16 | Modes headless | `akooti help` et `akooti version` répondent ; `akooti mcp` sans jeton sort en 2 ; `akooti native-host` sans daemon écrit une trame `daemon_unavailable` et sort en 1 (il sortait en 3 tant que le pont était un talon) |

## 4. Accessibilité

Audit `axe-core` (via `agent_browser_a11y`) sur `http://localhost:1420` :

| Écran | Thème | Violations critiques ou sérieuses |
| ----- | ----- | --------------------------------- |
| Création / déverrouillage | sombre | 0 |
| Coffre (liste + détail) | sombre | 0 |
| Coffre (liste + détail) | clair | 0 |
| Générateur (modale) | clair | 0 |
| Nouvelle entrée (modale) | clair | 0 |

Violations corrigées pendant la phase :

- `aria-meter-name` — chaque jauge de force porte son propre libellé (« Force du
  mot de passe », « Force du secret généré »).
- `color-contrast` — `--text-faint` éclairci en thème sombre (`#7d8fa3`) et
  assombri en thème clair (`#5f6c80`) ; le bouton primaire utilise
  `--accent-contrast` (texte sombre sur accent vif).
- `page-has-heading-one` — la marque de la sidebar est un `<h1>`.
- `label` — le champ mot de passe du formulaire d'entrée porte un `aria-label`.

## 5. Phase 5 — jetons, daemon et serveur MCP

### 5.1 Preuves automatisées (2026-11-17)

| # | Ce qui est prouvé | Preuve |
| - | ----------------- | ------ |
| 1 | RBAC du cœur : secret retourné une fois, refus d'un jeton inconnu / révoqué / expiré, recherche cloisonnée, plan d'injection, compte éphémère + TTL, `exec_env`, audit sans secret | `cargo test -p akooti-core --test mcp_access_test` (9 tests) |
| 2 | Protocole IPC : cadrage, plafond 1 Mio, version, erreur distante, reconnexion | `cargo test -p akooti-ipc` (17 tests) |
| 3 | Serveur MCP : refus = erreur d'outil à code stable, résultats sans champ secret, taille de page bornée | `cargo test -p akooti-mcp` (5 tests) |
| 4 | Daemon : masquage `stdout`/`stderr` de `vault_exec` (valeur brute, URL-encodée, JSON), échec explicite si CDP muet, pont extension déclaré `unavailable` | `cargo test -p akooti` |
| 5 | Chaîne `akooti mcp` → socket → daemon factice : `initialize`, `tools/list` (6 outils), `tools/call` de `vault_search` et `vault_status` en JSON-RPC, aucune valeur secrète | recette du 2026-11-17 |

### 5.2 Parcours manuel de la page « Accès IA »

1. Créer un jeton `read_metadata` sur `Dev Local` : le secret s'affiche **une
   seule fois**, avec le bouton « Copier » et l'avertissement d'unicité.
2. Recharger la page : plus de secret, seuls le préfixe (`ak_mcp_…`) et les
   portées restent visibles.
3. Révoquer le jeton : état « révoqué », le bouton devient « Supprimer ».
4. Configurer un client MCP (`akooti mcp --token …`) et appeler `vault_search` :
   les résultats ne contiennent ni mot de passe ni graine.
5. Vérifier que la ligne d'audit apparaît dans « Activité des agents » avec
   l'origine et le statut.

### 5.3 Recette de bout en bout des outils

Terrain de test : une page de connexion servie en local (`127.0.0.1:8808`) et un
Chrome headless exposant DevTools (`--remote-debugging-port=9222`). Les jetons
utilisés sont jetables et cloisonnés.

| Outil | Appel | Résultat attendu |
| ----- | ----- | ---------------- |
| `vault_status` | `{}` | nom, portées, dossiers autorisés, expiration du jeton |
| `vault_search` | `{"query":"test"}` | métadonnées seules, limitées aux dossiers du jeton |
| `vault_cdp_autofill` | `{"itemId":…,"origin":"http://127.0.0.1:8808","selectors":{"totp":"#totp"}}` | `status: filled`, `fieldsFilled: [username, password, totp]`, `submitted: true`, aucune valeur |
| `vault_cdp_autofill` (origine erronée) | origine `http://127.0.0.1:9999` | `status: failed`, `detail` « no page target matches the requested origin », audit `error` |
| `vault_create_and_fill` | sans `selectors` | `status: partial` (« no selector matched ») et **une seule** ligne d'audit `create_and_fill` en `error` |
| `vault_exec` | `{"itemId":…,"command":["sh","-c","echo $AKOOTI_PASSWORD"]}` | sortie `***`, environnement limité à `AKOOTI_USERNAME`, `AKOOTI_PASSWORD` et `AKOOTI_<CHAMP>` |
| `vault_blind_autofill` | tout appel | `unavailable` — le pont natif ne sert que les remplissages initiés par l'extension ([ADR 0008](./adr/0008-extension-native-messaging.md)) |

### 5.4 Défauts trouvés par cette recette (et corrigés)

Un test manuel n'a de valeur que s'il casse quelque chose. Ce parcours a produit
cinq constats, tous traités avant la fusion de la phase :

| Constat | Cause | Correctif |
| ------- | ----- | --------- |
| Un secret TOTP de 80 bits (format Google Authenticator) était refusé | validation exigeant 128 bits | décodage Base32 conservé comme seul refus ([ADR 0007](./adr/0007-fills-et-secrets-totp-courts.md)) |
| `vault_create_and_fill` créait le compte sans jamais remplir le formulaire | `selectors` acceptés par le MCP mais perdus avant le daemon | `selectors` transmis de bout en bout, avec repli sur ceux de l'entrée |
| Un remplissage totalement raté était journalisé `success` | statut déduit de l'absence d'erreur | `partial` (aucun champ rempli) requalifié en audit `error` |
| Deux lignes d'audit pour un seul appel | écriture côté MCP et côté daemon | une action = une ligne, écrite par le daemon une fois l'issue connue |
| Un refus de politique nommait un helper interne | message construit bas | le détail porte le nom de l'action demandée (`cdp_autofill refused by policy`) |

Ces cinq points sont désormais couverts par des tests : la recette manuelle sert à
trouver, les tests servent à ne pas régresser.

## 6. Phase 6 — extension navigateur et pont Native Messaging

> **État : plan écrit, pas encore déroulé.** Aucun navigateur n'a été piloté
> pendant l'implémentation : les lignes de §6.3 sont des **attendus**, pas des
> observations. La part automatisée correspondante est en
> [testing.md](./testing.md) §4.4, §4.5 et §7.

La phase 6 livre deux moitiés qui ne se connaissent pas : l'extension WXT (un
source, Chrome/Edge/Brave en MV3 et Firefox en MV2) et `akooti native-host`, une
**pompe à trames** qui ne désérialise rien (voir
[ADR 0008](./adr/0008-extension-native-messaging.md)). C'est cette séparation que
la recette doit mettre à l'épreuve : un hôte qui ne comprend pas les trames ne
peut pas laisser fuiter un secret, une extension qui ne garde rien ne peut pas
devenir un second coffre.

### 6.1 Préparer les deux moitiés

```bash
bun run --filter @akooti/extension build           # .output/chrome-mv3  (MV3)
bun run --filter @akooti/extension build:firefox   # .output/firefox-mv2 (MV2)
cargo build -p akooti                              # ./target/debug/akooti
```

1. **Charger l'extension.** Chrome : `chrome://extensions` → mode développeur →
   « Charger l'extension non empaquetée » → `.output/chrome-mv3`. Noter
   l'identifiant affiché (32 lettres `a`–`p`) : c'est **lui seul** que le
   manifeste doit autoriser. Firefox : `about:debugging#/runtime/this-firefox` →
   « Charger un module temporaire » → `.output/firefox-mv2/manifest.json` ;
   l'identifiant est épinglé (`extension@akooti.local`, à la fois dans
   `apps/extension/wxt.config.ts` et dans
   `native_register::FIREFOX_EXTENSION_ID`).
2. **Enregistrer l'hôte.** Une commande par navigateur, sans privilège :

   ```bash
   ./target/debug/akooti native-register --browser chrome --extension-id <id>
   # chrome: installed (/home/…/.config/google-chrome/NativeMessagingHosts/org.akooti.host.json)
   ./target/debug/akooti native-register --browser chrome --extension-id <id> --check
   # chrome: current (…)
   ./target/debug/akooti native-register --browser firefox
   # firefox: installed (/home/…/.mozilla/native-messaging-hosts/org.akooti.host.json)
   ./target/debug/akooti native-register --browser chrome --uninstall
   ```

   `--check` est une **barrière** (CI, script d'installation) : `missing`,
   `outdated` et `unreadable` sortent en 1. `--home` et `--host` permettent de
   rejouer toute la séquence dans un profil jetable
   (`--home /tmp/akooti-recette`), sans toucher aux manifestes réels.
3. **Appairer l'extension.** Dans Akooti, page **Accès IA** : créer un jeton
   portant les portées `read_metadata` et `blind_fill`, restreint aux dossiers
   que l'extension peut lire. Le secret ne s'affiche **qu'une fois** : le coller
   dans le popup (§6.2). C'est l'unique identifiant de l'extension, il ne quitte
   jamais le navigateur et le daemon le révoque comme les autres.

### 6.2 Parcours du popup

| # | Geste | Attendu |
| - | ----- | ------- |
| 1 | Ouvrir le popup sans appairage | pastille « Non appairée », champ `ak_mcp_…`, bouton « Appairer » **désactivé** tant que le jeton n'a pas la forme `ak_mcp_` + 64 hexadécimaux |
| 2 | Coller un jeton tronqué | message « Un jeton Akooti commence par `ak_mcp_` suivi de 64 caractères hexadécimaux », rien n'est écrit |
| 3 | Coller un jeton valide, appairer | pastille « Coffre déverrouillé », pied de page `Jeton ak_mcp_xxxx…yyyy` : le secret entier n'est jamais réaffiché |
| 4 | Fermer l'application, rouvrir le popup | pastille « Application fermée », aucune donnée en cache, l'extension ne bloque pas le navigateur |
| 5 | Verrouiller le coffre | pastille « Coffre verrouillé », la recherche répond « Coffre verrouillé » (`locked`) |
| 6 | Rechercher | résultats sans mot de passe, sans graine et sans champ secret ; une ligne d'audit `search` par envoi, jamais une par frappe |
| 7 | « Remplir » sur la page de test (`127.0.0.1:8808`) | champs identifiant / mot de passe / code remplis, rapport `fieldsFilled`, **aucune valeur** affichée dans le popup |
| 8 | « Remplir » depuis une page dont l'origine ne correspond pas à l'URL de l'entrée | titre « Action refusée » (`denied`), rien n'est injecté, audit en `error` |
| 9 | « Copier · mot de passe » | presse-papiers rempli, toast, valeur jamais rendue dans le DOM |
| 10 | « Capturer » sur un formulaire | sélecteurs CSS affichés dans le popup puis « Copier les sélecteurs » ; le pont **n'écrit pas** dans le coffre, la colle est manuelle |
| 11 | Passkey sur un site de test (`https://webauthn.io`) | le shim est injecté (`script[data-akooti-nonce]`), le daemon répond `unavailable`, le navigateur reprend la main et la cérémonie aboutit (repli assumé, tâche 6.4) |
| 12 | Consulter « Activité des agents » | une ligne `blind_autofill` par remplissage, avec origine et statut, sans valeur |
| 13 | « Dissocier » | jeton retiré de `storage.local`, retour au formulaire d'appairage, le daemon refuse l'ancien jeton s'il est réutilisé |

### 6.3 Preuves de non-fuite propres à la phase

Un remplissage réussi ne suffit pas : la phase 6 n'est valide que si l'on
démontre **l'absence** du secret là où il pourrait fuiter.

| Où l'on regarde | Attendu |
| --------------- | ------- |
| Sortie standard de l'hôte natif | vide : la pompe ne journalise ni ne décode aucune trame |
| `chrome://extensions` → « Erreurs » / console du service worker | aucune valeur, aucun jeton en clair |
| `browser.storage.local` | la seule clé `akooti.pairing` (jeton d'appairage), aucune valeur du coffre |
| Ligne d'audit `blind_autofill` | origine, identifiant d'entrée et statut seulement |
| `akooti mcp` → `vault_blind_autofill` | erreur d'outil `forbidden` : le verbe qui libère des valeurs reste fermé aux agents |
| `curl` sur un port local | aucune écoute ouverte par l'extension : la seule voie est le socket Unix du daemon |

### 6.4 Défauts attendus de cette recette

Cette section se remplit pendant le déroulement. Les points déjà connus, non
corrigés, y figurent d'avance pour ne pas les redécouvrir :

| Constat connu | Raison | Suivi |
| ------------- | ------ | ----- |
| La cérémonie WebAuthn retombe toujours sur le navigateur | aucun authentificateur côté daemon | tâche 6.4, [ADR 0008](./adr/0008-extension-native-messaging.md) |
| Windows : ni manifeste (registre) ni socket | transport nommé non implémenté | [ADR 0006](./adr/0006-transport-ipc-socket-unix.md) |
| Un credential reconstruit n'est pas une instance de `PublicKeyCredential` | objet compatible seulement | `apps/extension/lib/webauthn/credential.ts` |

### 6.5 Preuves automatisées (2026-11-17)

| # | Ce qui est prouvé | Preuve |
| - | ----------------- | ------ |
| 1 | La pompe n'altère rien : trames recopiées dans les deux sens, plafond 1 Mio, arrêt propre sur EOF ou port fermé, erreur typée si le daemon est absent | `cargo test -p akooti native_host` (4 tests) |
| 2 | Manifestes : chemin par navigateur et par OS, `allowed_origins` vs `allowed_extensions`, id validé avant toute écriture, installation idempotente, réparation, désinstallation | `cargo test -p akooti native_register` (10 tests) |
| 3 | Trame `vaultExtensionFill` : forme dorée `camelCase`, version 2, `FillPayload` jamais affiché | `cargo test -p akooti-ipc` |
| 4 | Une connexion qui ne s'est pas annoncée `native-host` ne peut pas déclencher de remplissage (`access_denied`) ; le serveur MCP traduit `extensionFill` en erreur `forbidden` | `cargo test -p akooti`, `cargo test -p akooti-mcp` |
| 5 | Extension : protocole, bus interne, appairage, pont natif, heuristiques DOM, WebAuthn | `cd apps/extension && bun test lib` (78 tests) |

## 7. Grille à recopier pour une nouvelle phase

```text
Phase :            …            Date : …            OS / WebKit : …
Coffre :           jetable (XDG_DATA_HOME=/tmp/…)  ☐
Déverrouillé :     ☐            Pont MCP actif : 9223 ☐            Vite :1420 ☐
Pont natif :       manifeste --check ☐   popup appairé ☐   sortie de l'hôte vide ☐

#  Parcours                     Attendu                      Observé        OK
1  …                            …                            …              ☐
2  …                            …                            …              ☐

Accessibilité (axe-core) : écrans audités …, violations critiques/sérieuses : …
Audit MCP : lignes attendues …, lignes observées …
Fuites : sortie de l'hôte, console du worker, stockage local, ligne d'audit …
Défauts trouvés : … (chacun devient un correctif + un test)
```

## 8. Ce que cette recette ne couvre pas

| Sujet | Raison | Suivi |
| ----- | ------ | ----- |
| Déverrouillage biométrique | aucun fournisseur branché, l'UI masque le contrôle | [ADR 0005](./adr/0005-biometrie-deverrouillage.md) |
| Cérémonie WebAuthn servie par le coffre | le shim intercepte, mais le daemon n'a pas d'authentificateur : `handled: false` et repli navigateur | tâche 6.4, [ADR 0008](./adr/0008-extension-native-messaging.md) |
| Remplissage piloté par un agent via le pont natif | le pont ne sert que les remplissages initiés par l'extension (`vaultExtensionFill`) | [ADR 0008](./adr/0008-extension-native-messaging.md) |
| Transport IPC et enregistrement Windows | named pipe non implémenté, manifeste stocké dans le registre | [ADR 0006](./adr/0006-transport-ipc-socket-unix.md) |
| Sync Supabase, sauvegardes, santé du coffre | non implémentés | [fonctionalites.md](./fonctionalites.md) |
| Packaging et mise à jour | hors périmètre des phases 5 et 6 | phase ultérieure |
