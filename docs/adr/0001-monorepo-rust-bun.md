# 0001 — Monorepo Cargo + Bun, un binaire multi-modes

- **Statut :** acceptée
- **Date :** 2026-11-15

## Contexte

Akooti doit livrer une application desktop, un daemon local, un pont Native
Messaging, un serveur MCP et une extension navigateur. Multiplier les binaires
et les dépôts complique la distribution, la parité de version et le partage des
contrats.

## Décision

- Un **workspace Cargo** : `crates/akooti-core` (métier pur), `crates/akooti-mcp`
  (serveur MCP et moteurs d'injection), `src-tauri` (GUI, daemon, CLI).
- Un **workspace Bun** : `apps/desktop`, `apps/extension`, `packages/shared`
  (types et protocoles TypeScript partagés).
- **Un seul binaire `akooti`** exposant plusieurs modes :
  `akooti` (GUI + tray), `akooti native-host`, `akooti mcp --token`.

## Conséquences

- Une seule version à publier, un seul installeur, une seule source de contrat
  pour l'extension et le desktop.
- `akooti-core` reste sans dépendance UI : il est testable seul et portable.
- Le binaire embarque plusieurs responsabilités : le routage des modes doit
  rester explicite et testé (tâche 5.1).
- Bun est imposé comme gestionnaire de paquets (`bun.lock` commité) ; npm,
  pnpm et yarn ne sont pas supportés.
