import { describe, expect, test } from "bun:test";

import {
  MAX_ALLOWED_CREDENTIALS,
  WEBAUTHN_CHANNEL,
  WebauthnRelay,
  isRelyingPartyAllowed,
  isReplyTo,
  isWebauthnReply,
  isWebauthnRequest,
  pageMessageTarget,
  unsupportedCeremony,
  type CeremonyOptions,
  type WebauthnReply,
  type WebauthnRequest,
} from "./bridge";

const NONCE = "0123456789abcdef0123456789abcdef";

/** Options of a plain assertion ceremony, as the shim would read them. */
function ceremony(partial: Partial<CeremonyOptions> = {}): CeremonyOptions {
  return {
    rpId: "example.test",
    rpName: null,
    userHandle: null,
    userName: null,
    challenge: "AQIDBAUGBwgJCgsMDQ4PEA",
    allowCredentials: ["YWJjZGVmZ2hpamts"],
    userVerification: null,
    residentKey: false,
    ...partial,
  };
}

function reply(partial: Partial<WebauthnReply> = {}): WebauthnReply {
  return {
    channel: WEBAUTHN_CHANNEL,
    nonce: NONCE,
    id: "get-1",
    handled: false,
    reason: null,
    credential: null,
    ...partial,
  };
}

/** Clock double: keeps the handlers so a test can fire them, and never fires. */
function clock() {
  const handlers: Array<() => void> = [];
  const cancelled: unknown[] = [];
  return {
    handlers,
    cancelled,
    clock: {
      schedule: (handler: () => void) => {
        handlers.push(handler);
        return handlers.length;
      },
      cancel: (handle: unknown) => {
        cancelled.push(handle);
      },
    },
  };
}

/**
 * Message of a rejected promise, or `"resolved"` when it did not reject.
 *
 * `expect(promise).rejects` is avoided on purpose: the promise here is created
 * before it is awaited, and the explicit handler always settles.
 */
async function rejectionMessage(promise: Promise<unknown>): Promise<string> {
  try {
    await promise;
    return "resolved";
  } catch (error) {
    return error instanceof Error ? error.message : "not-an-error";
  }
}

describe("relying party guard", () => {
  test("accepts the page host and its parent domains", () => {
    expect(isRelyingPartyAllowed("example.test", "example.test")).toBe(true);
    expect(isRelyingPartyAllowed("example.test", "app.example.test")).toBe(true);
    expect(isRelyingPartyAllowed("app.example.test", "app.example.test")).toBe(true);
  });

  test("rejects another site", () => {
    expect(isRelyingPartyAllowed("example.test", "evil.test")).toBe(false);
    expect(isRelyingPartyAllowed("example.test", "appexample.test")).toBe(false);
    expect(isRelyingPartyAllowed("app.example.test", "example.test")).toBe(false);
  });

  test("rejects a bare label, which would be a public suffix", () => {
    expect(isRelyingPartyAllowed("test", "example.test")).toBe(false);
  });

  test("a null relying party lets the browser derive it", () => {
    expect(isRelyingPartyAllowed(null, "example.test")).toBe(true);
    expect(isRelyingPartyAllowed("", "example.test")).toBe(true);
  });
});

describe("what this build cannot serve", () => {
  test("a ceremony that demands user verification goes to the browser", () => {
    expect(unsupportedCeremony(ceremony({ userVerification: "required" }))).toContain(
      "cannot verify a user",
    );
    // `preferred` is a wish, not a requirement: the vault answers without UV
    // and the relying party decides what that is worth.
    expect(unsupportedCeremony(ceremony({ userVerification: "preferred" }))).toBeNull();
    expect(unsupportedCeremony(ceremony({ userVerification: "discouraged" }))).toBeNull();
  });

  test("a discoverable credential goes to the browser", () => {
    // There is no account chooser on the daemon side: an assertion without a
    // listed credential cannot be answered, so promising one would be a lie.
    expect(unsupportedCeremony(ceremony({ residentKey: true }))).toContain("discoverable");
    expect(unsupportedCeremony(ceremony())).toBeNull();
  });
});

