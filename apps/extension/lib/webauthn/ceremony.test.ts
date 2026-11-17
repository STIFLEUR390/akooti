import { describe, expect, test } from "bun:test";

import type { PasskeyRequest } from "../messages";
import type { DaemonResponse } from "../protocol";
import { handOff, passkeyReply, passkeyRequest } from "./ceremony";

function relayed(partial: Partial<PasskeyRequest> = {}): PasskeyRequest {
  return {
    kind: "create",
    origin: "https://example.test",
    rpId: "example.test",
    rpName: "Example",
    userHandle: "BwcHBwc",
    userName: "user@example.test",
    challenge: "AQIDBAUGBwgJCgsMDQ4PEA",
    allowCredentials: [],
    userVerification: null,
    residentKey: false,
    ...partial,
  };
}

const VERBATIM = "MTIzNDU2Nzg5MGFiY2RlZg";

describe("the request sent for one ceremony", () => {
  test("a registration carries the relying party and the account", () => {
    expect(passkeyRequest("ak_mcp_token", relayed())).toEqual({
      method: "vaultPasskeyRegister",
      token: "ak_mcp_token",
      origin: "https://example.test",
      rpId: "example.test",
      rpName: "Example",
      userHandle: "BwcHBwc",
      userName: "user@example.test",
      challenge: "AQIDBAUGBwgJCgsMDQ4PEA",
    });
  });

  test("an assertion carries the credentials the site accepts", () => {
    expect(
      passkeyRequest("ak_mcp_token", relayed({ kind: "get", allowCredentials: ["YWJj", "ZGVm"] })),
    ).toEqual({
      method: "vaultPasskeyAssert",
      token: "ak_mcp_token",
      origin: "https://example.test",
      rpId: "example.test",
      challenge: "AQIDBAUGBwgJCgsMDQ4PEA",
      allowCredentials: ["YWJj", "ZGVm"],
    });
  });

  test("a missing name falls back to the relying party id, never to invented text", () => {
    const request = passkeyRequest("t", relayed({ rpName: null, userHandle: null }));
    expect(request).toMatchObject({ rpName: "example.test", userHandle: "" });
  });
});

describe("the reply handed to the page", () => {
  test("a served ceremony is marked handled and carries the credential", () => {
    const response = {
      protocolVersion: 2,
      kind: "passkeyAssertion" as const,
      id: VERBATIM,
      clientDataJSON: "e30",
      authenticatorData: "AAAA",
      signature: "MEUCIQ",
      userHandle: VERBATIM,
    } satisfies DaemonResponse;
    expect(passkeyReply(response)).toEqual({
      ok: true,
      kind: "passkey",
      handled: true,
      reason: null,
      credential: response,
    });
  });

  test("a daemon refusal hands the ceremony back instead of failing the site", () => {
    const reply = passkeyReply({
      protocolVersion: 2,
      kind: "error",
      code: "not_found",
      message: "item not found",
    });
    expect(reply).toEqual({
      ok: true,
      kind: "passkey",
      handled: false,
      reason: "item not found",
      credential: null,
    });
  });

  test("an answer of another kind is a hand-off too", () => {
    const reply = passkeyReply({ protocolVersion: 2, kind: "search", items: [] });
    expect(reply).toMatchObject({ ok: true, handled: false });
  });

  test("a hand-off never carries a value, only a reason", () => {
    expect(handOff("the daemon is not running")).toEqual({
      ok: true,
      kind: "passkey",
      handled: false,
      reason: "the daemon is not running",
      credential: null,
    });
  });
});
