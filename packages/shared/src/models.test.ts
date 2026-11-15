import { describe, expect, test } from "bun:test";

import { SYSTEM_FOLDERS, isSystemFolder } from "./models";
import { NATIVE_MESSAGE_HEADER_BYTES, decodeNativeMessage, encodeNativeMessage } from "./protocol";

describe("system folders", () => {
  test("match the SQLCipher seed", () => {
    expect(SYSTEM_FOLDERS.map((folder) => folder.slug)).toEqual([
      "personal",
      "dev-local",
      "agents-mcp",
    ]);
    expect(new Set(SYSTEM_FOLDERS.map((folder) => folder.id)).size).toBe(3);
  });

  test("cannot be mistaken for user folders", () => {
    expect(isSystemFolder({ isSystem: true })).toBe(true);
    expect(isSystemFolder({ isSystem: false })).toBe(false);
  });
});

describe("native messaging framing", () => {
  test("round-trips a message with a 4-byte header", () => {
    const frame = encodeNativeMessage({
      id: "1",
      type: "request",
      method: "vault.search",
      payload: { query: "docker" },
    });

    expect(frame.byteLength).toBeGreaterThan(NATIVE_MESSAGE_HEADER_BYTES);
    expect(decodeNativeMessage(frame)).toEqual({
      id: "1",
      type: "request",
      method: "vault.search",
      payload: { query: "docker" },
    });
  });

  test("rejects a truncated frame", () => {
    expect(() => decodeNativeMessage(new Uint8Array(2))).toThrow();
  });
});