describe("message shape", () => {
  test("a request needs the channel, a nonce, an id, a known ceremony and a challenge", () => {
    const valid: WebauthnRequest = {
      channel: WEBAUTHN_CHANNEL,
      nonce: NONCE,
      id: "create-1",
      kind: "create",
      ...ceremony(),
    };
    expect(isWebauthnRequest(valid)).toBe(true);
    expect(isWebauthnRequest({ ...valid, nonce: "" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, kind: "delete" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, channel: "other" })).toBe(false);
    expect(isWebauthnRequest(null)).toBe(false);
    expect(isWebauthnRequest("nope")).toBe(false);
  });

  test("a request with a field that is not base64url is refused", () => {
    const valid: WebauthnRequest = {
      channel: WEBAUTHN_CHANNEL,
      nonce: NONCE,
      id: "get-1",
      kind: "get",
      ...ceremony(),
    };
    expect(isWebauthnRequest({ ...valid, challenge: "" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, challenge: "not base64url!" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, allowCredentials: ["a b"] })).toBe(false);
    expect(isWebauthnRequest({ ...valid, allowCredentials: "abc" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, userHandle: "%%%" })).toBe(false);
    // A page cannot make the daemon chew through an unbounded list.
    expect(
      isWebauthnRequest({
        ...valid,
        allowCredentials: Array.from({ length: MAX_ALLOWED_CREDENTIALS + 1 }, () => "YWJj"),
      }),
    ).toBe(false);
  });

  test("a reply needs the channel, an id and a handled flag", () => {
    expect(isWebauthnReply(reply())).toBe(true);
    expect(isWebauthnReply({ ...reply(), handled: "yes" })).toBe(false);
    expect(isWebauthnReply({ ...reply(), credential: 3 })).toBe(false);
    expect(isWebauthnReply(undefined)).toBe(false);
  });

  test("a reply only matches its own request", () => {
    const request: WebauthnRequest = {
      channel: WEBAUTHN_CHANNEL,
      nonce: NONCE,
      id: "get-1",
      kind: "get",
      ...ceremony(),
    };
    expect(isReplyTo(request, reply())).toBe(true);
    expect(isReplyTo(request, reply({ id: "get-2" }))).toBe(false);
    // A page that learned the id still cannot answer without the nonce.
    expect(isReplyTo(request, reply({ nonce: "forged" }))).toBe(false);
  });

  test("the post-message target tolerates a sandboxed frame", () => {
    expect(pageMessageTarget("https://example.test")).toBe("https://example.test");
    expect(pageMessageTarget("null")).toBe("*");
    expect(pageMessageTarget("")).toBe("*");
  });
});

describe("ceremony relay", () => {
  test("posts the ceremony untouched and resolves on the matching reply", async () => {
    const posted: unknown[] = [];
    const { clock: timers, cancelled } = clock();
    const relay = new WebauthnRelay((message) => posted.push(message), NONCE, timers);
    const pending = relay.send("get", ceremony());
    expect(posted).toHaveLength(1);
    expect(posted[0]).toEqual({
      channel: WEBAUTHN_CHANNEL,
      nonce: NONCE,
      id: "get-1",
      kind: "get",
      rpId: "example.test",
      rpName: null,
      userHandle: null,
      userName: null,
      challenge: "AQIDBAUGBwgJCgsMDQ4PEA",
      allowCredentials: ["YWJjZGVmZ2hpamts"],
      userVerification: null,
      residentKey: false,
    });
    expect(relay.pending).toBe(1);
    expect(
      relay.receive(reply({ handled: true, credential: { id: "abc", clientDataJSON: "e30" } })),
    ).toBe(true);
    await expect(pending).resolves.toMatchObject({ handled: true });
    expect(relay.pending).toBe(0);
    // The timeout is dropped, so it cannot fire after the ceremony settled.
    expect(cancelled).toHaveLength(1);
  });

  test("ignores a reply with the wrong id, the wrong nonce or an unparsable body", () => {
    const { clock: timers } = clock();
    const relay = new WebauthnRelay(() => undefined, NONCE, timers);
    void relay.send("get", ceremony());
    expect(relay.receive({ id: "get-9" })).toBe(false);
    expect(relay.receive(reply({ nonce: "forged" }))).toBe(false);
    expect(relay.receive("nope")).toBe(false);
    expect(relay.receive(null)).toBe(false);
    expect(relay.receive(reply({ id: "get-0" }))).toBe(false);
    expect(relay.pending).toBe(1);
  });

  test("a silent extension ends the ceremony with an error, so the caller can fall back", async () => {
    const { handlers, clock: timers } = clock();
    const relay = new WebauthnRelay(() => undefined, NONCE, timers, 1_000);
    const pending = relay.send("create", ceremony({ userVerification: "required" }));
    const settled = rejectionMessage(pending);
    handlers[0]?.();
    expect(await settled).toContain("did not answer");
    expect(relay.pending).toBe(0);
  });

  test("cancelling rejects every ceremony still waiting", async () => {
    const { clock: timers } = clock();
    const relay = new WebauthnRelay(() => undefined, NONCE, timers);
    const first = relay.send("create", ceremony());
    const second = relay.send("get", ceremony());
    const settled = Promise.all([rejectionMessage(first), rejectionMessage(second)]);
    relay.cancel();
    expect(await settled).toEqual([
      "akooti: the passkey bridge stopped",
      "akooti: the passkey bridge stopped",
    ]);
    expect(relay.pending).toBe(0);
  });

  test("ids keep increasing across ceremonies", () => {
    const { clock: timers } = clock();
    const posted: Array<{ id: string }> = [];
    const relay = new WebauthnRelay(
      (message) => posted.push(message as { id: string }),
      NONCE,
      timers,
    );
    void relay.send("get", ceremony());
    void relay.send("get", ceremony());
    expect(posted.map((message) => message.id)).toEqual(["get-1", "get-2"]);
    expect(relay.pending).toBe(2);
  });
});
