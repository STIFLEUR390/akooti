# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The same developer as the desktop app, with equal weight in two roles:

- **Human at the browser**: searches an entry, fills a login, copies a TOTP code or a password for a site that ignores autofill, captures DOM selectors for an entry. This is a daily surface, not a fallback.
- **Agent at the same browser**: never sees this UI, but the paired token, the `blind_fill` scope and the exact-origin check are what let it drive a page without ever reading a secret.

Non-technical users are not a target. Someone who has never issued an MCP token must still be able to pair and fill without reading the docs.

## Product Purpose

The extension is the bridge between the vault and the browser: it carries a fill or a copy request to the daemon, which holds the master key, and hands the values to the active tab's content script. Nothing of the vault lives in the browser beyond a paired token and a masked label.

Success means a login happens on the page — by hand or by an agent — and the popup never had to show the secret.

## Positioning

The only password-manager popup whose job is to _not_ display secrets: fill is blind, copy is ephemeral (cleared after 30 s), the popup sees a report (`fieldsFilled`, `submitted`) or a field name and nothing else. The browser side is deliberately thin — no HTTP, no port, no local copy of the vault: Native Messaging is the only route to the daemon, and the native host relays frames without decoding them.

## Operating Context

Pairing: create a token in the desktop app's **Accès IA** with `read_metadata` and `blind_fill`, paste it in the popup; the token field is cleared before the status refresh. The footer shows the masked token and the label, with **Dissocier**.

Daily loop: open the popup on the page, read the header pill (not paired / app closed / vault locked / vault unlocked), search on submit (each query is audited, so never per keystroke), then **Remplir** or copy. Selector capture prints text to paste into the vault entry — the bridge never writes it.

A WebAuthn shim in the page's MAIN world lets the vault serve passkeys; the daemon must be running and unlocked. Chrome/Edge/Brave (MV3, Chrome ≥ 116) and Firefox (MV2, ≥ 115) are shipped. UI copy is French; code, comments and commits are English.

## Capabilities and Constraints

Implemented and tested: pairing/unpairing with token shape validation; daemon and vault status reporting; audited search; blind fill with automatic submit; clipboard copy of username, password and TOTP with 30 s wipe; DOM form detection (login, signup, TOTP); selector capture; passkey registration and assertion through the vault; per-failure French titles for every runtime error code.

Constraints: no secret is ever rendered in the popup; the popup owns no vault data and holds no key; permissions stay minimal (`nativeMessaging`, `storage`, `scripting`, `activeTab`, `clipboardWrite`); no remote code, no network call, no HTTP endpoint; a fill requires an exactly matching origin; the token lives in this browser and only reaches the local daemon.

Known limits: agent-driven fill through the extension answers `unavailable` (use CDP autofill instead); passkeys claim no attestation (`fmt: "none"`), no user verification (UP flag only) and no discoverable credentials, so a ceremony requiring `userVerification: "required"` or `residentKey: "required"` falls back to the browser; Safari is not shipped.

Explicitly undecided: UI copy is French today and i18n is planned (inherited from the desktop rule) — strings must stay extractable.

## Brand Commitments

Name Akooti, MIT licence, icon from the extension bundle. Same token palette as the desktop app, reduced to what a 360 px panel needs; dark is the only theme because a popup inherits no page styling. Voice: sober technical French, plain sentences; an error says what failed and what to do next.

User-volunteered binding constraints: the popup is a first-class human surface as much as an agent bridge, so neither role may be demoted to make the other simpler.

## Evidence on Hand

Real: the working extension with per-module test suites (`lib/dom`, `lib/native`, `lib/webauthn`, `lib/runtime`, `lib/protocol`) and the architecture document describing the frame flow end to end. The native host is registered per browser by `akooti native-register`.

Absent and not to be fabricated: store listings, ratings, user counts, press, benchmarks.

## Product Principles

1. The popup never shows a secret: it fills, or it copies something that erases itself.
2. Blind fill is the default path; clipboard copy is an explicit, first-class choice with a visible lifetime.
3. Human and agent are equal users of the same bridge — a simplification that serves one must not break the other.
4. The browser side stays thin: no vault copy, no network, one route to the daemon.
5. Be honest about limits: a passkey that cannot satisfy the site is handed back to the browser, and the error names the real cause.

## Accessibility & Inclusion

Keyboard operability and a sensible focus order in a 360 px panel; status is announced through `aria-live` regions, and every state is also carried by text, never by colour alone. Contrast comes from the shared token system (`--accent-contrast` documented as ≥ 4.5:1). No formal standard beyond that has been established.
