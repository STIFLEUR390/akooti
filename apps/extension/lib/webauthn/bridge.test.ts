import { describe, expect, test } from "bun:test";

import {
  WEBAUTHN_CHANNEL,
  WebauthnRelay,
  isRelyingPartyAllowed,
  isReplyTo,
  isWebauthnReply,
  isWebauthnRequest,
  pageMessageTarget,
  type WebauthnReply,
  type WebauthnRequest,
} from "./bridge";

const NONCE = "0123456789abcdef0123456789abcdef";

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

describe("message shape", () => {
  test("a request needs the channel, a nonce, an id and a known ceremony", () => {
    const valid: WebauthnRequest = {
      channel: WEBAUTHN_CHANNEL,
      nonce: NONCE,
      id: "create-1",
      kind: "create",
      rpId: null,
      userVerification: null,
    };
    expect(isWebauthnRequest(valid)).toBe(true);
    expect(isWebauthnRequest({ ...valid, nonce: "" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, kind: "delete" })).toBe(false);
    expect(isWebauthnRequest({ ...valid, channel: "other" })).toBe(false);
    expect(isWebauthnRequest(null)).toBe(false);
    expect(isWebauthnRequest("nope")).toBe(false);
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
      rpId: "example.test",
      userVerification: null,
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
  test("posts a tagged request and resolves on the matching reply", async () => {
    const posted: unknown[] = [];
    const { clock: timers, cancelled } = clock();
    const relay = new WebauthnRelay((message) => posted.push(message), NONCE, timers);
    const pending = relay.send("get", { rpId: "example.test" });
    expect(posted).toHaveLength(1);
    expect(posted[0]).toEqual({
      channel: WEBAUTHN_CHANNEL,
      nonce: NONCE,
      id: "get-1",
      kind: "get",
      rpId: "example.test",
      userVerification: null,
    });
    expect(relay.pending).toBe(1);
    expect(relay.receive(reply({ handled: true, credential: { id: "abc" } }))).toBe(true);
    await expect(pending).resolves.toMatchObject({ handled: true });
    expect(relay.pending).toBe(0);
    // The timeout is dropped, so it cannot fire after the ceremony settled.
    expect(cancelled).toHaveLength(1);
  });

  test("ignores a reply with the wrong id, the wrong nonce or an unparsable body", () => {
    const { clock: timers } = clock();
    const relay = new WebauthnRelay(() => undefined, NONCE, timers);
    void relay.send("get");
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
    const pending = relay.send("create", { userVerification: "required" });
    const settled = rejectionMessage(pending);
    handlers[0]?.();
    expect(await settled).toContain("did not answer");
    expect(relay.pending).toBe(0);
  });

  test("cancelling rejects every ceremony still waiting", async () => {
    const { clock: timers } = clock();
    const relay = new WebauthnRelay(() => undefined, NONCE, timers);
    const first = relay.send("create");
    const second = relay.send("get");
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
    void relay.send("get");
    void relay.send("get");
    expect(posted.map((message) => message.id)).toEqual(["get-1", "get-2"]);
    expect(relay.pending).toBe(2);
  });
});
