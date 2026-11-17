import { describe, expect, test } from "bun:test";

import {
  PAIRING_KEY,
  clearPairing,
  isTokenShaped,
  maskedToken,
  readPairing,
  writePairing,
  type PairingStore,
} from "./pairing";

const TOKEN = `ak_mcp_${"0123456789abcdef".repeat(4)}`;

/** In-memory `storage.local` double. */
class MemoryStore implements PairingStore {
  readonly data = new Map<string, unknown>();

  async get(keys: string): Promise<Record<string, unknown>> {
    return { [keys]: this.data.get(keys) };
  }

  async set(items: Record<string, unknown>): Promise<void> {
    for (const [key, value] of Object.entries(items)) {
      this.data.set(key, value);
    }
  }

  async remove(key: string): Promise<void> {
    this.data.delete(key);
  }
}

describe("token shape", () => {
  test("accepts a prefix plus 64 lowercase hex characters", () => {
    expect(isTokenShaped(TOKEN)).toBe(true);
    expect(isTokenShaped(`  ${TOKEN}\n`)).toBe(true);
  });

  test("rejects everything else", () => {
    expect(isTokenShaped("")).toBe(false);
    expect(isTokenShaped("ak_mcp_")).toBe(false);
    expect(isTokenShaped(`ak_mcp_${"a".repeat(63)}`)).toBe(false);
    expect(isTokenShaped(`ak_mcp_${"a".repeat(65)}`)).toBe(false);
    expect(isTokenShaped(`ak_mcp_${"A".repeat(64)}`)).toBe(false);
    expect(isTokenShaped(`sk_mcp_${"a".repeat(64)}`)).toBe(false);
    expect(isTokenShaped(`ak_mcp_${"g".repeat(64)}`)).toBe(false);
  });

  test("the masked form keeps a prefix and a suffix and nothing else", () => {
    const masked = maskedToken(TOKEN);
    expect(masked).toBe("ak_mcp_0123…cdef");
    expect(masked).not.toContain("456789");
    expect(maskedToken("not-a-token")).toBe("—");
  });
});

describe("pairing storage", () => {
  test("writes a validated pairing and reads it back", async () => {
    const store = new MemoryStore();
    const stored = await writePairing(store, TOKEN, { label: " extension ", now: 42 });
    expect(stored).toEqual({ token: TOKEN, label: "extension", pairedAt: 42 });
    expect(await readPairing(store)).toEqual(stored);
    expect(store.data.get(PAIRING_KEY)).toEqual(stored);
  });

  test("stores a null label when none is given", async () => {
    const store = new MemoryStore();
    const stored = await writePairing(store, TOKEN, { now: 1 });
    expect(stored.label).toBeNull();
  });

  test("refuses to store a malformed token", async () => {
    const store = new MemoryStore();
    await expect(writePairing(store, "nope")).rejects.toThrow(/ak_mcp_/);
    expect(store.data.size).toBe(0);
  });

  test("an empty store reads as not paired", async () => {
    expect(await readPairing(new MemoryStore())).toBeNull();
  });

  test("a hand-edited entry that no longer looks like a token reads as not paired", async () => {
    const store = new MemoryStore();
    store.data.set(PAIRING_KEY, { token: "garbage", label: null, pairedAt: 1 });
    expect(await readPairing(store)).toBeNull();
    store.data.set(PAIRING_KEY, { token: TOKEN, label: 3, pairedAt: 1 });
    expect(await readPairing(store)).toBeNull();
  });

  test("unpairing removes the secret", async () => {
    const store = new MemoryStore();
    await writePairing(store, TOKEN);
    await clearPairing(store);
    expect(store.data.has(PAIRING_KEY)).toBe(false);
    expect(await readPairing(store)).toBeNull();
  });
});
