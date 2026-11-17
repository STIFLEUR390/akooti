# Tests de l'application desktop (phase 4)

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

## Ce qui n'est pas couvert ici

- Le déverrouillage biométrique : aucun fournisseur n'est branché, l'UI masque
  le contrôle (voir `docs/adr/0005-biometrie-deverrouillage.md`).
- Les outils MCP, les jetons et l'audit : phase 5.
- L'extension et le pont Native Messaging : phase 6.
