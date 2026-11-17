/**
 * Typed bridge to the Rust command surface.
 *
 * Every command name and payload here mirrors `src-tauri/src/commands/*` and
 * `src-tauri/src/dto.rs`. Nothing in this module invents data: a mismatch shows
 * up as a build error here rather than as a runtime surprise in the UI.
 *
 * Two rules (see `docs/regles.md` §1.1 and §3.2):
 *
 * 1. no read command returns a plaintext secret — only `revealSecret` and
 *    `totpCode`, both called from an explicit user gesture, do;
 * 2. a failure is a typed {@link IpcError} carrying the backend's stable
 *    `code`, so the UI can branch without parsing prose.
 */

import type {
  CustomField,
  Folder,
  RevealedSecret,
  RoutingPatternType,
  RoutingRule,
  SecretField,
  SelectorSet,
  UriMatchType,
  VaultItem,
  VaultItemType,
} from "@akooti/shared";

import { createMockApi } from "./mock";

/** Vault state reported by `vault_status`. */
export interface VaultStatus {
  status: "locked" | "unlocked";
  hasVault: boolean;
  autoLockMinutes: number;
}

/** Biometric capability of the running platform. */
export interface BiometricsStatus {
  available: boolean;
  provider: "touchId" | "windowsHello" | "fprintd";
  reason: string;
}

/** Footer build information. */
export interface BuildInfo {
  coreVersion: string;
  appVersion: string;
  platform: string;
}

/** Options for `generate_password`. */
export interface PasswordOptions {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

/** Options for `generate_passphrase`. */
export interface PassphraseOptions {
  words: number;
  separator: string;
  capitalize: boolean;
}

/** Capability a scoped MCP token may carry. */
export type McpScope = "read_metadata" | "blind_fill" | "create_and_fill" | "exec_env";

/** One MCP token as listed in the settings screen. Never carries the secret. */
export interface McpToken {
  id: string;
  name: string;
  tokenPrefix: string;
  scopes: McpScope[];
  folderIds: string[];
  defaultCreateFolderId: string | null;
  expiresAt: number | null;
  revokedAt: number | null;
  lastUsedAt: number | null;
  createdAt: number;
  active: boolean;
}

/** The one and only time a token secret is returned. */
export interface IssuedMcpToken {
  token: McpToken;
  secret: string;
}

/** Token creation payload. */
export interface McpTokenDraft {
  name: string;
  scopes: McpScope[];
  folderIds: string[];
  defaultCreateFolderId?: string | null;
  ttlDays?: number | null;
}

/** One row of the AI activity trail. Never carries a value. */
export interface McpAuditEntry {
  id: string;
  tokenId: string | null;
  action: string;
  targetItemId: string | null;
  targetFolderId: string | null;
  targetOrigin: string | null;
  status: "success" | "denied" | "error";
  details: string | null;
  createdAt: number;
}

/** State of the local daemon that headless entrypoints talk to. */
export interface DaemonStatus {
  running: boolean;
  socketPath: string | null;
  capabilities: string[];
}

/** A generated secret: shown once, never persisted. */
export interface GeneratedSecret {
  value: string;
  entropyBits: number;
}

/** Item filter for `list_items`. */
export interface ItemFilter {
  folderId?: string;
  includeDeleted?: boolean;
}

/** URI payload accepted when creating or updating an item. */
export interface ItemUriInput {
  uri: string;
  matchType?: UriMatchType;
}

/** Custom field payload accepted when creating an item. */
export interface CustomFieldInput {
  fieldName: string;
  value: string;
  isSecret: boolean;
  sortOrder: number;
}

/** Full item creation payload. */
export interface CreateItemInput {
  itemType: VaultItemType;
  title: string;
  primaryUrl?: string | null;
  username?: string | null;
  password?: string | null;
  totpSecret?: string | null;
  notes?: string | null;
  selectors: SelectorSet;
  favorite: boolean;
  expiresAt?: number | null;
  folderIds: string[];
  uris: ItemUriInput[];
  customFields: CustomFieldInput[];
}

/**
 * Metadata-only edit: secrets are written through {@link AkootiApi.setItemSecret}
 * so the UI never has to hold a password it did not type.
 */
export interface UpdateItemInput {
  id: string;
  itemType: VaultItemType;
  title: string;
  primaryUrl?: string | null;
  username?: string | null;
  selectors: SelectorSet;
  favorite: boolean;
  expiresAt?: number | null;
  folderIds: string[];
  uris: ItemUriInput[];
}

/** Folder creation payload. */
export interface FolderInput {
  name: string;
  parentId?: string | null;
  icon?: string | null;
  color?: string | null;
  requireBiometricForMcp: boolean;
}

/** New routing rule payload. */
export interface NewRoutingRuleInput {
  pattern: string;
  patternType: RoutingPatternType;
  targetFolderId: string;
  enabled: boolean;
}

/** One entry of the import wizard's format picker. */
export interface ImportFormat {
  id: string;
  label: string;
}

/** Duplicate candidate reported by the import preview. */
export interface DuplicateMatch {
  importedExternalId: string;
  existingItemId: string;
  reason: string;
  score: number;
}

/** Non-revealing issue reported after an import. */
export interface ImportIssue {
  kind: string;
  detail: string;
}

/** Read-only import preview. */
export interface ImportPreview {
  itemCount: number;
  folderCount: number;
  duplicates: DuplicateMatch[];
}

/** Import outcome. */
export interface ImportReport {
  imported: number;
  skipped: number;
  routed: number;
  issues: ImportIssue[];
  cleanup: string;
}

/** Options for the import wizard. */
export interface ImportOptions {
  routeDevLocal: boolean;
  importDuplicates: boolean;
  secureDeleteSource: boolean;
  password?: string | null;
}

/** Receipt of a secure clipboard write. */
export interface ClipboardReceipt {
  ttlSeconds: number;
}

/** The full command surface exposed to React. */
export interface AkootiApi {
  vaultStatus(): Promise<VaultStatus>;
  createVault(password: string): Promise<Folder[]>;
  unlockVault(password: string): Promise<Folder[]>;
  lockVault(): Promise<void>;
  biometricsStatus(): Promise<BiometricsStatus>;
  buildInfo(): Promise<BuildInfo>;

