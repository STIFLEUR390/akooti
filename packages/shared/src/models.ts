/**
 * Domain models shared by the desktop app, the WXT extension and the MCP
 * bridge.
 *
 * Security invariant: no model carries a plaintext secret. Secrets are only
 * returned by explicit, user-triggered IPC calls (`RevealedSecret`) and never
 * cross the MCP boundary (see `docs/regles.md` §1.1).
 */

export type VaultItemType = "login" | "api_key" | "note" | "env_set";

/** How an item URI is matched against the current page. */
export type UriMatchType = "domain" | "host" | "starts_with" | "exact" | "regex";

/** CSS/JS selectors memorised for blind autofill. */
export interface SelectorSet {
  username?: string;
  password?: string;
  totp?: string;
  submit?: string;
}

export interface Folder {
  id: string;
  parentId: string | null;
  name: string;
  /** Stable identifier, only set on system folders. */
  slug: string | null;
  icon: string;
  color: string | null;
  /** System folders cannot be renamed or deleted. */
  isSystem: boolean;
  requireBiometricForMcp: boolean;
  createdAt: number;
  updatedAt: number;
  deletedAt: number | null;
}

export interface ItemUri {
  id: string;
  /** Only set by the core; the desktop IPC omits it (the item is known). */
  itemId?: string;
  uri: string;
  matchType: UriMatchType;
}

export interface CustomField {
  id: string;
  /** Only set by the core; the desktop IPC omits it (the item is known). */
  itemId?: string;
  fieldName: string;
  isSecret: boolean;
  sortOrder: number;
}

export interface VaultItem {
  id: string;
  itemType: VaultItemType;
  title: string;
  primaryUrl: string | null;
  username: string | null;
  selectors: SelectorSet;
  /** zxcvbn score from 0 (weak) to 4 (strong). */
  zxcvbnScore: number | null;
  /**
   * First 5 characters of the SHA-1 hash, used for k-Anonymity checks.
   * Never sent to the desktop UI: it is a password-derived value.
   */
  passwordSha1Prefix?: string | null;
  isCompromised: boolean;
  passwordUpdatedAt: number | null;
  createdByAgent: string | null;
  /** TTL of ephemeral test accounts, in epoch seconds. */
  expiresAt: number | null;
  favorite: boolean;
  /** Many-to-many folder membership (never duplicated rows). */
  folderIds: string[];
  uris: ItemUri[];
  customFields: CustomField[];
  createdAt: number;
  updatedAt: number;
  deletedAt: number | null;
  /** Presence flags only: the UI never receives a ciphertext or a plaintext. */
  hasPassword?: boolean;
  hasTotp?: boolean;
  hasNotes?: boolean;
}

/** Item fields an explicit user action may reveal. */
export type SecretField = "password" | "totp" | "notes" | "custom_field";

/**
 * Plaintext secret returned only to the desktop UI after an explicit user
 * action. Never persisted, never serialised over MCP.
 */
export interface RevealedSecret {
  itemId: string;
  field: SecretField;
  value: string;
  /** Epoch milliseconds after which the UI must hide the value again. */
  expiresAt: number;
}

export type RoutingPatternType = "suffix" | "exact_host" | "cidr" | "regex";

export interface RoutingRule {
  id: string;
  pattern: string;
  patternType: RoutingPatternType;
  targetFolderId: string;
  isDefault: boolean;
  enabled: boolean;
  createdAt: number;
}

export interface HealthFinding {
  itemId: string;
  title: string;
  reason: "weak" | "reused" | "compromised" | "stale";
  zxcvbnScore: number | null;
  passwordUpdatedAt: number | null;
}

export interface HealthReport {
  generatedAt: number;
  totalItems: number;
  averageScore: number | null;
  weak: number;
  reused: number;
  compromised: number;
  stale: number;
  findings: HealthFinding[];
}

/** Granular permissions carried by a scoped MCP token. */
export type McpScope = "read_metadata" | "blind_fill" | "create_and_fill" | "exec_env";

export interface McpToken {
  id: string;
  name: string;
  /** Display prefix (`ak_mcp_8f9a…`); the full secret is never stored. */
  tokenPrefix: string;
  scopes: McpScope[];
  folderIds: string[];
  defaultCreateFolderId: string | null;
  expiresAt: number | null;
  revokedAt: number | null;
  lastUsedAt: number | null;
  createdAt: number;
}

export interface McpAuditEntry {
  id: string;
  tokenId: string | null;
  action:
    | "vault_search"
    | "blind_autofill"
    | "cdp_autofill"
    | "create_and_fill"
    | "exec_env"
    | "access_denied";
  targetItemId: string | null;
  targetFolderId: string | null;
  targetOrigin: string | null;
  status: "success" | "denied" | "error";
  details: string | null;
  createdAt: number;
}

/** The three folders created with every vault and protected from edits. */
export const SYSTEM_FOLDERS = [
  {
    id: "00000000-0000-4000-8000-000000000001",
    name: "Personnel",
    slug: "personal",
    icon: "user",
    color: "#3b82f6",
  },
  {
    id: "00000000-0000-4000-8000-000000000002",
    name: "Dev Local",
    slug: "dev-local",
    icon: "terminal",
    color: "#10b981",
  },
  {
    id: "00000000-0000-4000-8000-000000000003",
    name: "Agents & MCP",
    slug: "agents-mcp",
    icon: "bot",
    color: "#8b5cf6",
  },
] as const;

export type SystemFolderSlug = (typeof SYSTEM_FOLDERS)[number]["slug"];

/** Stable identifier of the Personal system folder. */
export const PERSONAL_FOLDER_ID = SYSTEM_FOLDERS[0].id;
/** Stable identifier of the Dev Local system folder. */
export const DEV_LOCAL_FOLDER_ID = SYSTEM_FOLDERS[1].id;
/** Stable identifier of the Agents & MCP system folder. */
export const AGENTS_MCP_FOLDER_ID = SYSTEM_FOLDERS[2].id;

export function isSystemFolder(folder: Pick<Folder, "isSystem">): boolean {
  return folder.isSystem;
}
