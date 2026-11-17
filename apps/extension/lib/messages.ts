/**
 * Messages exchanged inside the extension (popup ↔ service worker ↔ content
 * script) and their replies.
 *
 * Only the service worker talks to the daemon: the popup asks for metadata, the
 * content script receives values it must inject. A value never travels back up
 * towards the popup, which is why no reply shape in here carries one.
 */
import type { FormDetection } from "./dom/detect";
import type { FillPayload, ItemSummary } from "./protocol";

/** Requests the popup or a content script may send. */
export type RuntimeRequest =
  | { type: "akooti:status" }
  | { type: "akooti:pair"; token: string; label?: string | null }
  | { type: "akooti:unpair" }
  | { type: "akooti:search"; query: string }
  | { type: "akooti:fill"; itemId: string }
  | { type: "akooti:copy"; itemId: string; field: CopyField }
  | { type: "akooti:detect" }
  | { type: "akooti:capture" }
  | { type: "akooti:passkey"; kind: "create" | "get"; rpId: string | null };

/** Field the user asked to copy. */
export type CopyField = "username" | "password" | "totp";

/** Stable failure codes shown by the popup. */
export const RUNTIME_ERROR_CODES = [
  "not_paired",
  "locked",
  "daemon_unavailable",
  "host_missing",
  "protocol",
  "timeout",
  "denied",
  "no_tab",
  "unsupported_page",
  "no_form",
  "internal",
] as const;

/** One failure code, as the popup receives it. */
export type RuntimeErrorCode = (typeof RUNTIME_ERROR_CODES)[number];

/** Successful replies, discriminated by `kind`; the failure arm carries `"error"`. */
export type RuntimeReply =
  | { ok: true; kind: "status"; status: ExtensionStatus }
  | { ok: true; kind: "paired"; label: string | null }
  | { ok: true; kind: "unpaired" }
  | { ok: true; kind: "search"; items: ItemSummary[] }
  | { ok: true; kind: "fill"; outcome: FillReport }
  | { ok: true; kind: "copy"; field: CopyField }
  | { ok: true; kind: "detect"; detection: FormDetection; origin: string }
  | { ok: true; kind: "capture"; report: CaptureReport }
  | {
      ok: true;
      kind: "passkey";
      handled: boolean;
      reason: string | null;
      credential: Record<string, unknown> | null;
    }
  | { ok: false; kind: "error"; code: RuntimeErrorCode; message: string };

/** Everything the popup shows about the pairing and the daemon. */
export interface ExtensionStatus {
  paired: boolean;
  maskedToken: string | null;
  label: string | null;
  /** Handshake answered by the daemon, when it is reachable. */
  daemon: { vaultUnlocked: boolean; capabilities: string[] } | null;
  /** Reason the daemon is unreachable, when it is. */
  daemonError: string | null;
}

/** Non-revealing result of a fill. */
export interface FillReport {
  /** Field roles filled (`username`, `password`, `totp`). */
  fieldsFilled: string[];
  submitted: boolean;
  /** What the form looked like before the fill. */
  detected: FormDetection["kind"];
}

/** Non-revealing result of a selector capture. */
export interface CaptureReport {
  /** Ready-to-paste text for the desktop item editor. */
  text: string;
  detected: FormDetection["kind"];
}

/** Whether a message is one this extension understands. */
export function isRuntimeRequest(value: unknown): value is RuntimeRequest {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { type?: unknown };
  return typeof candidate.type === "string" && candidate.type.startsWith("akooti:");
}

/** Reply used when the worker itself failed. */
export function internalError(message: string): RuntimeReply {
  return { ok: false, kind: "error", code: "internal", message };
}

/**
 * Messages the service worker sends to the content script of the active tab.
 *
 * These carry values from the daemon to the page, in one direction only: no
 * reply shape below can echo a value back.
 */
export type ContentRequest =
  | { type: "akooti:inject"; mode: "fill"; payload: FillPayload }
  | {
      type: "akooti:inject";
      mode: "clipboard";
      field: CopyField;
      payload: FillPayload;
    }
  | { type: "akooti:inspect"; mode: "detect" }
  | { type: "akooti:inspect"; mode: "capture" };

/** What the content script answers. */
export type ContentReply =
  | { ok: true; kind: "fill"; report: FillReport }
  | { ok: true; kind: "clipboard"; field: CopyField }
  | { ok: true; kind: "detect"; detection: FormDetection; origin: string }
  | { ok: true; kind: "capture"; report: CaptureReport }
  | { ok: false; code: "no_form" | "unsupported_page" | "internal"; message: string };

/** Whether a message is a command for a content script. */
export function isContentRequest(value: unknown): value is ContentRequest {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { type?: unknown };
  return candidate.type === "akooti:inject" || candidate.type === "akooti:inspect";
}
