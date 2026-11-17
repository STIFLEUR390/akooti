# AGENTS.md

Akooti is a local-first password vault. One Rust binary (`akooti`) ships the Tauri
desktop app, the local daemon that holds the master key, the headless MCP server and
(later) the browser bridge. Bun workspaces host the React UI, the WXT extension and
shared TS types.

## Layout and current state

Rust workspace (`crates/*`, `src-tauri`) + Bun workspace (`apps/*`, `packages/*`).

- `crates/akooti-core` — the product logic: crypto, SQLCipher, folders/items, generator,
  TOTP, routing, importers, session, MCP tokens and audit. **It must never import Tauri,
  WebView or React** (keeps it unit-testable and portable).
  - `importers/`: parsers normalize untrusted files into `ImportBundle`; only
    `importers/pipeline.rs` opens the SQLCipher transaction. `import_file` erases the
    source file only after commit and reports cleanup failure without rolling back.
  - `mcp/`: token issue/hash (SHA-256, constant-time compare), store, audit.
    `vault_mcp.rs` holds the scoped operations. Expired agent accounts are purged by
    `items/maintenance.rs`, driven by `VaultRuntime::run_maintenance` on the UI loop —
    not by a timer.
- `crates/akooti-ipc` — the local wire protocol: 4-byte LE length + JSON, 1 MiB cap,
  `Request`/`Response`, socket path (`AKOOTI_SOCKET`, else `$XDG_RUNTIME_DIR/akooti/daemon.sock`)
  and the blocking `DaemonClient`. Unix only; Windows named pipes deferred (ADR 0006).
- `crates/akooti-mcp` — `rmcp` stdio server exposing six tools. It depends only on
  `akooti-ipc`: no vault handle, no key, no `akooti-core`. Every tool forwards the token
  to the daemon and maps refusals to tool errors with stable codes. Zero-Leak tests are in-file.
- `src-tauri` — crate/package name is **`akooti`** (dir `src-tauri`, lib `akooti_lib`,
  bin `akooti`), so `cargo test -p akooti` is the desktop shell and needs Tauri system deps.
  `lib.rs` routes CLI modes before any window (`cli.rs`), `headless.rs` runs `mcp` /
  `native-host` / `native-register`, `native_host.rs` is the frame pump (stdin/stdout ↔
  daemon socket, no decoding) and `native_register.rs` the per-browser manifests,
  `vault_runtime.rs` owns the shared `Arc<VaultRuntime>` and 15-minute lock,
  `daemon/` is the IPC server (`inject.rs` = CDP fill, `exec.rs` = `vault_exec` masking),
  `commands/` is the typed IPC surface, `dto.rs`/`error.rs` the payloads and stable codes.
- `apps/desktop` — React/Vite UI. `lib/ipc.ts` is the typed command bridge with an in-memory
  mock used outside Tauri; `hooks/`, `components/`, `pages/` cover unlock, vault, generator,
  import, routing and the AI access page. `apps/extension` is WXT: service worker
  (`entrypoints/background.ts`, the only bridge client), popup (React), content script,
  `lib/dom` heuristics and a MAIN-world WebAuthn shim injected by an ISOLATED script.
  `packages/shared` exports raw `.ts` (no build step) as `@akooti/shared`, aliased in
  `apps/desktop/vite.config.ts` and `tsconfig.json`.
- `docs/` is the reference and is now accurate, with per-claim statuses: ✅ implemented,
  🧭 planned, ⚠️ known limit. `docs/README.md` is the index, `docs/regles.md` the normative
  rules, `docs/architecture.md` processes/flows/contracts, `docs/testing.md` coverage,
  `docs/desktop-tests.md` the manual recipe, `docs/adr/` the decisions. Still design-only:
  sync, backups, vault health, the Rust WebAuthn authenticator (the shim intercepts, then
  falls back to the browser), and Windows (named pipe + registry manifest).
  Verify against `src/` before assuming; the code wins.

## Commands

```bash
bun install                     # Bun workspaces; bun.lock is committed
bun run dev                     # desktop frontend only (Vite on :1420)
bun run tauri dev               # app + debug MCP bridge (spawns Vite itself)
bun run dev:extension           # WXT HMR
bun run build                   # build every package that has a build script
bun run lint                    # ESLint flat config, ignores src-tauri/**
bun run format:check            # Prettier; ignores docs/ and bun.lock
bun run typecheck               # wxt prepare (extension), then tsc per package
bun test                        # bun:test, run from the repo root
bun test packages/shared/src/models.test.ts          # one TS suite
cargo test -p akooti-core --locked                   # fast core feedback
cargo test -p akooti-core --test import_pipeline_test  # one integration suite
cargo test -p akooti-core <name>                     # one Rust test
cargo test -p akooti-core -p akooti-ipc -p akooti-mcp --locked   # no Tauri deps needed
cargo clippy -p akooti-core --all-targets --locked -- -D warnings
cargo fmt --all --check         # safe without Tauri system deps
```

- Workspace packages are scoped: `bun run --filter @akooti/desktop dev`. A bare
  `--filter desktop` matches nothing and **exits 0** — a silent no-op.
- CI runs `bun install --frozen-lockfile` and cargo with `--locked`: commit `bun.lock`
  and `Cargo.lock` whenever dependencies change.
- CI order — Rust: fmt → clippy → test → build over the whole workspace; JS: lint →
  format:check → typecheck → test → build; then `bun run tauri build --no-bundle`.
- `cargo ... --workspace` needs the Tauri system deps. Locally prefer `-p akooti-core`
  (and `-p akooti-ipc -p akooti-mcp`) for fast loops; CI is the full compile gate.
