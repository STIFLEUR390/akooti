# Security policy

Akooti holds secrets: a vulnerability can be a data loss or a credential leak.
Please report privately, never in a public issue.

## Reporting a vulnerability

Use **GitHub Security Advisories** on
<https://github.com/STIFLEUR390/akooti/security/advisories/new> (private by
default). If that is unavailable, open a minimal public issue asking for a
private channel — without details, payloads or secrets.

Include:

- version (`akooti version`) and platform;
- the command or screen involved, and the steps to reproduce;
- what an attacker gains (read a secret, bypass a scope, exfiltrate data);
- a proof of concept **with fake secrets only**.

Never send a real master password, a real MCP token or a real vault file. If you
think a real secret leaked, rotate it first (change the password, revoke the
token) and say so.

## What is in scope

- `akooti-core` (crypto, SQLCipher usage, sync codec, snapshots, importers)
- the desktop shell and its IPC command surface
- the local daemon socket and the MCP server (`akooti-mcp`, `akooti-ipc`)
- the browser extension (content scripts, WebAuthn shim, native bridge)
- the Supabase schema and RLS policies (`supabase/setup.sql`)
- the release pipeline (signing, update manifest)

Out of scope: a compromised operating system or user session, a browser
extension store compromise, and physical access to an unlocked machine — see the
residual risks in [`docs/threat-model.md`](./docs/threat-model.md).

## Supported versions

| Version | Supported |
| ------- | --------- |
| 1.x     | yes       |
| < 1.0   | no        |

## Our commitments

- Acknowledgement within 72 hours, a first assessment within 7 days.
- A coordinated disclosure date once a fix is ready (usually 90 days).
- Credit in the release notes unless you ask otherwise.
- No legal action for good-faith research that stays within your own vault and
  does not attack third parties.

## Hardening you can do today

- Keep the master password out of the vault's machine (password manager, paper).
- Use scoped, expiring MCP tokens; revoke unused ones.
- Keep the app updated; verify the signature of downloaded bundles.
- Back up with `.akooti` snapshots to a target you control, and test a restore.
