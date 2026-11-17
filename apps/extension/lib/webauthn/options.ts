/**
 * Reading a WebAuthn ceremony out of the options the page passed.
 *
 * Kept away from the shim so it can be tested: this is the only place where a
 * page-provided `BufferSource` is turned into something that can cross a
 * message channel, and a wrong slice here would relay a challenge nobody asked
 * for.
 */
import type { CeremonyOptions } from "./bridge";
import { encodeBase64Url } from "./credential";

/**
 * Parameters of `navigator.credentials.create`, or `null` when the call is not
 * a ceremony this shim can relay.
 *
 * `null` is not an error handler: the caller falls back to the browser, which
 * raises the `TypeError` the page deserves. Nothing is invented here, so a
 * missing challenge never becomes a default one.
 */
export function creationOptions(options?: CredentialCreationOptions): CeremonyOptions | null {
  const publicKey = options?.publicKey;
  if (!publicKey || publicKey.challenge === undefined) {
    return null;
  }
  return {
    rpId: publicKey.rp?.id ?? null,
    rpName: publicKey.rp?.name ?? null,
    userHandle: publicKey.user?.id ? encodeBase64Url(toBytes(publicKey.user.id)) : null,
    userName: publicKey.user?.name ?? null,
    challenge: encodeBase64Url(toBytes(publicKey.challenge)),
    // A registration lists no credential: the vault mints a new one.
    allowCredentials: [],
    userVerification: publicKey.authenticatorSelection?.userVerification ?? null,
    residentKey:
      publicKey.authenticatorSelection?.residentKey === "required" ||
      publicKey.authenticatorSelection?.requireResidentKey === true,
  };
}

/** Parameters of `navigator.credentials.get`, with the same `null` contract. */
export function requestOptions(options?: CredentialRequestOptions): CeremonyOptions | null {
  const publicKey = options?.publicKey;
  if (!publicKey || publicKey.challenge === undefined) {
    return null;
  }
  return {
    rpId: publicKey.rpId ?? null,
    rpName: null,
    userHandle: null,
    userName: null,
    challenge: encodeBase64Url(toBytes(publicKey.challenge)),
    allowCredentials: (publicKey.allowCredentials ?? []).map((credential) =>
      encodeBase64Url(toBytes(credential.id)),
    ),
    userVerification: publicKey.userVerification ?? null,
    // A `get` never asks for a new credential, so nothing is discoverable here.
    residentKey: false,
  };
}

/**
 * Copies a `BufferSource` into a plain byte array.
 *
 * A view may cover only part of its buffer, so all three fields matter: reading
 * `source.buffer` alone would encode the whole underlying allocation instead of
 * the bytes the page actually chose.
 */
function toBytes(source: BufferSource): Uint8Array {
  if (source instanceof ArrayBuffer) {
    return new Uint8Array(source);
  }
  return new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
}
