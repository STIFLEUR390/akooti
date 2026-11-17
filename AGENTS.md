# Repository Guidelines

## Layout and current state

Rust workspace (`crates/*`, `src-tauri`) plus Bun workspace (`apps/*`, `packages/*`).
Only part of the documented design is implemented; check `src/` before assuming a module exists.

- `crates/akooti-core` — the implemented product: crypto, SQLCipher, folders/items,
  generator, TOTP, routing, importers, session. It must never import Tauri, WebView or
  React, so it stays unit-testable and portable.
- `crates/akooti-core/src/importers/` — parsers normalize untrusted files into
  `ImportBundle`; only `importers/pipeline.rs` opens the SQLCipher transaction. Parse
  failures stay typed and must not echo decrypted values. `import_file` securely
  deletes the source only after commit and reports cleanup failures without rolling
  back the import.
- `crates/akooti-mcp` — stub (`src/lib.rs` only). The MCP server and injection engines
  land in phase 2.
- `src-tauri` — desktop shell for phase 4: `lib.rs` routes the CLI modes before any
  window (`cli.rs`), keeps headless `mcp`/`native-host` stubs (`headless.rs`), installs
  the tray (`tray.rs`) and owns the 15-minute vault lifecycle (`vault_runtime.rs`).
  `commands/` is the whole typed IPC surface (vault, folders, items, generator,
  routing, import/export, clipboard); `dto.rs` holds the serde payloads and `error.rs`
  the stable error codes. `biometrics.rs` reports no provider (ADR 0005).
- `apps/desktop` — the React/Vite vault UI: `lib/ipc.ts` is the typed command bridge
  (with an in-memory mock used outside Tauri), `hooks/` owns theme, shortcuts, reveal
  countdown and vault data, `components/` and `pages/` render the unlock screen, the
  three-column vault, the generator, the import wizard and the routing page.
  `apps/extension` — WXT with only `entrypoints/background.ts`.
- `packages/shared` — domain types and wire protocols, exported as raw `.ts` (no build
  step). Import as `@akooti/shared`; it is aliased in `apps/desktop/vite.config.ts` and
  `apps/desktop/tsconfig.json`.
- `README.md` and `docs/*.md` describe the target design and list files that do not
  exist yet (`health.rs`, `passkeys.rs`, `rbac.rs`, `sync_supabase.rs`, `backup/`,
  `skills/`, …). Trust manifests and `src/` over prose.
- Non-negotiable rules: `docs/regles.md`. Test ownership and critical coverage:
  `docs/testing.md`. Decisions: `docs/adr/`. README/CONTRIBUTING/docs are French;
  code, comments and commits are English.

## Commands

```bash
bun install                     # Bun workspaces; bun.lock is committed
bun run dev                     # desktop frontend only (Vite on :1420)
bun run tauri dev               # desktop app + debug MCP bridge
bun run dev:extension           # WXT HMR
bun run build                   # build every package that defines a build script
bun run lint                    # ESLint (flat config)
bun run format:check            # Prettier; ignores docs/ and bun.lock
bun run typecheck               # wxt prepare, then tsc per package
bun test                        # bun:test (currently only packages/shared)
bun test packages/shared/src/models.test.ts   # one TS suite
cargo test -p akooti-core --locked
cargo test -p akooti-core --test import_pipeline_test   # one integration suite
cargo test -p akooti-core <name>                        # one Rust test
cargo clippy -p akooti-core --all-targets --locked -- -D warnings
cargo fmt --all --check         # safe without Tauri system deps
```

- Workspace package names are scoped: `bun run --filter @akooti/desktop dev`. A bare
  `--filter desktop` matches nothing and exits 0 — a silent no-op.
- CI runs `bun install --frozen-lockfile` and cargo with `--locked`: commit `bun.lock`
  and `Cargo.lock` whenever dependencies change.