  listFolders(includeDeleted?: boolean): Promise<Folder[]>;
  createFolder(draft: FolderInput): Promise<Folder>;
  renameFolder(id: string, name: string): Promise<Folder>;
  deleteFolder(id: string): Promise<void>;
  addItemToFolder(itemId: string, folderId: string): Promise<void>;
  removeItemFromFolder(itemId: string, folderId: string): Promise<void>;

  listItems(filter?: ItemFilter): Promise<VaultItem[]>;
  getItem(id: string): Promise<VaultItem>;
  createItem(draft: CreateItemInput): Promise<VaultItem>;
  updateItem(draft: UpdateItemInput): Promise<VaultItem>;
  deleteItem(id: string): Promise<void>;
  setItemSecret(id: string, field: SecretField, value: string | null): Promise<void>;
  revealSecret(id: string, field: SecretField, customFieldId?: string): Promise<RevealedSecret>;
  totpCode(id: string): Promise<RevealedSecret>;
  addItemCustomField(itemId: string, field: CustomFieldInput): Promise<VaultItem>;
  removeItemCustomField(itemId: string, fieldId: string): Promise<void>;

  generatePassword(options: PasswordOptions): Promise<GeneratedSecret>;
  generatePassphrase(options: PassphraseOptions): Promise<GeneratedSecret>;

  listRoutingRules(): Promise<RoutingRule[]>;
  addRoutingRule(rule: NewRoutingRuleInput): Promise<RoutingRule>;
  setRoutingRuleEnabled(id: string, enabled: boolean): Promise<RoutingRule>;
  applyRoutingRules(): Promise<number>;

  listImportFormats(): Promise<ImportFormat[]>;
  pickImportFile(): Promise<string | null>;
  previewImportFile(path: string, format?: string, password?: string): Promise<ImportPreview>;
  importFile(
    path: string,
    format: string | undefined,
    options: ImportOptions,
  ): Promise<ImportReport>;

  copySecret(value: string): Promise<ClipboardReceipt>;

  listMcpTokens(): Promise<McpToken[]>;
  createMcpToken(draft: McpTokenDraft): Promise<IssuedMcpToken>;
  revokeMcpToken(id: string): Promise<McpToken>;
  deleteMcpToken(id: string): Promise<void>;
  listMcpAuditLog(limit?: number): Promise<McpAuditEntry[]>;
  daemonStatus(): Promise<DaemonStatus>;
}

/** Structured backend error: stable `code`, safe `message`. */
export class IpcError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }

  /** True when the vault locked itself mid-action. */
  get isLocked(): boolean {
    return this.code === "locked";
  }
}

