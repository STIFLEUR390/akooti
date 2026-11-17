/**
 * Client side of the internal message bus (popup / content script → worker).
 *
 * Every caller gets a discriminated {@link RuntimeReply}: a worker that crashed,
 * a page that answered nothing and a reply of an unexpected shape all become the
 * same `ok: false` value, so a UI never has to wrap the call in `try`.
 *
 * The transport is injected: the popup passes `browser.runtime.sendMessage`, the
 * passkey relay passes the same function, and the tests pass a double.
 */
import {
  RUNTIME_ERROR_CODES,
  type RuntimeErrorCode,
  type RuntimeReply,
  type RuntimeRequest,
} from "./messages";

/** Sends one message to the service worker; the transport is injected. */
export type RuntimeSend = (message: RuntimeRequest) => Promise<unknown>;

/** Sends one request and normalises every failure into a reply. */
export async function callRuntime(
  send: RuntimeSend,
  request: RuntimeRequest,
): Promise<RuntimeReply> {
  let answer: unknown;
  try {
    answer = await send(request);
  } catch (error) {
    // A reloaded or updated extension leaves this page with no worker to talk
    // to: the browser throws instead of answering. That is not the desktop app
    // being closed, so it must not be reported as `daemon_unavailable`.
    if (isContextLost(error)) {
      return {
        ok: false,
        kind: "error",
        code: "context_lost",
        message: "the extension was reloaded; reopen this page",
      };
    }
    return {
      ok: false,
      kind: "error",
      code: "daemon_unavailable",
      message: messageOf(error, "the extension could not reach its service worker"),
    };
  }
  return asRuntimeReply(answer);
}

/**
 * Whether a transport failure means the extension context itself is gone.
 *
 * The wording belongs to the browser, not to us: Chrome says "Extension context
 * invalidated", Firefox reports a missing receiving end, and a page that is not
 * an extension at all has no `runtime` to call. Matching on those strings is
 * best effort by nature, so the fallback stays a plain transport failure.
 */
export function isContextLost(error: unknown): boolean {
  const message = error instanceof Error ? error.message : "";
  return /context invalidated|receiving end does not exist|message port closed|reading 'sendMessage'/i.test(
    message,
  );
}

/**
 * Narrows an untrusted value to a {@link RuntimeReply}.
 *
 * The worker is our own code, but a reloaded extension can leave an orphan
 * content script talking to a worker that no longer answers this message;
 * a malformed answer is therefore a normal outcome, not a panic.
 */
export function asRuntimeReply(value: unknown): RuntimeReply {
  if (typeof value !== "object" || value === null) {
    return internalFailure("the service worker answered nothing");
  }
  const candidate = value as { ok?: unknown; kind?: unknown; code?: unknown; message?: unknown };
  if (candidate.ok === false) {
    if (
      candidate.kind === "error" &&
      isErrorCode(candidate.code) &&
      typeof candidate.message === "string"
    ) {
      return value as RuntimeReply;
    }
    return internalFailure("the service worker answered an invalid error");
  }
  if (candidate.ok !== true || typeof candidate.kind !== "string") {
    return internalFailure("the service worker answered an unknown reply");
  }
  return value as RuntimeReply;
}

/** Whether a value is one of the stable failure codes. */
export function isErrorCode(value: unknown): value is RuntimeErrorCode {
  return typeof value === "string" && (RUNTIME_ERROR_CODES as readonly string[]).includes(value);
}

/** Reply used when the bus itself failed. */
export function internalFailure(message: string): RuntimeReply {
  return { ok: false, kind: "error", code: "internal", message };
}

function messageOf(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.length > 0) {
    return error.message;
  }
  return fallback;
}
