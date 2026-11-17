import { describe, expect, test } from "bun:test";

import { internalError, isContentRequest, isRuntimeRequest } from "./messages";

describe("message guards", () => {
  test("a runtime request is tagged with the namespace", () => {
    expect(isRuntimeRequest({ type: "akooti:status" })).toBe(true);
    expect(isRuntimeRequest({ type: "akooti:fill", itemId: "item-1" })).toBe(true);
    expect(isRuntimeRequest({ type: "other:status" })).toBe(false);
    expect(isRuntimeRequest({})).toBe(false);
    expect(isRuntimeRequest(null)).toBe(false);
    expect(isRuntimeRequest("akooti:status")).toBe(false);
  });

  test("only the two injection verbs reach a content script", () => {
    expect(isContentRequest({ type: "akooti:inject", mode: "detect" })).toBe(true);
    expect(isContentRequest({ type: "akooti:inspect", mode: "capture" })).toBe(true);
    expect(isContentRequest({ type: "akooti:status" })).toBe(false);
    expect(isContentRequest(undefined)).toBe(false);
  });

  test("the worker failure helper is a well-formed failure", () => {
    const reply = internalError("the page did not answer");
    expect(reply).toEqual({
      ok: false,
      kind: "error",
      code: "internal",
      message: "the page did not answer",
    });
  });
});
