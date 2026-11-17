---
name: akooti
description: Use the Akooti vault MCP tools to search credentials, fill a login form in the user's browser, create a throwaway test account, or run a command with credentials in its environment — without ever seeing a secret. Use when a task needs the user's password, a local dev login, a disposable signup account, or an authenticated command.
---

# Akooti — use the vault without seeing a secret

Akooti is a local password vault. The `akooti mcp` server gives you six tools
over stdio. **No tool ever returns a password, a TOTP seed or a custom field
value.** Injection happens inside the browser or inside a child process; you
only get back which fields were filled or masked command output.

## Setup (the user does this, once)

1. In the Akooti desktop app → **Accès IA**, create a token: pick the folders it
   may read and the scopes it needs. The secret is shown **once**.
2. In your MCP client configuration:

```json
{
  "mcpServers": {
    "akooti": {
      "command": "akooti",
      "args": ["mcp", "--token", "ak_mcp_…"]
    }
  }
}
```

The secret is only a handle: it is scoped to folders and actions, it expires,
and the user can revoke it at any time. The Akooti daemon must be running (the
desktop app unlocked) — otherwise every call fails with `daemon_unavailable`.

## Tools

| Tool                    | Scope             | What it does                                                                                      |
| ----------------------- | ----------------- | ------------------------------------------------------------------------------------------------- |
| `vault_status`          | —                 | Name, scopes, allowed folders and expiry of the current token. Start here when a call is refused. |
| `vault_search`          | `read_metadata`   | Search titles, usernames and URLs. Returns ids and metadata only.                                 |
| `vault_cdp_autofill`    | `blind_fill`      | Fill a login form in a Chrome/Chromium page exposing DevTools, then report the fields filled.     |
| `vault_blind_autofill`  | `blind_fill`      | Same through the browser extension. Not available in this build — use `vault_cdp_autofill`.       |
| `vault_create_and_fill` | `create_and_fill` | Create a throwaway account with a generated password and a TTL, then fill its signup form.        |
| `vault_exec`            | `exec_env`        | Run a command with the item's credentials in `AKOOTI_*` environment variables; output is masked.  |

## Workflow that works

1. `vault_search` with a short query (`docker`, `staging`, `postgres`).
   An empty result means the folder is outside the token — do not guess ids.
2. For a browser login: start the browser with a DevTools endpoint
   (`google-chrome --remote-debugging-port=9222`), open the login page, then
   call `vault_cdp_autofill` with that `origin` and the item id.
   Pass `selectors` only when the page has no stored selectors.
3. For a CLI that needs credentials: `vault_exec` with the item id and an argv
   array. `argv[0]` is the program and no shell is involved, so a variable is
   only expanded if you ask for a shell yourself:
   `["bash", "-c", "docker login -u \"$AKOOTI_USERNAME\" -p \"$AKOOTI_PASSWORD\""]`.
   The output you get back has every injected value replaced by `***`.
   The environment carries `AKOOTI_USERNAME`, `AKOOTI_PASSWORD` and one
   `AKOOTI_<FIELD_NAME>` per custom field. There is no `AKOOTI_TOTP`: generate
   the code with the fill tools or ask the user, never by reading the vault.
4. For a disposable signup: `vault_create_and_fill` with a title, the URL and,
   like `vault_cdp_autofill`, a `selectors` object (`username`, `password`,
   `totp`, `submit`). A throwaway item has no stored selectors, so without them
   the fill comes back `partial` and nothing is filled. The account is deleted
   automatically when its TTL expires.

### Fill results

Both fill tools answer `{status, fieldsFilled, submitted, detail}`:

- `filled` — at least one field was injected; `fieldsFilled` names them.
- `partial` — no selector matched. Treat it as a failure: fix the selectors and
  call again.
- `failed` — the page, the endpoint or the origin was refused; `detail` carries
  a stable code such as `no page target matches the requested origin`.
- `unavailable` — the native messaging bridge is missing (extension path only).

(A filled TOTP code is reported as `totp` in `fieldsFilled`; the code itself is
never returned.)

## Rules you must follow

- **Never ask the user to paste a password into the chat.** If a step needs a
  value, the right answer is one of the fill tools or `vault_exec`.
- Do not try to read a secret out of an error, a log or a masked output: the
  masking is deliberate, and `***` means the value was injected.
- A `forbidden` error means the token lacks the scope or the folder. Report it
  and ask the user to widen the token in **Accès IA** — do not retry in a loop.
- Stay on the origin the user named. Akooti refuses to fill a page whose URL
  does not match the requested `origin`; that refusal is a feature.
- Prefer `vault_create_and_fill` over typing made-up credentials into a signup
  form: the generated password is stored, so the account stays usable later.

## Failure codes

| Code                  | Meaning                                                         |
| --------------------- | --------------------------------------------------------------- |
| `daemon_unavailable`  | The desktop app is closed. Ask the user to start and unlock it. |
| `locked`              | The vault is locked; it re-locks after 15 minutes idle.         |
| `forbidden`           | Token scope or folder does not cover this item/action.          |
| `not_found`           | Unknown item id, or the id belongs to another vault.            |
| `invalid_credentials` | The token was revoked, expired, or never existed.               |
| `invalid_input`       | Missing field, out-of-range TTL, or malformed command.          |

`vault_blind_autofill` is the one exception: it answers a normal result whose
`detail` says the extension bridge is unavailable, so check `status` rather
than trusting the call's success.
