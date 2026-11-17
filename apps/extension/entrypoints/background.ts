import { browser } from "wxt/browser";
import { defineBackground } from "wxt/utils/define-background";
import {
  BridgeError,
  NativeBridge,
  bridgeErrorFromResponse,
  type NativePortLike,
} from "../lib/native/bridge";
import { clearPairing, maskedToken, readPairing, writePairing } from "../lib/native/pairing";
import { handOff, passkeyReply, passkeyRequest } from "../lib/webauthn/ceremony";
import {
  isRuntimeRequest,
  type ContentReply,
  type ContentRequest,
  type CopyField,
  type ExtensionStatus,
  type RuntimeErrorCode,
  type RuntimeReply,
  type PasskeyRequest,
  type RuntimeRequest,
} from "../lib/messages";
import {
  isErrorResponse,
  type DaemonRequest,
  type DaemonResponse,
  type FillPayload,
  type FillTarget,
  type ItemSummary,
  type TokenStatus,
} from "../lib/protocol";

/**
 * Service worker: the only process that talks to the daemon.
 *
 * One Native Messaging port is opened on demand and kept while the worker
 * lives. Nothing here stores a value: a fill or a copy hands the daemon's
 * payload straight to the content script of the active tab and keeps only a
 * non-revealing report.
 */

const bridge = new NativeBridge({
  connectNative: (hostName) => browser.runtime.connectNative(hostName) as unknown as NativePortLike,
});

export default defineBackground(() => {
  browser.runtime.onInstalled.addListener(({ reason }) => {
    console.info(`[akooti] extension installed (${reason})`);
  });
  browser.runtime.onMessage.addListener((message: unknown) => {
    if (!isRuntimeRequest(message)) {
      return undefined;
    }
    return handle(message);
  });
});

async function handle(request: RuntimeRequest): Promise<RuntimeReply> {
  try {
    switch (request.type) {
      case "akooti:status":
        return await status();
      case "akooti:pair":
        return await pair(request.token, request.label ?? null);
      case "akooti:unpair":
        await clearPairing(browser.storage.local);
        bridge.reset();
        return { ok: true, kind: "unpaired" };
      case "akooti:search":
        return await search(request.query);
      case "akooti:fill":
        return await injectIntoActiveTab(request.itemId, "fill");
      case "akooti:copy":
        return await copyFromActiveTab(request.itemId, request.field);
      case "akooti:detect":
        return await inspectActiveTab("detect");
      case "akooti:capture":
        return await inspectActiveTab("capture");
      case "akooti:passkey":
        return await passkey(request.request);
      default:
        return failure("internal", "unknown request");
    }
  } catch (error) {
    return failureFromError(error);
  }
}

