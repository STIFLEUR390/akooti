import { browser } from "wxt/browser";
import { defineContentScript } from "wxt/utils/define-content-script";
import { injectScript } from "wxt/utils/inject-script";
import { callRuntime, type RuntimeSend } from "../lib/runtime";
import {
  WEBAUTHN_CHANNEL,
  isRelyingPartyAllowed,
  isWebauthnRequest,
  pageMessageTarget,
  type WebauthnReply,
  type WebauthnRequest,
} from "../lib/webauthn/bridge";

/**
 * ISOLATED-world relay for WebAuthn ceremonies.
 *
 * The shim runs in the page's own world (it must replace `navigator.credentials`
 * before the site reads it) and therefore cannot call the extension API. This
 * script injects it, picks the per-page nonce that authenticates its messages,
 * and forwards each ceremony to the service worker, which is the only process
 * allowed to talk to the daemon.
 *
 * Registration is limited to secure contexts: a `http:` page has no WebAuthn at
 * all, so there is nothing to intercept there.
 */

/** Unlisted script produced by `entrypoints/webauthn-main.ts`. */
const SHIM_PATH = "/webauthn-main.js";

export default defineContentScript({
  matches: ["https://*/*", "http://localhost/*", "http://127.0.0.1/*"],
  runAt: "document_start",
  main() {
    const nonce = randomNonce();
    window.addEventListener("message", (event) => {
      const request = event.data as unknown;
      if (
        event.source !== window ||
        event.origin !== location.origin ||
        !isWebauthnRequest(request) ||
        // A page can post anything it likes: only our own tag is answered, so it
        // cannot fabricate a request the extension would treat as trusted.
        request.nonce !== nonce
      ) {
        return;
      }
      void relay(request, nonce);
    });
    void injectShim(nonce);
  },
});

/** Forwards one ceremony to the worker and posts its decision back. */
async function relay(request: WebauthnRequest, nonce: string): Promise<void> {
  const reply: WebauthnReply = {
    channel: WEBAUTHN_CHANNEL,
    nonce,
    id: request.id,
    handled: false,
    reason: null,
    credential: null,
  };
  if (!isRelyingPartyAllowed(request.rpId, location.hostname)) {
    reply.reason = "the requested relying party does not belong to this origin";
    post(reply);
    return;
  }
  const answer = await callRuntime(sendToWorker, {
    type: "akooti:passkey",
    kind: request.kind,
    rpId: request.rpId,
  });
  if (answer.ok && answer.kind === "passkey") {
    reply.handled = answer.handled;
    reply.reason = answer.reason;
    reply.credential = answer.credential;
  } else {
    reply.reason = answer.ok ? "the extension answered an unexpected message" : answer.message;
  }
  post(reply);
}

const sendToWorker: RuntimeSend = (message) => browser.runtime.sendMessage(message);

function post(reply: WebauthnReply): void {
  window.postMessage(reply, pageMessageTarget(location.origin));
}

/**
 * Injects the shim with its nonce, or gives up silently.
 *
 * A page that blocks the script (strict CSP on some sites) simply keeps the
 * browser authenticator: no passkey is intercepted, nothing else breaks.
 */
async function injectShim(nonce: string): Promise<void> {
  try {
    await injectScript(SHIM_PATH, {
      modifyScript(script) {
        script.dataset.akootiNonce = nonce;
      },
    });
  } catch {
    console.info("[akooti] the passkey shim could not be injected on this page");
  }
}

/** Random tag shared with the shim, unguessable by the page. */
function randomNonce(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