- There is no per-app test script: TS tests only run through `bun test` at the root.
  Rust coverage lives in `crates/akooti-core/tests/`; share helpers via
  `tests/common/mod.rs`, use `tempfile` vaults, keep fixtures anonymized.
- `apps/desktop`'s `build` runs `tsc --noEmit` first, so `bun run build` typechecks it.

## Things that bite

- **stdout is the wire in `mcp` mode.** JSON-RPC owns stdout; every log goes to stderr
  (`tracing` is wired to stderr, and only `akooti version` prints to stdout). A
  `println!` in the headless path corrupts the protocol.
- Exit codes of headless modes: `2` = `mcp` without a token, `3` = `native-register` on a
  platform without manifests (Windows), `1` = startup/wire failure (including
  `native-host` with no daemon, which still writes a `daemon_unavailable` frame).
- **Schema:** `db.rs` embeds `migrations/001_init.sql` via `include_str!`, applies it once
  and stamps `PRAGMA user_version` (`CURRENT_SCHEMA_VERSION = 1`; a newer version is
  treated as corruption). Never edit that shipped migration — add a new file and bump the
  constant.
- The real vault lives in `app_data_dir` (`~/.local/share/com.akooti.desktop` on Linux:
  `vault.meta` + `vault.db`). Never point builds, tests or scripts at it; tests use
  `tempfile` directories.
- `tauri-plugin-mcp-bridge` is registered only under `#[cfg(debug_assertions)]`, binds
  `127.0.0.1:9223`, and needs `withGlobalTauri: true` plus the `mcp-bridge:default`
  capability. After `bun run tauri dev`, connect with `driver_session`; DOM/screenshot/IPC
  tools target window `main`. `curl` on that port returns `000` even when healthy.
- Vite is pinned to `:1420` with `strictPort`; a stale dev server makes `tauri dev` fail.
  Don't start Vite separately — `beforeDevCommand` does it. `frontendDist` is relative to
  `src-tauri/` (`../apps/desktop/dist`).
- `.wxt/`, `apps/desktop/dist/`, `src-tauri/gen/` are generated: never edit. Extension
  types need `wxt prepare` (the `typecheck` script runs it).
- `.env.example` documents `AKOOTI_LOG`, `AKOOTI_SUPABASE_*`, `VITE_AKOOTI_ENV`. Only Vite
  auto-loads `VITE_*`; no Rust code reads the Supabase values yet.

## Local test vault and project MCP (gitignored secrets)

`TEST-VAULT.local.txt` and `opencode.json` are **gitignored** and hold a throwaway vault
password, MCP tokens and the recipe to rebuild them. Never commit them, never paste their
values into tracked files, docs, tickets or PRs — `*.local.txt` and `opencode.json` are in
`.gitignore` for that reason. Cleanup: `TEST-VAULT.local.txt` §5.

- `opencode.json` declares a project-local MCP server running
  `target/debug/akooti mcp --token …`: it needs a built binary **and** the desktop app
  running and unlocked. App closed → `daemon_unavailable`; locked → `locked`; expired or
  revoked token → `invalid_credentials` (reissue in **Accès IA** and realign both files).
- Rebuild (`cargo build -p akooti`) after Rust changes; the MCP server is a thin client of
  the running daemon, so the debug binary and the app must come from the same build.
- Recette and expected outputs: `docs/desktop-tests.md`.

## Style

- Rust: edition 2024, resolver 3, 100 columns, four spaces. `unsafe` forbidden, `unwrap`
  and `dbg_macro` denied, `expect`/`todo` warn — tests are exempt via `clippy.toml`.
- TypeScript: strict + `noUncheckedIndexedAccess` + `verbatimModuleSyntax`; ESLint enforces
  `import type`. Two spaces, `PascalCase` components, 100 columns.
- Conventional Commits in English (`fix(mcp): …`); branches `feat/`, `fix/`, `chore/`,
  `docs/`, `refactor/`, `test/`. PR template asks for the security checklist and the Kaneo
  task link. Docs and README are French; code, comments, commits are English.

## Security invariants (see `docs/regles.md`)

- No password, key, TOTP seed/code or decrypted value in MCP responses, errors, logs,
  audit rows, screenshots or committed fixtures.
- Any secret held in Rust goes through `zeroize::Zeroizing<T>`.
- A fill requires an explicit, exactly matching `origin`; any page whose URL differs is
  refused before loading a script, and the refusal is audited.
- `vault_exec` exports only `AKOOTI_USERNAME`, `AKOOTI_PASSWORD` and `AKOOTI_<FIELD>` per
  custom field — no TOTP, no master password — and masks those values in stdout/stderr.
- Routing may only **add** `Dev Local` membership via `item_folders`, never remove a folder.
- MCP tokens, audit logs and backup config stay local: never synced or exported.
- Local secrets live in `.env.local` or the gitignored files above, never in tracked files.
  Report vulnerabilities privately via GitHub Security Advisories.

## Task tracking

Kaneo account: **DEV MASTER** `heroldtamko39@gmail.com`, user id
`hGBISIpmch9D6vbjBnKBiX4Ml6oP4heV`.

- Workspace "Aplix": `GAiYlKA3Nr2yEh3Ea40IY0f0tvK4rjQ0`
- Project "Akooti" (slug `AKO`): `ql4kabfti5m3hlbofdlb5bek`
- Statuses are column slugs: `to-do`, `in-progress`, `in-review`, `done`.
- Subtasks use `create_task_relation` with `sourceTaskId` = parent,
  `targetTaskId` = child, `relationType: "subtask"`.

Phases are tracked as tasks: 0 foundations, 1 crypto/storage, 2 vault features,
3 importers, 4 desktop UI, 5 AI bridge — all `done`; 6 extension + passkeys is
`in-review` except the Rust WebAuthn authenticator, still `to-do`.
Update the task status/comment as part of each change.
