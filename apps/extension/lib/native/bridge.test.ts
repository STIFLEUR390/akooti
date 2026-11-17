import { describe, expect, test } from "bun:test";

import { BridgeError, NativeBridge, bridgeErrorFromResponse, type NativePortLike } from "./bridge";
import type { DaemonRequest, DaemonResponse } from "../protocol";

/** Port double: records frames and lets a test answer them. */
class FakePort implements NativePortLike {
  readonly sent: DaemonRequest[] = [];
  disconnects = 0;
  private readonly onMessageListeners: Array<(message: unknown) => void> = [];
  private readonly onDisconnectListeners: Array<() => void> = [];

  readonly onMessage = {
    addListener: (listener: (message: unknown) => void) => {
      this.onMessageListeners.push(listener);
    },
  };

  readonly onDisconnect = {
    addListener: (listener: () => void) => {
      this.onDisconnectListeners.push(listener);
    },
  };

  postMessage(message: unknown): void {
    this.sent.push(message as DaemonRequest);
  }

  disconnect(): void {
    this.disconnects += 1;
  }

  /** The daemon answers. */
  receive(message: unknown): void {
    for (const listener of [...this.onMessageListeners]) {
      listener(message);
    }
  }

  /** The host exits (browser restart, daemon stop). */
  close(): void {
    for (const listener of [...this.onDisconnectListeners]) {
      listener();
    }
  }
}

const HANDSHAKE: DaemonResponse = {
  protocolVersion: 2,
  kind: "handshake",
  vaultUnlocked: true,
  capabilities: ["search", "extension_fill"],
};

const TOKEN = `ak_mcp_${"a".repeat(64)}`;

/** Drains every pending microtask, so the bridge queue advances. */
async function flush(): Promise<void> {
  for (let index = 0; index < 3; index += 1) {
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
  }
}

/**
 * Code of a rejected call, or `"resolved"` when it did not reject.
 *
 * Written as a `try`/`catch` rather than `expect(promise).rejects`: the promise
 * is created and awaited at different times in these tests, and the explicit
 * handler is the version that always settles.
 */
async function rejectionCode(promise: Promise<unknown>): Promise<string> {
  try {
    await promise;
    return "resolved";
  } catch (error) {
    return error instanceof BridgeError ? error.code : "not-a-bridge-error";
  }
}

function harness() {
  const port = new FakePort();
  const timers: Array<() => void> = [];
  const cleared: unknown[] = [];
  let connects = 0;
  const bridge = new NativeBridge({
    connectNative: () => {
      connects += 1;
      return port;
    },
    setTimeout: (handler) => {
      timers.push(handler);
      return timers.length;
    },
    clearTimeout: (handle) => {
      cleared.push(handle);
    },
  });
  return {
    bridge,
    port,
    timers,
    cleared,
    connects: () => connects,
  };
}

describe("native bridge handshake", () => {
  test("handshakes before the first request, then forwards it", async () => {
    const { bridge, port } = harness();
    const pending = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    expect(port.sent[0]).toEqual({
      method: "handshake",
      protocolVersion: 2,
      client: "native-host",
    });
    expect(port.sent).toHaveLength(1);
    port.receive(HANDSHAKE);
    await flush();
    expect(port.sent[1]).toEqual({ method: "vaultStatus", token: TOKEN });
    port.receive({
      protocolVersion: 2,
      kind: "vaultStatus",
      tokenName: "extension",
      scopes: ["read_metadata", "blind_fill"],
      folderIds: [],
      expiresAt: null,
    });
    const response = await pending;
    expect(response.kind).toBe("vaultStatus");
    expect(bridge.capabilities).toEqual(["search", "extension_fill"]);
  });

  test("a protocol mismatch is sticky until the bridge is reset", async () => {
    const { bridge, port, connects } = harness();
    const pending = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    port.receive({ ...HANDSHAKE, protocolVersion: 3 });
    expect(await rejectionCode(pending)).toBe("protocol");

    const second = bridge.request({ method: "vaultStatus", token: TOKEN });
    expect(await rejectionCode(second)).toBe("protocol");
    expect(port.sent).toHaveLength(1);

    bridge.reset();
    const third = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    expect(port.sent).toHaveLength(2);
    port.receive(HANDSHAKE);
    await flush();
    port.receive({
      protocolVersion: 2,
      kind: "vaultStatus",
      tokenName: "extension",
      scopes: [],
      folderIds: [],
      expiresAt: null,
    });
    await third;
    expect(connects()).toBe(2);
  });
});