async function status(): Promise<RuntimeReply> {
  const pairing = await readPairing(browser.storage.local);
  if (!pairing) {
    return {
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
  }
  const base: ExtensionStatus = {
    paired: true,
    maskedToken: maskedToken(pairing.token),
    label: pairing.label,
    daemon: null,
    daemonError: null,
  };
  try {
    const answer = await bridge.request({
      method: "handshake",
      protocolVersion: 2,
      client: "native-host",
    });
    if (answer.kind === "handshake") {
      base.daemon = {
        vaultUnlocked: answer.vaultUnlocked,
        capabilities: answer.capabilities,
      };
    }
  } catch (error) {
    base.daemonError = messageOf(error);
  }
  return { ok: true, kind: "status", status: base };
}

async function pair(token: string, label: string | null): Promise<RuntimeReply> {
  const candidate = token.trim();
  // Verify before storing: a token that the daemon rejects must never be
  // persisted, otherwise the popup keeps showing a broken pairing.
  const status = await callDaemon({ method: "vaultStatus", token: candidate });
  if (status === null) {
    return failure("protocol", "the daemon did not answer the token check");
  }
  const stored = await writePairing(browser.storage.local, candidate, {
    label: stripeLabel(status.tokenName, label),
  });
  bridge.reset();
  return { ok: true, kind: "paired", label: stored.label };
}

async function search(query: string): Promise<RuntimeReply> {
  const response = await withToken((token) => ({
    method: "vaultSearch",
    token,
    query,
    limit: 25,
  }));
  if (response.kind !== "search") {
    return failure("protocol", "the daemon did not answer a search");
  }
  return { ok: true, kind: "search", items: response.items as ItemSummary[] };
}

async function injectIntoActiveTab(itemId: string, target: FillTarget): Promise<RuntimeReply> {
  const tab = await activeTab();
  if (!tab) {
    return failure("no_tab", "no active tab");
  }
  const origin = originOf(tab.url);
  if (!origin) {
    return failure("unsupported_page", "this page cannot receive a login form");
  }
  const payload = await releaseValues(itemId, origin, target);
  if (payload === null) {
    return failure("protocol", "the daemon released no value");
  }
  const reply = await sendToTab<ContentReply>(tab.id, {
    type: "akooti:inject",
    mode: "fill",
    payload,
  });
  if (!reply || reply.ok === false) {
    return failure("no_form", reply?.ok === false ? reply.message : "the page did not answer");
  }
  if (reply.kind !== "fill") {
    return failure("internal", "the page answered an unexpected report");
  }
  return { ok: true, kind: "fill", outcome: reply.report };
}

async function copyFromActiveTab(itemId: string, field: CopyField): Promise<RuntimeReply> {
  const tab = await activeTab();
  if (!tab) {
    return failure("no_tab", "no active tab");
  }
  const origin = originOf(tab.url);
  if (!origin) {
    return failure("unsupported_page", "this page cannot receive a value");
  }
  const payload = await releaseValues(itemId, origin, "clipboard");
  if (payload === null) {
    return failure("protocol", "the daemon released no value");
  }
  const reply = await sendToTab<ContentReply>(tab.id, {
    type: "akooti:inject",
    mode: "clipboard",
    field,
    payload,
  });
  if (!reply || reply.ok === false) {
    return failure("no_form", reply?.ok === false ? reply.message : "the page did not answer");
  }
  return { ok: true, kind: "copy", field };
}

async function inspectActiveTab(mode: "detect" | "capture"): Promise<RuntimeReply> {
  const tab = await activeTab();
  if (!tab) {
    return failure("no_tab", "no active tab");
  }
  const reply = await sendToTab<ContentReply>(tab.id, { type: "akooti:inspect", mode });
  if (!reply || reply.ok === false) {
    return failure("no_form", reply?.ok === false ? reply.message : "the page did not answer");
  }
  if (reply.kind === "detect") {
    return { ok: true, kind: "detect", detection: reply.detection, origin: reply.origin };
  }
  if (reply.kind === "capture") {
    return { ok: true, kind: "capture", report: reply.report };
  }
  return failure("internal", "the page answered an unexpected report");
}

/**
 * Serves one WebAuthn ceremony from the vault, or hands it to the browser.
 *
 * The vault-side authenticator only runs on a connection that declared
 * `client: "native-host"`, and this is that connection. Everything that can go
 * wrong here — no pairing, an unreachable daemon, a locked vault, a refusal, a
 * credential the vault does not hold — becomes `handled: false` with a reason,
 * never an error: a site that could register or sign in before installing
 * Akooti must still be able to.
 */
async function passkey(request: PasskeyRequest): Promise<RuntimeReply> {
  try {
    return passkeyReply(await withToken((token) => passkeyRequest(token, request)));
  } catch (error) {
    return handOff(error instanceof BridgeError ? error.message : messageOf(error));
  }
}

/** Releases the values for one origin, or reports why the daemon refused. */
async function releaseValues(
  itemId: string,
  origin: string,
  target: FillTarget,
): Promise<FillPayload | null> {
  const response = await withToken((token): DaemonRequest => ({
    method: "vaultExtensionFill",
    token,
    itemId,
    origin,
    selectors: null,
    target,
  }));
  if (response.kind !== "extensionFill") {
    return null;
  }
  return {
    origin: response.origin,
    selectors: response.selectors,
    username: response.username,
    password: response.password,
    totp: response.totp,
  };
}

/** Builds a request with the paired token and turns a refusal into an error. */
async function withToken(build: (token: string) => DaemonRequest): Promise<DaemonResponse> {
  const pairing = await readPairing(browser.storage.local);
  if (!pairing) {
    throw new BridgeError("not_paired", "pair the extension with a token first");
  }
  const response = await bridge.request(build(pairing.token));
  const refusal = bridgeErrorFromResponse(response);
  if (refusal) {
    throw refusal;
  }
  return response;
}

/** Token introspection that returns `null` instead of throwing. */
async function callDaemon(request: DaemonRequest): Promise<TokenStatus | null> {
  try {
    const response = await bridge.request(request);
    if (isErrorResponse(response) || response.kind !== "vaultStatus") {
      return null;
    }
    return {
      tokenName: response.tokenName,
      scopes: response.scopes,
      folderIds: response.folderIds,
      expiresAt: response.expiresAt,
    };
  } catch {
    return null;
  }
}

async function activeTab(): Promise<{ id: number; url: string | null } | null> {
  const tabs = await browser.tabs.query({ active: true, currentWindow: true });
  const tab = tabs[0];
  if (!tab || typeof tab.id !== "number") {
    return null;
  }
  return { id: tab.id, url: tab.url ?? null };
}

/** Origin of an http(s) page, or `null` for anything else. */
export function originOf(url: string | null | undefined): string | null {
  if (!url) {
    return null;
  }
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return null;
    }
    return parsed.origin;
  } catch {
    return null;
  }
}

async function sendToTab<T>(tabId: number, request: ContentRequest): Promise<T | null> {
  try {
    return (await browser.tabs.sendMessage(tabId, request)) as T;
  } catch {
    // A page opened before the extension was installed has no content script.
    return null;
  }
}

function stripeLabel(tokenName: string, label: string | null): string {
  return label?.trim() || tokenName;
}

function failure(code: RuntimeErrorCode, message: string): RuntimeReply {
  return { ok: false, kind: "error", code, message };
}

function failureFromError(error: unknown): RuntimeReply {
  if (error instanceof BridgeError) {
    return failure(error.code, error.message);
  }
  return failure("internal", messageOf(error));
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : "unexpected failure";
}
