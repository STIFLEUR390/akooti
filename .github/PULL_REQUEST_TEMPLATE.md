## Résumé

<!-- Ce que fait cette PR, et pourquoi. Liez la tâche Kaneo / l'issue. -->

## Type de changement

- [ ] Fonctionnalité (`feat`)
- [ ] Correction (`fix`)
- [ ] Refactorisation (`refactor`)
- [ ] Documentation (`docs`)
- [ ] Outillage / CI (`chore`)

## Checklist sécurité

- [ ] Aucun secret, clé, TOTP ou contenu déchiffré dans le code, les logs, les tests ou les captures.
- [ ] Les nouveaux secrets manipulés en Rust sont dans `zeroize::Zeroizing<T>`.
- [ ] Aucune réponse d'outil MCP ne contient de secret (test Zero-Leak à jour si concerné).
- [ ] Les permissions de jeton MCP et l'audit sont respectés.
- [ ] `akooti-core` n'a gagné aucune dépendance Tauri / WebView / React.
- [ ] La configuration cloud reste côté client (aucune clé envoyée au serveur).

## Tests

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `bun run lint && bun run typecheck && bun run test`
- [ ] Test manuel via le bridge MCP Tauri si l'UI change

## Notes de revue

<!-- Points d'attention, dette assumée (`shortcut:`), suivi post-merge. -->
