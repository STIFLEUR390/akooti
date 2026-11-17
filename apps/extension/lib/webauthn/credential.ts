/**
 * Reconstruction of a WebAuthn credential from the JSON the daemon would send.
 *
 * A relying party expects a real `PublicKeyCredential`; the browser only hands
 * that class to an authenticator, so a credential produced elsewhere has to be
 * rebuilt. The object below is a duck-typed stand-in: it carries every member a
 * page reads (`id`, `rawId`, `type`, `response`, `getClientExtensionResults`)
 * and the buffers are real `ArrayBuffer`s, but `instanceof PublicKeyCredential`
 * is false — a limitation that only the daemon-side authenticator can remove
 * (task 6.4, see `docs/architecture.md` §8).
 *
 * Nothing here talks to the network and nothing reads a secret: a credential is
 * public by design (it is meant to be handed to the page).
 */

/** Fields the daemon returns for a ceremony, base64url encoded. */
export interface PasskeyCredentialJson {
  /** Credential id, base64url. */
  id: string;
  /** `clientDataJSON` the daemon signed over, base64url. */
  clientDataJSON: string;
  /** Registration only: `attestationObject`, base64url. */
  attestationObject?: string | null;
  /** Registration only: COSE algorithm identifier. */
  publicKeyAlgorithm?: number | null;
  /** Registration only: transports the authenticator supports. */
  transports?: string[] | null;
  /** Assertion only: `authenticatorData`, base64url. */
  authenticatorData?: string | null;
  /** Assertion only: `signature`, base64url. */
  signature?: string | null;
  /** Assertion only: the user handle, base64url (`null` when not returned). */
  userHandle?: string | null;
}

/** Members every credential exposes to the page. */
export interface CredentialShim {
  readonly id: string;
  readonly rawId: ArrayBuffer;
  readonly type: "public-key";
  readonly authenticatorAttachment: string;
  readonly response: Record<string, unknown>;
  getClientExtensionResults(): Record<string, unknown>;
}

/** Decodes base64url (unpadded) into a buffer, or `null` when invalid. */
export function decodeBase64Url(value: string): ArrayBuffer | null {
  if (value.length === 0 || !/^[A-Za-z0-9_-]+$/.test(value)) {
    return null;
  }
  const padded = value
    .replace(/-/g, "+")
    .replace(/_/g, "/")
    .padEnd(value.length + ((4 - (value.length % 4)) % 4), "=");
  try {
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes.buffer;
  } catch {
    return null;
  }
}

/** Encodes a buffer as base64url, used for the credential id. */
export function encodeBase64Url(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/**
 * Rebuilds a credential from the daemon payload.
 *
 * Returns `null` for anything incomplete or malformed: the caller then falls
 * back to the browser authenticator instead of handing the page a broken
 * object, which would surface as an obscure TypeError deep in the site.
 */
export function credentialFromJson(kind: "create" | "get", json: unknown): CredentialShim | null {
  if (typeof json !== "object" || json === null) {
    return null;
  }
  const payload = json as Partial<PasskeyCredentialJson>;
  if (typeof payload.id !== "string" || typeof payload.clientDataJSON !== "string") {
    return null;
  }
  const rawId = decodeBase64Url(payload.id);
  const clientDataJSON = decodeBase64Url(payload.clientDataJSON);
  if (!rawId || !clientDataJSON) {
    return null;
  }
  const response =
    kind === "create"
      ? registrationResponse(payload, clientDataJSON)
      : assertionResponse(payload, clientDataJSON);
  if (!response) {
    return null;
  }
  return {
    id: encodeBase64Url(rawId),
    rawId,
    type: "public-key",
    authenticatorAttachment: "platform",
    response,
    getClientExtensionResults: () => ({}),
  };
}

function registrationResponse(
  payload: Partial<PasskeyCredentialJson>,
  clientDataJSON: ArrayBuffer,
): Record<string, unknown> | null {
  if (typeof payload.attestationObject !== "string") {
    return null;
  }
  const attestationObject = decodeBase64Url(payload.attestationObject);
  if (!attestationObject) {
    return null;
  }
  const transports = Array.isArray(payload.transports)
    ? payload.transports.filter((item): item is string => typeof item === "string")
    : [];
  return {
    clientDataJSON,
    attestationObject,
    getTransports: () => transports,
    getPublicKeyAlgorithm: () =>
      typeof payload.publicKeyAlgorithm === "number" ? payload.publicKeyAlgorithm : -7,
  };
}

function assertionResponse(
  payload: Partial<PasskeyCredentialJson>,
  clientDataJSON: ArrayBuffer,
): Record<string, unknown> | null {
  if (typeof payload.authenticatorData !== "string" || typeof payload.signature !== "string") {
    return null;
  }
  const authenticatorData = decodeBase64Url(payload.authenticatorData);
  const signature = decodeBase64Url(payload.signature);
  if (!authenticatorData || !signature) {
    return null;
  }
  const userHandle =
    typeof payload.userHandle === "string" ? decodeBase64Url(payload.userHandle) : null;
  return { clientDataJSON, authenticatorData, signature, userHandle };
}
