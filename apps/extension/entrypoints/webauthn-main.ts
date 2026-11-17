import { defineUnlistedScript } from "wxt/utils/define-unlisted-script";
import {
  WebauthnRelay,
  pageMessageTarget,
  type CeremonyKind,
  type CeremonyOptions,
} from "../lib/webauthn/bridge";
import { credentialFromJson } from "../lib/webauthn/credential";
import { creationOptions, requestOptions } from "../lib/webauthn/options";

/**
 * MAIN-world WebAuthn shim.
 *
 * Runs inside the page, so it has no extension API: it posts a nonce-tagged
 * message, `webauthn.content.ts` relays it to the daemon, and the answer comes
 * back through the same channel. Every path falls back to the original browser
 * function — a site that worked before installing Akooti keeps working, and a
 * user who never stores a passkey sees no difference.
 *
 * The ceremony itself is served by the vault (`crates/akooti-core/src/
 * passkeys.rs`). This file only installs the interception, reads the options
 * (`webauthn/options.ts`) and rebuilds the answer (`webauthn/credential.ts`): it
 * holds no key, sees no secret, and decides nothing a relying party would trust.
 */

declare global {
  interface Window {
    /** Set once the shim is in place, so a second injection is a no-op. */
    __akootiPasskeyShim?: true;
  }
}

export default defineUnlistedScript(() => {
  install();
});

function install(): void {
  const nonce = nonceOfCurrentScript();
  if (nonce === null || window.__akootiPasskeyShim === true) {
    return;
  }
  window.__akootiPasskeyShim = true;
  const container = navigator.credentials;
  const relay = new WebauthnRelay(
    (message) => window.postMessage(message, pageMessageTarget(location.origin)),
    nonce,
  );
  window.addEventListener("message", (event) => {
    if (event.source !== window || event.origin !== location.origin) {
      return;
    }
    relay.receive(event.data);
  });

  const originalCreate = container.create?.bind(container);
  const originalGet = container.get?.bind(container);

  container.create = async (options?: CredentialCreationOptions) => {
    // `null` means "not a ceremony we can relay": the browser gets the call and
    // raises its own error, which is the one the page knows how to read.
    const ceremony = creationOptions(options);
    const credential = ceremony === null ? null : await intercept(relay, "create", ceremony);
    if (credential !== null) {
      return credential;
    }
    if (!originalCreate) {
      throw new DOMException("WebAuthn is not available", "NotSupportedError");
    }
    return originalCreate(options);
  };

  container.get = async (options?: CredentialRequestOptions) => {
    const ceremony = requestOptions(options);
    const credential = ceremony === null ? null : await intercept(relay, "get", ceremony);
    if (credential !== null) {
      return credential;
    }
    if (!originalGet) {
      throw new DOMException("WebAuthn is not available", "NotSupportedError");
    }
    return originalGet(options);
  };
}

/**
 * Asks the daemon for one credential, and never throws.
 *
 * A timeout, a closed desktop app, a refusal or a malformed payload all return
 * `null`, which the caller turns into a call to the original function.
 */
async function intercept(
  relay: WebauthnRelay,
  kind: CeremonyKind,
  options: CeremonyOptions,
): Promise<Credential | null> {
  try {
    const reply = await relay.send(kind, options);
    if (!reply.handled || reply.credential === null) {
      return null;
    }
    return credentialFromJson(kind, reply.credential) as unknown as Credential;
  } catch {
    return null;
  }
}

/** Nonce handed over by the injecting content script, via `data-akooti-nonce`. */
function nonceOfCurrentScript(): string | null {
  const script = document.currentScript;
  if (!(script instanceof HTMLScriptElement)) {
    return null;
  }
  const nonce = script.dataset.akootiNonce;
  return typeof nonce === "string" && nonce.length > 0 ? nonce : null;
}
