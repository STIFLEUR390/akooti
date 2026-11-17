import { describe, expect, test } from "bun:test";

import { credentialFromJson, decodeBase64Url, encodeBase64Url } from "./credential";

/** base64url of a short marker, so a test never inlines an opaque blob. */
function mark(text: string): string {
  return encodeBase64Url(new TextEncoder().encode(text).buffer as ArrayBuffer);
}

const CREDENTIAL_ID = mark("credential-id");
const CLIENT_DATA = mark("client-data");
const ATTESTATION = mark("attestation-object");
const AUTH_DATA = mark("authenticator-data");
const SIGNATURE = mark("signature");

describe("base64url", () => {
  test("round-trips an arbitrary buffer", () => {
    const bytes = new Uint8Array([0, 1, 250, 251, 252, 255, 62, 63]);
    const encoded = encodeBase64Url(bytes.buffer);
    expect(encoded).not.toContain("+");
    expect(encoded).not.toContain("/");
    expect(encoded).not.toContain("=");
    const decoded = decodeBase64Url(encoded);
    expect(decoded).not.toBeNull();
    expect(new Uint8Array(decoded ?? new ArrayBuffer(0))).toEqual(bytes);
  });

  test("decodes a known value", () => {
    const decoded = decodeBase64Url("aGVsbG8");
    expect(decoded).not.toBeNull();
    expect(new TextDecoder().decode(decoded ?? new ArrayBuffer(0))).toBe("hello");
  });

  test("rejects an empty or non base64url value", () => {
    expect(decodeBase64Url("")).toBeNull();
    expect(decodeBase64Url("a+b/c=")).toBeNull();
    expect(decodeBase64Url("a b")).toBeNull();
    expect(decodeBase64Url("!!!!")).toBeNull();
  });
});

describe("credential reconstruction", () => {
  test("a registration carries an attestation object and its transports", () => {
    const credential = credentialFromJson("create", {
      id: CREDENTIAL_ID,
      clientDataJSON: CLIENT_DATA,
      attestationObject: ATTESTATION,
      publicKeyAlgorithm: -7,
      transports: ["internal", 3],
    });
    expect(credential).not.toBeNull();
    expect(credential?.type).toBe("public-key");
    expect(credential?.id).toBe(CREDENTIAL_ID);
    expect(credential?.getClientExtensionResults()).toEqual({});
    const response = credential?.response as {
      getTransports(): string[];
      getPublicKeyAlgorithm(): number;
      attestationObject: ArrayBuffer;
    };
    expect(response.getTransports()).toEqual(["internal"]);
    expect(response.getPublicKeyAlgorithm()).toBe(-7);
    expect(new TextDecoder().decode(response.attestationObject)).toBe("attestation-object");
  });

  test("an assertion carries a signature and an optional user handle", () => {
    const credential = credentialFromJson("get", {
      id: CREDENTIAL_ID,
      clientDataJSON: CLIENT_DATA,
      authenticatorData: AUTH_DATA,
      signature: SIGNATURE,
      userHandle: mark("user-1"),
    });
    expect(credential).not.toBeNull();
    const response = credential?.response as {
      signature: ArrayBuffer;
      userHandle: ArrayBuffer | null;
    };
    expect(response.signature.byteLength).toBeGreaterThan(0);
    expect(new TextDecoder().decode(response.signature)).toBe("signature");
    expect(response.userHandle).not.toBeNull();

    const handleless = credentialFromJson("get", {
      id: CREDENTIAL_ID,
      clientDataJSON: CLIENT_DATA,
      authenticatorData: AUTH_DATA,
      signature: SIGNATURE,
      userHandle: null,
    });
    expect((handleless?.response as { userHandle: unknown }).userHandle).toBeNull();
  });

  test("an incomplete payload is refused, so the caller falls back", () => {
    expect(credentialFromJson("create", null)).toBeNull();
    expect(credentialFromJson("create", "nope")).toBeNull();
    expect(credentialFromJson("create", { id: CREDENTIAL_ID })).toBeNull();
    // A registration without an attestation object.
    expect(
      credentialFromJson("create", { id: CREDENTIAL_ID, clientDataJSON: CLIENT_DATA }),
    ).toBeNull();
    // An assertion without a signature.
    expect(
      credentialFromJson("get", {
        id: CREDENTIAL_ID,
        clientDataJSON: CLIENT_DATA,
        authenticatorData: AUTH_DATA,
      }),
    ).toBeNull();
    // Values that are not base64url.
    expect(
      credentialFromJson("create", {
        id: "not base64",
        clientDataJSON: CLIENT_DATA,
        attestationObject: ATTESTATION,
      }),
    ).toBeNull();
  });
});
