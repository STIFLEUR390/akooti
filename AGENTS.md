# AGENTS.md

Akooti is a local-first password vault. One Rust binary (`akooti`) ships the Tauri
desktop app, the local daemon that holds the master key, the headless MCP server and
the browser bridge (Native Messaging). Bun workspaces host the React UI, the WXT
extension and shared TS types.

## Layout and current state

Rust workspace (`crates/*`, `src-tauri`) + Bun workspace (`apps/*`, `packages/*`).

- `crates/akooti-core` — the product logic: crypto, SQLCipher, folders/items, generator,
  TOTP, routing, importers, session, MCP tokens and audit. **It must never import Tauri,
  WebView or React** (keeps it unit-testable and portable).
  - `importers/`: parsers normalize untrusted files into `ImportBundle`; only
    `importers/pipeline.rs` opens the SQLCipher transaction; the source is erased after commit.
  - `mcp/`: token issue/hash (SHA-256, constant-time), store, audit. `vault_mcp.rs` holds the
    scoped operations; expired agent accounts are purged by `VaultRuntime::run_maintenance`
    on the UI loop, not by a timer.
  - `health.rs` (zxcvbn, reuse, age; HIBP split into prefix here / range in the shell),
    `sync.rs` (E2EE codec, LWW + tombstones), `snapshot.rs` (`.akooti`, restore, plaintext
    export), `backup.rs` (encrypted targets, retention). **No network code here** (SEC-5).
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
  `commands/health.rs` is the HIBP egress: `range/{prefix}`, explicit gesture, in-memory
  cache, `health:progress` events. `sync/` holds the Supabase REST + Realtime clients,
  `backup/` the local/S3 SigV4/Drive drivers and the hourly scheduler; `commands/sync.rs`,
  `commands/backup.rs`, `commands/export.rs` and `commands/updates.rs` are the IPC surface.
  `tauri.conf.json` holds the strict CSP, the bundle metadata and the updater public key;
  the private key stays outside the repo. Drive and the bridge have debug-only test
  overrides — never compiled in release.
- `apps/desktop` — React/Vite UI; `lib/ipc.ts` is the typed bridge with a mock used outside
  Tauri; `pages/` = unlock, vault, import, routing, AI access, health, sync/backups; the
  first-run guide is keyed by `akooti.onboarding.seen`. `apps/extension` is WXT (service
  worker = only bridge client, popup, content scripts, MAIN-world WebAuthn shim).
  `packages/shared` exports raw `.ts` (no build) as `@akooti/shared`, aliased in desktop
  `vite.config.ts` / `tsconfig.json`.
- `supabase/setup.sql` is what the **user** runs in their own Supabase project (tables,
  RLS, Realtime) — not a SQLCipher migration. `scripts/e2e/app.e2e.mjs` drives the real
  app through the debug bridge.
- `docs/` is the reference, with per-claim statuses (✅ implemented / 🧭 planned / ⚠️ limit):
  `README.md` index, `regles.md` normative rules, `architecture.md` flows, `schema.md`,
  `testing.md`, `desktop-tests.md` recipes, `guides/` user guides, `threat-model.md` +
  `security-review.md`, `release.md`, `adr/`. Design-only: Windows (named pipe + registry),
  macOS notarization / Windows signing, real Google Drive recipe (validated on the gfeh API
  v3 emulator). Version 1.0.0; tags `vX.Y.Z`. Verify against `src/` — the code wins.

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
- E2E: launch the app on a throwaway profile, then run the bridge runner. It creates the
  vault and covers create/lock/item/health/backup/token; failures write
  `artifacts/e2e/` (gitignored).

  ```bash
  XDG_DATA_HOME=/tmp/akooti-e2e bun run tauri dev &
  bun scripts/e2e/app.e2e.mjs
  ```

- Bundles: `bun run tauri build --bundles deb` (or `appimage`/`all`); updater artifacts need
  the signing key. Full matrix and signing in `docs/release.md` (Fedora needs a pkg-config
  shim for appindicator — documented there).

  ```bash
  TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/akooti.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    bun run tauri build --bundles deb
  ```

- CI: `ci.yml` on PR, `e2e.yml` nightly (Xvfb), `release.yml` on tag `v*`.

