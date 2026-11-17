/**
 * Wire contract between the extension and `akooti native-host`.
 *
 * The native host is a frame pump to the daemon socket, so the extension speaks
 * the daemon protocol itself (`crates/akooti-ipc/src/protocol.rs`): one JSON
 * document per message, `camelCase` everywhere, `method` as the request tag and
 * `kind` as the response tag. `PROTOCOL_VERSION` is the daemon's constant; the
 * golden frames in `protocol.test.ts` pin the shape on both sides.
 */

/** Must equal `akooti_ipc::PROTOCOL_VERSION`. */
export const PROTOCOL_VERSION = 2;

/** Native Messaging host name declared in the manifest. */
export const HOST_NAME = "org.akooti.host";

/** Selectors for one form. `null` means "let the extension find it". */
export interface SelectorSet {
  username: string | null;
  password: string | null;
  totp: string | null;
  submit: string | null;
}

export const EMPTY_SELECTORS: SelectorSet = {
  username: null,
  password: null,
  totp: null,
  submit: null,
};

/** Metadata-only item view returned by `vaultSearch`. */
export interface ItemSummary {
  id: string;
  title: string;
  itemType: string;
  primaryUrl: string | null;
  username: string | null;
  folderIds: string[];
  hasTotp: boolean;
  expiresAt: number | null;
}

/** Introspection of the paired token. */
export interface TokenStatus {
  tokenName: string;
  scopes: string[];
  folderIds: string[];
  expiresAt: number | null;
}

/** What the user asked the extension to do with the released values. */
export type FillTarget = "fill" | "clipboard";

/** Requests the extension may send. An MCP agent never sends the last one. */
export type DaemonRequest =
  | { method: "handshake"; protocolVersion: number; client: string }
  | { method: "vaultStatus"; token: string }
  | { method: "vaultSearch"; token: string; query: string; limit: number }
  | {
      method: "vaultExtensionFill";
      token: string;
      itemId: string;
      origin: string;
      selectors: SelectorSet | null;
      target: FillTarget;
    };

/**
 * Values released for one origin.
 *
 * The extension is the only consumer: the daemon authenticates the token,
 * checks the folder allowlist and refuses an origin the item does not claim.
 * Values are never logged, never stored and only reach the page or the
 * clipboard.
 */
export interface FillPayload {
  origin: string;
  selectors: SelectorSet;
  username: string | null;
  password: string | null;
  totp: string | null;
}

/** Responses the daemon may send. */
export type DaemonResponse =
  | {
      protocolVersion: number;
      kind: "handshake";
      vaultUnlocked: boolean;
      capabilities: string[];
    }
  | ({
      protocolVersion: number;
      kind: "vaultStatus";
    } & TokenStatus)
  | { protocolVersion: number; kind: "search"; items: ItemSummary[] }
  | ({ protocolVersion: number; kind: "extensionFill" } & FillPayload)
  | { protocolVersion: number; kind: "error"; code: string; message: string };

/** Stable error codes the daemon uses (see `docs/architecture.md` §5). */
export type DaemonErrorCode =
  | "forbidden"
  | "locked"
  | "not_found"
  | "invalid_credentials"
  | "invalid_input"
  | "internal"
  | "daemon_unavailable"
  | "version_mismatch"
  | "protocol";

/** Builds the handshake the bridge sends before anything else. */
export function handshakeRequest(): DaemonRequest {
  return { method: "handshake", protocolVersion: PROTOCOL_VERSION, client: "native-host" };
}

/** Narrows a decoded response to its error arm. */
export function isErrorResponse(
  response: DaemonResponse,
): response is Extract<DaemonResponse, { kind: "error" }> {
  return response.kind === "error";
}
