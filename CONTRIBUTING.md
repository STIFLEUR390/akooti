# Contribuer à Akooti

Merci de prendre le temps de contribuer. Ce dépôt contient des secrets
utilisateurs potentiels : les règles ci-dessous ne sont pas décoratives.

## Prérequis

- **Rust 1.90+** (toolchain épinglée par `rust-toolchain.toml`)
- **Bun 1.4+**
- Les dépendances système de Tauri v2 :

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel openssl-devel

# Ubuntu / Debian
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev build-essential

# macOS (Xcode Command Line Tools requis)
xcode-select --install

# Windows : WebView2 (fourni avec Windows 11) + Visual Studio Build Tools (C++)
```

## Installation

```bash
git clone https://github.com/STIFLEUR390/akooti.git
cd akooti
bun install
```

## Commandes utiles

| Commande                                                | Effet                                                    |
| ------------------------------------------------------- | -------------------------------------------------------- |
| `bun run tauri dev`                                     | Application desktop (Tauri + Vite + bridge MCP de debug) |
| `bun run dev:extension`                                 | Extension WXT en HMR                                     |
| `bun run build`                                         | Build desktop et extension                               |
| `bun run lint` / `bun run format`                       | ESLint / Prettier                                        |
| `bun run typecheck`                                     | `tsc --noEmit` sur chaque package                        |
| `bun test`                                              | Tests TypeScript (`bun:test`)                            |
| `cargo test --workspace`                                | Tests Rust                                               |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lints Rust                                               |

## Règles non négociables

Elles sont détaillées dans [`docs/regles.md`](../docs/regles.md).

1. **Zero-Leak** — aucun outil MCP ne retourne de mot de passe, de clé ou de
   code TOTP. Toute réponse d'outil est couverte par un test.
2. **Mémoire** — toute clé ou secret manipulé en Rust est encapsulé dans
   `zeroize::Zeroizing<T>`.
3. **Frontière** — `crates/akooti-core` n'importe jamais Tauri, WebView ou
   React. `src-tauri` ne contient que l'adaptation UI/IPC.
4. **Cloud** — aucun chiffrement/déchiffrement hors client ; aucun jeton MCP
   ni configuration de sauvegarde synchronisé.
5. **Logs** — jamais de secret, de clé ou de contenu déchiffré dans un log,
   une erreur, un test snapshot ou une capture d'écran commitée.

## Branches et commits

- Branches : `feat/…`, `fix/…`, `chore/…`, `docs/…`, `refactor/…`, `test/…`.
- Commits : [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat(mcp): add vault_search tool`).
- Un commit = un changement cohérent ; pas de commit de merge inutile
  (rebase sur `main` avant la PR).
- Pas de `--force` sur `main`.

## Tests

- Toute logique du cœur métier (`akooti-core`) doit avoir des tests unitaires
  ou d'intégration.
- Les tests de sécurité MCP doivent échouer si une réponse contient un secret.
- Les fixtures d'import doivent être anonymisées et minimales.
- Un correctif de sécurité doit inclure un test de non-régression.

## Pull requests

1. Décrivez le problème et le comportement attendu.
2. Cochez la checklist du template (sécurité, tests, docs).
3. Liez la tâche Kaneo ou l'issue correspondante.
4. Attendez une CI verte (Rust, Bun, build Tauri) avant de demander une revue.

## Signaler une vulnérabilité

N'ouvrez **pas** d'issue publique. Suivez
[`SECURITY`](.github/ISSUE_TEMPLATE/security.md) : rapport privé via les
GitHub Security Advisories de ce dépôt.
