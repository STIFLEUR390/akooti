# Tests de l'application desktop (phases 4 et 5)

Deux niveaux de vérification, tous deux rejouables :

1. **Automatisé** — `bun test`, `cargo test`, `cargo clippy`, ESLint, `tsc`,
   et l'audit `axe-core` (aucune violation critique ou sérieuse).
2. **Manuel** — parcours réel dans le binaire Tauri, piloté par le pont MCP
   (`tauri-plugin-mcp-bridge`, port `9223`), avec captures d'écran et instantanés
   DOM. C'est la preuve demandée par la tâche 4.2 (« tests manuels documentés :
   MCP Tauri screenshot + DOM »).

## Pré-requis

```bash
bun install
bun run --filter @akooti/desktop dev   # Vite sur :1420 (le binaire de dev y pointe)
cargo build -p akooti
./target/debug/akooti &
```

Puis, depuis un client MCP connecté au pont :

```
driver_session(action="start")
webview_dom_snapshot(windowId="main")
webview_screenshot(windowId="main")
```

## Parcours vérifié le 2026-11-16 (Linux, Fedora 45, WebKitGTK 2.54)

| # | Vérification | Résultat observé |
| --- | --- | --- |
| 1 | Écran de création : mot de passe faible, confirmation différente | Bouton « Créer le coffre » désactivé, messages explicites, avertissement d'irréversibilité affiché |
| 2 | Création d'un coffre (12+ caractères) | Trois dossiers système rendus dans la sidebar avec badge « système » et cadenas |
| 3 | Création d'une entrée (`Serveur local`, `http://localhost:4321`) | Entrée listée et sélectionnée ; mot de passe masqué (`••••••••••••`) |
| 4 | Routage automatique à la création | Chips « Dev Local » ajoutés sans retirer « Agents & MCP » (routage additif) |
| 5 | Révélation du mot de passe | Valeur affichée avec compte à rebours « Masqué dans 30 s », puis re-masquée automatiquement |
| 6 | Copie d'un secret | Toast « Mot de passe copié — effacement du presse-papiers dans 30 s » + badge de décompte |
| 7 | Purge du presse-papiers | `xclip -o` renvoie le secret à T+0 (24 octets) et **rien** à T+32 s |
| 8 | Secret TOTP | Graine écrite via la fenêtre « Secrets », code live affiché (`139 244`), barre de validité et « Renouvelé dans 20 s » |
| 9 | Raccourcis clavier | `Ctrl N` ouvre « Nouvelle entrée », `Échap` la ferme, `Ctrl B` copie l'identifiant, `Ctrl T` copie le code TOTP, `/` place le focus dans la recherche |
| 10 | Thème | Bascule sombre → clair : `data-theme="light"`, `color-scheme: light` ; préférence conservée localement |
| 11 | Page Routage | 5 règles par défaut listées, désactivation d'une règle persistée, ajout d'une règle `.internal`, tri rétroactif rapportant « 0 appartenance(s) ajoutée(s) » |
| 12 | Page Import | 11 formats proposés par le backend, bouton « Analyser » désactivé sans fichier |
| 13 | Import Bitwarden (JSON, 2 entrées) | Aperçu : 2 entrées / 1 dossier / 0 doublon ; import : 2 importées, 1 routée Dev Local, 0 ignorée |
| 14 | Import avec effacement de la source | Rapport `cleanup: completed`, fichier absent du disque ensuite |
| 15 | Import avec doublons autorisés | Rapport `imported: 2`, `skipped: 0`, entrées dupliquées visibles dans la liste |
| 16 | Modes headless | `akooti help` et `akooti version` répondent ; `akooti mcp` sans jeton sort en code 2 ; `akooti native-host` sort en code 3 |

## Audit d'accessibilité

`axe-core` (via `agent_browser_a11y`) sur `http://localhost:1420` :

| Écran | Thème | Violations |
| --- | --- | --- |
| Création / déverrouillage | sombre | 0 |
| Coffre (liste + détail) | sombre | 0 |
| Coffre (liste + détail) | clair | 0 |
| Générateur (modale) | clair | 0 |
| Nouvelle entrée (modale) | clair | 0 |

Violations corrigées pendant la phase :

- `aria-meter-name` : la jauge de force a désormais un `aria-label` propre à
  chaque contexte (« Force du mot de passe », « Force du secret généré »).
- `color-contrast` : `--text-faint` éclairci en thème sombre (`#7d8fa3`) et
  assombri en thème clair (`#5f6c80`) ; le bouton primaire utilise
  `--accent-contrast` (texte sombre sur l'accent vif).
- `page-has-heading-one` : le nom de marque de la sidebar est un `<h1>`.
- `label` : le champ mot de passe du formulaire d'entrée porte un `aria-label`.

## Phase 5 — jetons, daemon et serveur MCP

### Vérifié automatiquement le 2026-11-17

| # | Vérification | Preuve |
| --- | --- | --- |
| 1 | Cœur RBAC : 9 tests d'intégration (jeton retourné une fois, refus d'un jeton inconnu/révoqué/expiré, recherche cloisonnée, plan d'injection, compte éphémère + TTL, `exec_env`, audit sans secret) | `cargo test -p akooti-core --test mcp_access_test` |
| 2 | Protocole IPC : cadrage, plafond 1 Mio, version, erreur distante, reconnexion | `cargo test -p akooti-ipc` |
| 3 | Serveur MCP : refus = erreur d'outil avec code stable, résultats de recherche sans champ secret, taille de page bornée | `cargo test -p akooti-mcp` |
| 4 | Daemon : masquage `stdout`/`stderr` de `vault_exec` (valeur brute, URL-encodée et JSON), échec explicite si le CDP ne répond pas, pont extension déclaré `unavailable` | `cargo test -p akooti` |
| 5 | Chaîne complète `akooti mcp` → socket → daemon factice | `initialize`, `tools/list` (6 outils), `tools/call vault_search` et `vault_status` répondent en JSON-RPC, aucune valeur secrète dans les réponses |

Parcours manuel de la nouvelle page **Accès IA** (à rejouer dans le binaire) :

1. Créer un jeton `read_metadata` sur `Dev Local` : le secret s'affiche une seule
   fois, avec le bouton « Copier » et l'avertissement d'unicité.
2. Recharger la page : le secret n'apparaît plus, seuls le préfixe
   (`ak_mcp_`) et les portées sont listés.
3. Révoquer le jeton : il passe à l'état révoqué, le bouton devient
   « Supprimer ».
4. Configurer un client MCP avec `akooti mcp --token …` et appeler
   `vault_search` : les résultats ne contiennent ni mot de passe ni graine.
5. Vérifier que la ligne d'audit correspondante apparaît dans « Activité des
   agents » avec l'origine.

## Ce qui n'est pas couvert ici

- Le déverrouillage biométrique : aucun fournisseur n'est branché, l'UI masque
  le contrôle (voir `docs/adr/0005-biometrie-deverrouillage.md`).
- Le pont Native Messaging : `vault_blind_autofill` répond `unavailable` tant
  que l'extension n'est pas branchée ; utiliser `vault_cdp_autofill`.
- Le transport Windows (named pipe) : voir
  `docs/adr/0006-transport-ipc-socket-unix.md`.