/** Coerces any thrown value into an {@link IpcError}. */
export function toIpcError(error: unknown): IpcError {
  if (error instanceof IpcError) {
    return error;
  }
  if (typeof error === "string") {
    return new IpcError("internal", error);
  }
  // Only the backend envelope is trusted: a bare JS `Error` comes from the
  // webview itself and its message is implementation detail, not user copy.
  if (isRecord(error) && typeof error.code === "string") {
    const message = typeof error.message === "string" ? error.message : "Une erreur est survenue.";
    return new IpcError(error.code, message);
  }
  return new IpcError("internal", "Une erreur est survenue.");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

interface TauriInvoke {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

interface TauriGlobal {
  core?: TauriInvoke;
  invoke?: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
}

/** Reads the `window.__TAURI__` bridge installed by `withGlobalTauri`. */
function tauriBridge(): TauriInvoke | null {
  if (typeof window === "undefined") {
    return null;
  }
  const global = (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__;
  if (!global) {
    return null;
  }
  if (global.core) {
    return global.core;
  }
  if (typeof global.invoke === "function") {
    return { invoke: global.invoke };
  }
  return null;
}

/** True when the app runs inside the Tauri webview. */
export function isTauri(): boolean {
  return tauriBridge() !== null;
}

/**
 * Invokes a Rust command.
 *
 * Unwraps the `{ code, message }` envelope every command returns on failure so
 * callers only ever see an {@link IpcError}.
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const bridge = tauriBridge();
  if (!bridge) {
    throw new IpcError("no_bridge", "L'application desktop n'est pas disponible.");
  }
  try {
    return await bridge.invoke<T>(command, args);
  } catch (error) {
    throw toIpcError(error);
  }
}

/** The Tauri-backed implementation. */
export const tauriApi: AkootiApi = {
  vaultStatus: () => call("vault_status"),
  createVault: (password) => call("create_vault", { password }),
  unlockVault: (password) => call("unlock_vault", { password }),
  lockVault: () => call("lock_vault"),
  biometricsStatus: () => call("biometrics_status"),
  buildInfo: () => call("build_info"),

  listFolders: (includeDeleted = false) => call("list_folders", { includeDeleted }),
  createFolder: (draft) => call("create_folder", { draft }),
  renameFolder: (id, name) => call("rename_folder", { id, name }),
  deleteFolder: (id) => call("delete_folder", { id }),
  addItemToFolder: (itemId, folderId) => call("add_item_to_folder", { itemId, folderId }),
  removeItemFromFolder: (itemId, folderId) => call("remove_item_from_folder", { itemId, folderId }),

  listItems: (filter) => call("list_items", { filter: filter ?? null }),
  getItem: (id) => call("get_item", { id }),
  createItem: (draft) => call("create_item", { draft }),
  updateItem: (draft) => call("update_item", { draft }),
  deleteItem: (id) => call("delete_item", { id }),
  setItemSecret: (id, field, value) => call("set_item_secret", { id, field, value }),
  revealSecret: (id, field, customFieldId) =>
    call("reveal_secret", { id, field, customFieldId: customFieldId ?? null }),
  totpCode: (id) => call("totp_code", { id }),
  addItemCustomField: (itemId, field) => call("add_item_custom_field", { itemId, field }),
  removeItemCustomField: (itemId, fieldId) => call("remove_item_custom_field", { itemId, fieldId }),

  generatePassword: (options) => call("generate_password", { options }),
  generatePassphrase: (options) => call("generate_passphrase", { options }),

  listRoutingRules: () => call("list_routing_rules"),
  addRoutingRule: (rule) => call("add_routing_rule", { rule }),
  setRoutingRuleEnabled: (id, enabled) => call("set_routing_rule_enabled", { id, enabled }),
  applyRoutingRules: () => call("apply_routing_rules"),

  listImportFormats: () => call("list_import_formats"),
  pickImportFile: () => call("pick_import_file"),
  previewImportFile: (path, format, password) =>
    call("preview_import_file", { path, format: format ?? null, password: password ?? null }),
  importFile: (path, format, options) =>
    call("import_file", { path, format: format ?? null, options }),

  copySecret: (value) => call("copy_secret", { value }),

  listMcpTokens: () => call("list_mcp_tokens"),
  createMcpToken: (draft) => call("create_mcp_token", { draft }),
  revokeMcpToken: (id) => call("revoke_mcp_token", { id }),
  deleteMcpToken: (id) => call("delete_mcp_token", { id }),
  listMcpAuditLog: (limit) => call("list_mcp_audit_log", { limit: limit ?? null }),
  daemonStatus: () => call("daemon_status"),
};

/** Picks the real command surface or the mock. */
export function resolveApi(): AkootiApi {
  return isTauri() ? tauriApi : createMockApi();
}

/**
 * The API the UI talks to.
 *
 * Inside Tauri this is the real command surface. In a plain browser (`vite dev`
 * outside the shell, or a unit test) it is an in-memory mock, so the UI is
 * reviewable without compiling Rust. The mock is never reached in production:
 * the Tauri webview always installs `window.__TAURI__`.
 */
export const api: AkootiApi = resolveApi();

/** Re-exported so components do not import from `@akooti/shared` directly. */
export type { CustomField, Folder, RevealedSecret, RoutingRule, VaultItem };
