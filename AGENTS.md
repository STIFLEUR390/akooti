# Repository Guidelines

## Layout and boundaries

- `crates/akooti-core` — pure vault logic (crypto, SQLCipher, routing, imports).
  It must never import Tauri, WebView, or React; keep it unit-testable and
  portable.
- `crates/akooti-mcp` — MCP server and blind-injection engines.
- `src-tauri` — desktop shell only: Tauri builder, capabilities, CLI/daemon/IPC
  adapters. Business logic belongs in the crates.
- `apps/desktop` (React/Vite), `apps/extension` (WXT), `packages/shared`
  (domain types + wire protocols). Import shared code as `@akooti/shared`: it
  ships raw `.ts` via `exports`, so there is no build step. Vite aliases it in
  `apps/desktop/vite.config.ts`.
- Design specs live in `docs/*.md`; decisions in `docs/adr/`.

## Commands

```bash
bun install                     # Bun workspaces (bun.lock is committed)
bun run tauri dev               # desktop app + debug MCP bridge
bun run dev:extension           # WXT HMR
bun run build                   # desktop + extension
bun run lint                    # ESLint (flat config)
bun run format:check            # Prettier (ignores docs/ and bun.lock)
bun run typecheck               # runs `wxt prepare` first, then tsc everywhere
bun test                        # all bun:test suites
bun test packages/shared/src/models.test.ts   # one suite
cargo test -p akooti-core       # core only, no Tauri system deps needed
cargo test -p akooti-core <name>  # one Rust test
```

- Workspace package names are scoped: `bun run --filter @akooti/desktop dev`.
  A bare `--filter desktop` matches nothing and exits 0 — a silent no-op.
- CI runs `bun install --frozen-lockfile` and cargo with `--locked`: commit
  `bun.lock` and `Cargo.lock` whenever dependencies change.
- CI order per job: lint → format:check → typecheck → test → build.

## Tauri specifics

- Tauri system deps are listed in `CONTRIBUTING.md` (Fedora and Ubuntu).
  Without `webkit2gtk-4.1`/`gtk3` devel, `cargo clippy --workspace`,
  `cargo build --workspace`, and `bun run tauri dev` fail; use
  `cargo test -p akooti-core` for core work and treat CI as the compile gate.
- `bun run tauri build` triggers `beforeBuildCommand`, which builds
  `apps/desktop` first. `frontendDist` in `src-tauri/tauri.conf.json` is
  relative to `src-tauri/`, not the repo root (`../apps/desktop/dist`).
- The MCP bridge (`tauri-plugin-mcp-bridge`) is compiled in debug builds only,
  listens on `127.0.0.1:9223`, and needs `withGlobalTauri: true` plus the
  `mcp-bridge:default` capability. After `bun run tauri dev`, connect with
  `driver_session`; screenshots/DOM/IPC tools then target window `main`.
- `.wxt/` and `apps/desktop/dist/` are generated and gitignored; never edit
  them. Extension types require `wxt prepare` (the `typecheck` script does it).

## Style

- Rust: edition 2024, resolver 3, 100 columns, four spaces. `unsafe` is
  forbidden, `unwrap` is denied, `expect` warns; tests are exempt via
  `clippy.toml`.
- TypeScript: strict plus `noUncheckedIndexedAccess`; use `import type`, two
  spaces, `PascalCase` components.
- Conventional Commits (`fix(mcp): ...`). PRs must fill the security checklist
  and link the Kaneo task.

## Security invariants

- No password, key, TOTP code, or decrypted value in MCP responses, errors,
  logs, snapshots, or committed fixtures.
- Any secret held in Rust goes through `zeroize::Zeroizing<T>`.
- Tokens, audit logs, and backup config stay local; never sync them.
- Local config goes in `.env.local` (gitignored); start from `.env.example`.
  Report vulnerabilities privately via GitHub Security Advisories.

## Task tracking

Kaneo project "Akooti" — workspace `GAiYlKA3Nr2yEh3Ea40IY0f0tvK4rjQ0`, project
`ql4kabfti5m3hlbofdlb5bek`. Tasks carry acceptance criteria and doc references;
update their status/comment as part of each change.