## Things that bite

- **stdout is the wire in `mcp` mode.** JSON-RPC owns stdout; every log goes to stderr
  (`tracing` is wired to stderr, and only `akooti version` prints to stdout). A
  `println!` in the headless path corrupts the protocol.
- Exit codes of headless modes: `2` = `mcp` without a token, `3` = `native-register` on a
  platform without manifests (Windows), `1` = startup/wire failure (including
  `native-host` with no daemon, which still writes a `daemon_unavailable` frame).
- **Schema:** `db.rs` embeds `migrations/001_init.sql` and `002_sync_backup.sql` via
  `include_str!` and stamps `PRAGMA user_version` (`CURRENT_SCHEMA_VERSION = 2`; a newer
  version is corruption). Never edit a shipped migration — add `00N_*.sql` and bump the
  constant.
- The real vault lives in `app_data_dir` (`~/.local/share/com.akooti.desktop` on Linux:
  `vault.meta` + `vault.db`). Never point builds, tests or scripts at it; tests use
  `tempfile` directories.
- `tauri-plugin-mcp-bridge` is registered only under `#[cfg(debug_assertions)]`, binds
  `127.0.0.1:9223`, and needs `withGlobalTauri: true` plus the `mcp-bridge:default`
  capability. Its WS protocol is `{id, command, args}` — `scripts/e2e/app.e2e.mjs` speaks
  it. `invoke_tauri` only proxies a whitelist: call app commands with
  `window.__TAURI__.core.invoke(…)` inside `execute_js` (promises are awaited). `curl` on
  that port returns `000` even when healthy.
- Vite is pinned to `:1420` with `strictPort`; a stale dev server makes `tauri dev` fail.
  Don't start Vite separately — `beforeDevCommand` does it. `frontendDist` is relative to
  `src-tauri/` (`../apps/desktop/dist`).
- `.wxt/`, `apps/desktop/dist/`, `src-tauri/gen/` are generated: never edit. Extension
  types need `wxt prepare` (the `typecheck` script runs it).
- `.env.example`: only Vite auto-loads `VITE_*`. `AKOOTI_SUPABASE_*` are **not read** (sync
  is configured in the UI); `AKOOTI_GDRIVE_API_BASE` / `AKOOTI_GDRIVE_STATIC_TOKEN` are
  debug-only overrides for the Drive emulator, compiled out of release.
- Supabase, S3 and Drive tests need real services (or the external `gfeh-drive` emulator);
  their suites are offline. `supabase/setup.sql` is applied once per project — until then
  `sync_now` answers `sync_schema`.
- The CSP is strict on purpose: adding a remote script/style host means editing
  `app.security.csp` (and is a security-review trigger, not a config tweak).

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
- `TEST-VAULT.local.txt` §2.1 (Supabase test project, backup targets) and §3.5 (recipes
  actually run) are the local source of truth; general recipes in `docs/desktop-tests.md`.

## Style

- Rust: edition 2024, resolver 3, 100 columns, four spaces. `unsafe` forbidden, `unwrap`
  and `dbg_macro` denied, `expect`/`todo` warn — tests are exempt via `clippy.toml`.
- TypeScript: strict + `noUncheckedIndexedAccess` + `verbatimModuleSyntax`; ESLint enforces
  `import type`. Two spaces, `PascalCase` components, 100 columns.
- Conventional Commits in English (`fix(mcp): …`); branches `feat/`, `fix/`, `chore/`,
  `docs/`, `refactor/`, `test/`. Process and security checklist: `CONTRIBUTING.md`.
  Docs and README are French; code, comments, commits are English.

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
- Sync and backup credentials (Supabase tokens, S3 secret, Drive refresh token) are
  encrypted with the master key and never returned to the UI.
- The updater private key never enters the repo; changing `plugins.updater.pubkey` breaks
  updates for already-installed clients.
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

Phases 0–9 are `done` (foundations → extension → health → sync/backups → release 1.0.0).
Open follow-ups are GitHub issues #74–#78, mirrored in Kaneo To Do: macOS notarization /
Windows signing, real Google Drive recipe, OS session lock, two-device sync, updater
end-to-end. Update the task status/comment as part of each change.
