# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Solo developers and small technical teams who work with coding agents (Claude Code, Codex, Cursor, Antigravity) and autonomous browsers, running Akooti on their own machine. They know what a secret pasted in a prompt or an `.env` in clear costs, and they are comfortable with MCP tokens, folder scopes and CDP.

Second audience: the agents themselves. They never see this UI, but every screen that issues a token, narrows a scope or writes a routing rule exists to make agent access safe. Non-technical users are not a target.

## Product Purpose

A local-first password manager: one Rust binary (Tauri v2 desktop app, daemon, headless MCP server, browser bridge) that holds the master key and sends secrets to their target — extension fill, CDP fill, child-process environment — without a secret ever coming back to a model, a log or a screenshot.

Success means an agent finishes real work (sign in to a local back-office, run a test stack) and no secret has left the machine or reached the model.

## Positioning

Zero-leak by architecture, not by policy: MCP tools return metadata only, fills are blind and origin-checked, `vault_exec` masks injected values in stdout/stderr, and the desktop UI is the only surface that ever displays a secret, behind an explicit reveal.

Local-first and zero-knowledge with bring-your-own sync and storage: the product is fully usable with no vendor backend, and Supabase, S3 or Drive only ever see client-encrypted blobs.

## Operating Context

Local development loop: unlock the vault (15-minute auto-lock), keep the desktop app running because the daemon holds the key, register the browser extension, issue a scoped MCP token in **Accès IA**, point the agent's config at `akooti mcp --token …`.

Vault items are folders (N:N) plus URLs, custom fields, TOTP seeds and remembered DOM selectors. Maintenance surfaces — health, sync, backups, routing, AI access — are visited occasionally, not daily. UI copy is French; code, comments and commits are English.

## Capabilities and Constraints

Implemented and tested: multi-folder vault; protected system folders (`Personnel`, `Dev Local`, `Agents & MCP`); TOTP display and blind fill; passkeys served by the vault (no attestation, no user verification, no discoverable credentials); 10 import formats with post-import hygiene; `Dev Local` routing that only adds membership; offline health audit (zxcvbn, reuse, age) with opt-in HIBP k-anonymity; E2EE Supabase sync (LWW + tombstones); scheduled snapshots to local/NAS/S3/Google Drive; plaintext emergency export behind master-password confirmation.

Constraints: secrets, MCP tokens, audit logs and backup config stay local and are never synced or exported; secrets in Rust go through `Zeroizing`; a fill requires an exactly matching origin; the CSP is strict, so adding a remote host is a security decision rather than a config tweak.

Known limits: biometrics has no provider wired and the control is hidden; agent-driven fill through the extension answers `unavailable` (use CDP); the real Google Drive recipe is unverified.

Explicitly undecided: UI copy is French today and i18n is planned — user-facing strings must stay extractable, not welded into un-translatable prose.

## Brand Commitments

Name Akooti, MIT licence, icon at `assets/icon.png`. Voice: sober technical French, plain sentences, no marketing superlatives; an error says what failed and what to do next.

User-volunteered binding constraints: dark theme by default (light available), dense but breathing layout, fully keyboard-operable, and a visible hierarchy between protected system folders, masked secrets and AI access controls.

## Evidence on Hand

Real: the working product and its test suites; its own documentation in `docs/` with per-feature statuses (✅ implemented / 🧭 planned / ⚠️ limit); the security model and threat model.

Absent and not to be fabricated: testimonials, customer names, press, benchmarks, pricing, usage numbers.

## Product Principles

1. A secret only travels from the vault to its target — never back to a reader, a log or a model.
2. Local-first wins ties: anything requiring a vendor backend stays optional.
3. Security decisions are shown, not hidden: refusal, masking and scope are visible and auditable.
4. The UI serves a developer at a keyboard: dense, scannable, shortcut-driven, no ceremony.
5. Irreversible actions (source erasure, plaintext export, any removal) need explicit confirmation and must preserve what the user already had.

## Accessibility & Inclusion

Keyboard operability is a product requirement: shortcuts, visible focus, no pointer-only flow. Contrast is enforced through the token system (`--accent-contrast` is documented as ≥ 4.5:1). No formal standard beyond that (WCAG level, screen-reader certification) has been established.