- CI JS order: lint → format:check → typecheck → test → build. CI Rust order:
  fmt → clippy → test → build over the whole workspace, then
  `bun run tauri build --no-bundle`.
- `cargo test/clippy/build --workspace` needs the Tauri system deps; locally use
  `-p akooti-core` for fast feedback and treat CI as the full compile gate.
  `cargo fmt` does not need those deps.
- TS tests run only through `bun test`; there is no per-app test script. Rust suites in
  `crates/akooti-core/tests/` hold the real coverage — share helpers via
  `tests/common/mod.rs`, use `tempfile` isolated vaults, and keep fixtures anonymized.
- `apps/desktop/build` runs `tsc --noEmit` before Vite, so `bun run build` also
  typechecks the desktop app.

## Tauri specifics

- System packages are listed in `CONTRIBUTING.md` (Fedora and Ubuntu). Without
  `webkit2gtk-4.1`/`gtk3` devel, workspace cargo commands and `bun run tauri dev` fail.
- Vite is pinned to `1420` with `strictPort`: a stale dev server makes `tauri dev` fail.
- `tauri build` triggers `beforeBuildCommand` (desktop build first). `frontendDist` is
  relative to `src-tauri/`, not the repo root (`../apps/desktop/dist`).
- `tauri-plugin-mcp-bridge` is registered only under `#[cfg(debug_assertions)]`, binds
  `127.0.0.1:9223`, and needs `withGlobalTauri: true` plus the `mcp-bridge:default`
  capability. After `bun run tauri dev`, connect with `driver_session`; screenshots/DOM/
  IPC tools target window `main`.
- `.wxt/`, `apps/desktop/dist/` and `src-tauri/gen/` are generated and gitignored;
  never edit them. Extension types require `wxt prepare` (the `typecheck` script does it).
- `.env.example` / `.env.local` document Supabase and logging values for later phases;
  no Rust code reads them yet (Vite only auto-loads `VITE_*`).

## Style

- Rust: edition 2024, resolver 3, 100 columns, four spaces. `unsafe` is forbidden,
  `unwrap` and `dbg_macro` are denied, `expect`/`todo` warn; tests are exempt via
  `clippy.toml`.
- TypeScript: strict plus `noUncheckedIndexedAccess` and `verbatimModuleSyntax`; ESLint
  requires `import type` (`consistent-type-imports`). Two spaces, `PascalCase`
  components, 100 columns. ESLint ignores `src-tauri/**` and generated directories.
- Conventional Commits (`fix(mcp): ...`); branches `feat/`, `fix/`, `chore/`, `docs/`,
  `refactor/`, `test/`. PRs must fill the security checklist and link the Kaneo task.

## Security invariants

- No password, key, TOTP code, or decrypted value in MCP responses, errors, logs,
  snapshots, or committed fixtures.
- Any secret held in Rust goes through `zeroize::Zeroizing<T>`.
- Automatic routing may only add `Dev Local` membership through `item_folders`; it must
  never remove an existing folder.
- Tokens, audit logs, and backup config stay local; never sync them.
- Local config goes in `.env.local` (gitignored), never in tracked files. Report
  vulnerabilities privately via GitHub Security Advisories.

## Task tracking

Kaneo account: **DEV MASTER** `heroldtamko39@gmail.com`, user id
`hGBISIpmch9D6vbjBnKBiX4Ml6oP4heV`.

- Workspace "Aplix": `GAiYlKA3Nr2yEh3Ea40IY0f0tvK4rjQ0`
- Project "Akooti" (slug `AKO`): `ql4kabfti5m3hlbofdlb5bek`
- Statuses are column slugs: `to-do`, `in-progress`, `in-review`, `done`.
- Subtasks use `create_task_relation` with `sourceTaskId` = parent and
  `targetTaskId` = child, `relationType: "subtask"`.

Tasks carry acceptance criteria and doc references; update their status/comment as part
of each change.
