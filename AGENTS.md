# Repository Guidelines

## Project Structure & Module Organization

Akooti is a Rust and Bun monorepo. `crates/akooti-core` contains pure business
logic and must not depend on Tauri, WebView, or React. `crates/akooti-mcp`
contains MCP and blind-injection code. `src-tauri` holds the desktop shell,
CLI/daemon, and IPC adapters. The React/Vite UI lives in `apps/desktop`, the WXT
extension in `apps/extension`, and shared TypeScript contracts in
`packages/shared`. Put architecture decisions in `docs/adr/` and repository
documentation in `docs/`.

## Build, Test, and Development Commands

Use Rust 1.90+, Bun 1.4+, and the Tauri system dependencies listed in
`CONTRIBUTING.md`.

```bash
bun install                    # Install workspace dependencies
bun run tauri dev              # Run the desktop app
bun run dev:extension          # Run the extension with HMR
bun run build                  # Build desktop and extension packages
bun run lint                   # Run ESLint
bun run format:check           # Check Prettier formatting
bun run typecheck              # Type-check every TypeScript package
bun test                       # Run bun:test suites
cargo fmt --all --check        # Check Rust formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace         # Run all Rust tests
```

## Coding Style & Naming Conventions

Rust uses edition 2024, four-space indentation, `rustfmt` with a 100-column
width, and Clippy warnings as errors. Avoid `unsafe`, `unwrap`, and `expect`
outside tests. Use `snake_case` for Rust functions and modules and
`PascalCase` for types. TypeScript uses strict checking, two-space indentation,
Prettier, ESLint, and explicit `import type`; use `camelCase` for values and
`PascalCase` for types and React components.

## Testing Guidelines

Place TypeScript tests beside their source as `*.test.ts`; Rust tests belong in
the owning crate. Add coverage for core business logic and a non-regression test
for every security fix. No numeric coverage threshold is configured.

## Commit & Pull Request Guidelines

Use Conventional Commits, such as `fix(mcp): reject leaked secrets`, and branch
names such as `feat/...`, `fix/...`, or `docs/...`. Rebase on `main` and keep
each commit coherent. Pull requests must describe the change, link the issue or
Kaneo task, complete the security checklist, and list the commands run. Include
UI evidence when behavior changes.

## Security & Configuration

Never commit passwords, keys, TOTP codes, decrypted vault data, or screenshots
containing secrets. Local configuration belongs in `.env.local`; start from
`.env.example`. Rust code handling secrets must use `zeroize::Zeroizing<T>`,
and MCP responses must never expose decrypted values. Follow `docs/regles.md`
and report vulnerabilities privately through GitHub Security Advisories.
