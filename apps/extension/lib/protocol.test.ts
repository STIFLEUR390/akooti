import { describe, expect, test } from "bun:test";

import {
  HOST_NAME,
  PROTOCOL_VERSION,
  handshakeRequest,
  isErrorResponse,
  type DaemonRequest,
  type DaemonResponse,
} from "./protocol";

/**
 * Golden frames.
 *
 * The Rust side pins the same documents in
 * `crates/akooti-ipc/src/protocol.rs`; if either side renames a field, one of
 * the two suites fails before a browser and a daemon disagree at runtime.
 */

describe("protocol constants", () => {
  test("match the daemon", () => {
    expect(PROTOCOL_VERSION).toBe(2);
    expect(HOST_NAME).toBe("org.akooti.host");
  });

  test("the handshake names the native host as the client", () => {
    expect(handshakeRequest()).toEqual({
      method: "handshake",
      protocolVersion: 2,
      client: "native-host",
    });
  });
});

describe("vaultExtensionFill request", () => {
  test("serialises exactly like the Rust golden frame", () => {
    const request: DaemonRequest = {
      method: "vaultExtensionFill",
      token: "ak_mcp_probe",
      itemId: "item-1",
      origin: "https://app.example.test",
      selectors: { username: null, password: "#pass", totp: null, submit: null },
      target: "fill",
    };
    expect(JSON.parse(JSON.stringify(request))).toEqual({
      method: "vaultExtensionFill",
      token: "ak_mcp_probe",
      itemId: "item-1",
      origin: "https://app.example.test",
      selectors: { username: null, password: "#pass", totp: null, submit: null },
      target: "fill",
    });
  });

  test("accepts null selectors, which ask the extension to detect the form", () => {
    const request: DaemonRequest = {
      method: "vaultExtensionFill",
      token: "ak_mcp_probe",
      itemId: "item-1",
      origin: "https://app.example.test",
      selectors: null,
      target: "clipboard",
    };
    expect(JSON.parse(JSON.stringify(request)).selectors).toBeNull();
  });
});

describe("extensionFill response", () => {
  test("carries the values flat, next to the envelope", () => {
    const frame = {
      protocolVersion: 2,
      kind: "extensionFill",
      origin: "https://app.example.test",
      selectors: { username: "#email", password: null, totp: null, submit: null },
      username: "agent@example.test",
      password: "s3cret-value",
      totp: null,
    };
    const response = JSON.parse(JSON.stringify(frame)) as DaemonResponse;
    expect(isErrorResponse(response)).toBe(false);
    if (response.kind !== "extensionFill") {
      throw new Error("the frame should decode as an extension fill");
    }
    expect(response.protocolVersion).toBe(PROTOCOL_VERSION);
    expect(response.password).toBe("s3cret-value");
    expect(response.totp).toBeNull();
  });

  test("an error frame is recognised by its kind", () => {
    const response: DaemonResponse = {
      protocolVersion: 2,
      kind: "error",
      code: "forbidden",
      message: "the token may not read this folder",
    };
    expect(isErrorResponse(response)).toBe(true);
  });
});
