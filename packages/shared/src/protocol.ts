/**
 * Wire protocols shared by the desktop app, the extension and the Rust core.
 */

/** Native Messaging frames stdin/stdout with a 4-byte little-endian length. */
export const NATIVE_MESSAGE_HEADER_BYTES = 4;
export const NATIVE_MESSAGE_MAX_BYTES = 1024 * 1024;

export interface NativeMessageEnvelope<T = unknown> {
  id: string;
  type: "request" | "response" | "event";
  method: string;
  payload: T;
}

export function encodeNativeMessage(message: NativeMessageEnvelope): Uint8Array {
  const body = new TextEncoder().encode(JSON.stringify(message));
  if (body.byteLength > NATIVE_MESSAGE_MAX_BYTES) {
    throw new Error("native message exceeds the 1 MiB limit");
  }
  const frame = new Uint8Array(NATIVE_MESSAGE_HEADER_BYTES + body.byteLength);
  new DataView(frame.buffer).setUint32(0, body.byteLength, true);
  frame.set(body, NATIVE_MESSAGE_HEADER_BYTES);
  return frame;
}

export function decodeNativeMessage(frame: Uint8Array): NativeMessageEnvelope {
  if (frame.byteLength < NATIVE_MESSAGE_HEADER_BYTES) {
    throw new Error("native message frame is truncated");
  }
  const length = new DataView(frame.buffer, frame.byteOffset).getUint32(0, true);
  if (
    length > NATIVE_MESSAGE_MAX_BYTES ||
    length !== frame.byteLength - NATIVE_MESSAGE_HEADER_BYTES
  ) {
    throw new Error("native message length is inconsistent");
  }
  const body = new TextDecoder().decode(frame.subarray(NATIVE_MESSAGE_HEADER_BYTES));
  return JSON.parse(body) as NativeMessageEnvelope;
}

/** Local daemon IPC envelope (Unix socket / named pipe). */
export interface IpcRequest<T = unknown> {
  id: string;
  method: string;
  params: T;
}

export type IpcResponse<T = unknown> =
  | { id: string; ok: true; result: T }
  | { id: string; ok: false; error: { code: string; message: string } };

/** Methods exposed by the daemon to the `mcp` and `native-host` subprocesses. */
export const IPC_METHODS = {
  handshake: "daemon.handshake",
  search: "vault.search",
  blindAutofill: "vault.blindAutofill",
  cdpAutofill: "vault.cdpAutofill",
  createAndFill: "vault.createAndFill",
  exec: "vault.exec",
  totp: "vault.totp",
} as const;

export type IpcMethod = (typeof IPC_METHODS)[keyof typeof IPC_METHODS];

/** Tool names exposed by `akooti mcp`. Metadata only, never a secret. */
export const MCP_TOOLS = {
  search: "vault_search",
  blindAutofill: "vault_blind_autofill",
  cdpAutofill: "vault_cdp_autofill",
  createAndFill: "vault_create_and_fill",
  exec: "vault_exec",
} as const;

export type McpToolName = (typeof MCP_TOOLS)[keyof typeof MCP_TOOLS];