describe("native bridge serialisation", () => {
  test("nothing else is posted while one exchange is in flight", async () => {
    const { bridge, port } = harness();
    const first = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    port.receive(HANDSHAKE);
    await flush();

    const second = bridge.request({ method: "vaultSearch", token: TOKEN, query: "", limit: 5 });
    await flush();
    expect(port.sent).toHaveLength(2);

    port.receive({
      protocolVersion: 2,
      kind: "vaultStatus",
      tokenName: "extension",
      scopes: [],
      folderIds: [],
      expiresAt: null,
    });
    await first;
    await flush();
    expect(port.sent).toHaveLength(3);

    port.receive({ protocolVersion: 2, kind: "search", items: [] });
    const response = await second;
    expect(response.kind).toBe("search");
  });

  test("a failed exchange does not poison the next one", async () => {
    const { bridge, port } = harness();
    const first = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    port.receive(HANDSHAKE);
    await flush();
    port.receive({ protocolVersion: 2, kind: "error", code: "forbidden", message: "no" });
    const refused = await first;
    expect(bridgeErrorFromResponse(refused)).toMatchObject({ code: "denied" });

    const second = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    expect(port.sent).toHaveLength(3);
    port.receive({
      protocolVersion: 2,
      kind: "vaultStatus",
      tokenName: "extension",
      scopes: [],
      folderIds: [],
      expiresAt: null,
    });
    await second;
  });
});

describe("native bridge failures", () => {
  test("a missing host is reported as host_missing", async () => {
    const bridge = new NativeBridge({
      connectNative: () => {
        throw new Error("Specified native messaging host not found.");
      },
    });
    expect(await rejectionCode(bridge.request({ method: "vaultStatus", token: TOKEN }))).toBe(
      "host_missing",
    );
  });

  test("a closed port reports the stopped app and reconnects on the next call", async () => {
    const { bridge, port, connects } = harness();
    const pending = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    port.receive(HANDSHAKE);
    await flush();
    port.close();
    expect(await rejectionCode(pending)).toBe("daemon_unavailable");
    expect(bridge.connected).toBe(false);

    const second = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    port.receive(HANDSHAKE);
    await flush();
    port.receive({
      protocolVersion: 2,
      kind: "vaultStatus",
      tokenName: "extension",
      scopes: [],
      folderIds: [],
      expiresAt: null,
    });
    await second;
    expect(connects()).toBe(2);
  });

  test("a silent daemon times out", async () => {
    const { bridge, port, timers, cleared } = harness();
    const pending = bridge.request({ method: "vaultStatus", token: TOKEN });
    const settled = rejectionCode(pending);
    await flush();
    port.receive(HANDSHAKE);
    await flush();
    // Both the handshake and the request armed a timer; the last one is the
    // timer of the exchange that is still in flight.
    const last = timers[timers.length - 1];
    expect(last).toBeDefined();
    last?.();
    expect(await settled).toBe("timeout");
    expect(cleared.length).toBeGreaterThan(0);
  });

  test("an invalid frame is rejected as a protocol error", async () => {
    const { bridge, port } = harness();
    const pending = bridge.request({ method: "vaultStatus", token: TOKEN });
    await flush();
    port.receive("nope");
    expect(await rejectionCode(pending)).toBe("protocol");
  });
});

describe("daemon refusal mapping", () => {
  test("each code becomes a distinct bridge error", () => {
    expect(
      bridgeErrorFromResponse({
        protocolVersion: 2,
        kind: "handshake",
        vaultUnlocked: true,
        capabilities: [],
      }),
    ).toBeNull();
    expect(
      bridgeErrorFromResponse({
        protocolVersion: 2,
        kind: "error",
        code: "locked",
        message: "locked",
      }),
    ).toMatchObject({ code: "locked" });
    expect(
      bridgeErrorFromResponse({
        protocolVersion: 2,
        kind: "error",
        code: "invalid_credentials",
        message: "rejected",
      }),
    ).toMatchObject({ code: "not_paired" });
    expect(
      bridgeErrorFromResponse({
        protocolVersion: 2,
        kind: "error",
        code: "daemon_unavailable",
        message: "gone",
      }),
    ).toMatchObject({ code: "daemon_unavailable" });
    expect(
      bridgeErrorFromResponse({
        protocolVersion: 2,
        kind: "error",
        code: "internal",
        message: "boom",
      }),
    ).toMatchObject({ code: "protocol" });
  });
});
