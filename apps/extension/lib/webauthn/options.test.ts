import { describe, expect, test } from "bun:test";

import { creationOptions, requestOptions } from "./options";

/** `BufferSource` of a few bytes, as a page would pass it. */
function bytes(...values: number[]): Uint8Array<ArrayBuffer> {
  return new Uint8Array(values);
}

function label(value: string): Uint8Array<ArrayBuffer> {
  return new TextEncoder().encode(value);
}

describe("credentials.create", () => {
  test("reads the relying party, the account and the challenge", () => {
    const options = creationOptions({
      publicKey: {
        challenge: bytes(1, 2, 3),
        rp: { id: "example.test", name: "Example" },
        user: { id: bytes(9, 9), name: "user@example.test", displayName: "User" },
        pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        authenticatorSelection: { userVerification: "preferred" },
      },
    });
    expect(options).toEqual({
      rpId: "example.test",
      rpName: "Example",
      userHandle: "CQk",
      userName: "user@example.test",
      challenge: "AQID",
      allowCredentials: [],
      userVerification: "preferred",
      residentKey: false,
    });
  });

  test("a page may omit the relying party id, which the browser then derives", () => {
    const options = creationOptions({
      publicKey: {
        challenge: bytes(1),
        rp: { name: "Example" },
        user: { id: bytes(1), name: "user", displayName: "User" },
        pubKeyCredParams: [],
      },
    });
    expect(options?.rpId).toBeNull();
    expect(options?.userVerification).toBeNull();
  });

  test("a required resident key is reported, so the relay can hand off", () => {
    const options = creationOptions({
      publicKey: {
        challenge: bytes(1),
        rp: { name: "Example" },
        user: { id: bytes(1), name: "user", displayName: "User" },
        pubKeyCredParams: [],
        authenticatorSelection: { residentKey: "required" },
      },
    });
    expect(options?.residentKey).toBe(true);

    // The legacy spelling is the same promise.
    const legacy = creationOptions({
      publicKey: {
        challenge: bytes(1),
        rp: { name: "Example" },
        user: { id: bytes(1), name: "user", displayName: "User" },
        pubKeyCredParams: [],
        authenticatorSelection: { requireResidentKey: true },
      },
    });
    expect(legacy?.residentKey).toBe(true);
  });

  test("the challenge is the page's bytes, not its whole buffer", () => {
    // A view over a larger allocation: the neighbours must not be encoded.
    const buffer: Uint8Array<ArrayBuffer> = new Uint8Array([0xaa, 0xbb, 0xcc, 0xdd]);
    const options = creationOptions({
      publicKey: {
        challenge: new Uint8Array(buffer.buffer, 1, 2),
        rp: { name: "Example" },
        user: { id: new Uint8Array(buffer.buffer, 0, 1), name: "user", displayName: "User" },
        pubKeyCredParams: [],
      },
    });
    expect(options?.challenge).toBe("u8w");
    expect(options?.userHandle).toBe("qg");
  });

  test("a call without a challenge is not relayed at all", () => {
    expect(creationOptions(undefined)).toBeNull();
    expect(creationOptions({})).toBeNull();
    expect(creationOptions({ publicKey: { rp: { name: "Example" } } as never })).toBeNull();
  });
});

describe("credentials.get", () => {
  test("reads the challenge and the credentials the site accepts", () => {
    const options = requestOptions({
      publicKey: {
        challenge: bytes(4, 5, 6),
        rpId: "example.test",
        allowCredentials: [
          { type: "public-key", id: label("first") },
          { type: "public-key", id: label("second") },
        ],
        userVerification: "required",
      },
    });
    expect(options).toEqual({
      rpId: "example.test",
      rpName: null,
      userHandle: null,
      userName: null,
      challenge: "BAUG",
      allowCredentials: ["Zmlyc3Q", "c2Vjb25k"],
      userVerification: "required",
      residentKey: false,
    });
  });

  test("an assertion with no listed credential is still relayed, and refused later", () => {
    // The vault has no account chooser: it answers `invalid_input`, the shim
    // falls back to the browser, and the site's own authenticator takes over.
    const options = requestOptions({
      publicKey: { challenge: bytes(1) },
    });
    expect(options?.allowCredentials).toEqual([]);
    expect(options?.rpId).toBeNull();
  });

  test("a call without a challenge is not relayed at all", () => {
    expect(requestOptions(undefined)).toBeNull();
    expect(requestOptions({})).toBeNull();
  });
});
