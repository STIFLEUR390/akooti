/**
 * Turning what the daemon answered into what the page gets.
 *
 * One rule decides everything here: a ceremony that the vault cannot serve is
 * handed back to the browser, never turned into an error. A site that could
 * register or sign in before Akooti was installed must still be able to, and a
 * WebAuthn failure is silent and confusing for the user.
 */
import type { RuntimeReply } from "../messages";
import type { DaemonRequest, DaemonResponse } from "../protocol";
import type { PasskeyRequest } from "../messages";

/** Builds the daemon request that serves one relayed ceremony. */
export function passkeyRequest(token: string, request: PasskeyRequest): DaemonRequest {
  if (request.kind === "create") {
    return {
      method: "vaultPasskeyRegister",
      token,
      origin: request.origin,
      rpId: request.rpId,
      // `rp.name` is mandatory for the browser; falling back to the id keeps a
      // page that omits it working instead of failing deep in the vault.
      rpName: request.rpName ?? request.rpId,
      // Also mandatory upstream. An empty handle is refused by the vault rather
      // than invented here: a credential with a fabricated user is worse than a
      // fallback to the browser.
      userHandle: request.userHandle ?? "",
      userName: request.userName ?? "",
      challenge: request.challenge,
    };
  }
  return {
    method: "vaultPasskeyAssert",
    token,
    origin: request.origin,
    rpId: request.rpId,
    challenge: request.challenge,
    allowCredentials: request.allowCredentials,
  };
}

/**
 * Turns a daemon answer into the reply the shim expects.
 *
 * A response of the wrong kind is a hand-off, not an error: the shim's job is
 * to decide whether *it* serves the credential, and the answer "no" is always
 * safe.
 */
export function passkeyReply(response: DaemonResponse): RuntimeReply {
  if (response.kind === "error") {
    return handOff(response.message);
  }
  if (response.kind === "passkeyRegistration" || response.kind === "passkeyAssertion") {
    return {
      ok: true,
      kind: "passkey",
      handled: true,
      reason: null,
      credential: response,
    };
  }
  return handOff("the daemon answered no passkey ceremony");
}

/** Hands one ceremony back to the browser with a non-revealing reason. */
export function handOff(reason: string): RuntimeReply {
  return { ok: true, kind: "passkey", handled: false, reason, credential: null };
}
