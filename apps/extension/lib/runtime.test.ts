import { describe, expect, test } from "bun:test";

import { RUNTIME_ERROR_CODES, type RuntimeReply, type RuntimeRequest } from "./messages";
import { asRuntimeReply, callRuntime, isContextLost, isErrorCode } from "./runtime";

const STATUS: RuntimeReply = {
  ok: true,
  kind: "status",
  status: {
    paired: false,
    maskedToken: null,
    label: null,
    daemon: null,
    daemonError: null,
  },
};

describe("runtime client", () => {
  test("returns the reply of the worker unchanged", async () => {
    const reply = await callRuntime(async () => STATUS, { type: "akooti:status" });
    expect(reply).toEqual(STATUS);
  });

  test("passes the request through untouched", async () => {
    const seen: RuntimeRequest[] = [];
    await callRuntime(
      async (message) => {
        seen.push(message);
        return { ok: true, kind: "unpaired" };
      },
      { type: "akooti:pair", token: "ak_mcp_test", label: "extension" },
    );
    expect(seen[0]).toEqual({ type: "akooti:pair", token: "ak_mcp_test", label: "extension" });
  });

  test("a reloaded extension becomes context_lost, not a closed desktop app", async () => {
    const cases = [
      "Extension context invalidated.",
      "Could not establish connection. Receiving end does not exist.",
      "Cannot read properties of undefined (reading 'sendMessage')",
    ];
    for (const message of cases) {
      const reply = await callRuntime(
        async () => {
          throw new Error(message);
        },
        { type: "akooti:status" },
      );
      expect(reply).toMatchObject({ ok: false, kind: "error", code: "context_lost" });
    }
  });

  test("another transport failure stays daemon_unavailable with a readable detail", async () => {
    const reply = await callRuntime(
      async () => {
        throw new Error("port closed before an answer");
      },
      { type: "akooti:status" },
    );
    expect(reply).toMatchObject({ ok: false, code: "daemon_unavailable" });
    if (reply.ok) {
      throw new Error("the reply should be a failure");
    }
    expect(reply.message).toBe("port closed before an answer");
  });

  test("a transport that throws a non-error still yields a reply", async () => {
    const reply = await callRuntime(
      async () => {
        throw "boom";
      },
      { type: "akooti:status" },
    );
    expect(reply.ok).toBe(false);
    if (reply.ok) {
      throw new Error("the reply should be a failure");
    }
    expect(reply.message.length).toBeGreaterThan(0);
  });

  test("only a lost context is told apart from a plain transport failure", () => {
    expect(isContextLost(new Error("Extension context invalidated."))).toBe(true);
    expect(
      isContextLost(new Error("Cannot read properties of undefined (reading 'sendMessage')")),
    ).toBe(true);
    expect(isContextLost(new Error("the daemon answered nothing"))).toBe(false);
    expect(isContextLost("boom")).toBe(false);
    expect(isContextLost(undefined)).toBe(false);
  });
});

describe("reply validation", () => {
  test("accepts a well-formed failure", () => {
    expect(asRuntimeReply({ ok: false, kind: "error", code: "locked", message: "locked" })).toEqual(
      {
        ok: false,
        kind: "error",
        code: "locked",
        message: "locked",
      },
    );
  });

  test("rejects an unknown code, a missing message or a bad discriminant", () => {
    const cases: unknown[] = [
      null,
      "nope",
      {},
      { ok: false, kind: "error", code: "made_up", message: "x" },
      { ok: false, kind: "error", code: "locked" },
      { ok: false, kind: "error", message: "no code" },
      { ok: false, code: "locked", message: "no kind" },
      { ok: true },
    ];
    for (const value of cases) {
      const reply = asRuntimeReply(value);
      expect(reply.ok).toBe(false);
      if (!reply.ok) {
        expect(reply.code).toBe("internal");
      }
    }
  });

  test("accepts every documented failure code", () => {
    for (const code of RUNTIME_ERROR_CODES) {
      expect(isErrorCode(code)).toBe(true);
      const reply = asRuntimeReply({ ok: false, kind: "error", code, message: "detail" });
      expect(reply.ok).toBe(false);
    }
    expect(isErrorCode("something_else")).toBe(false);
    expect(isErrorCode(undefined)).toBe(false);
  });
});
