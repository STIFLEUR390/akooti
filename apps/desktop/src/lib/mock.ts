/**
 * In-memory implementation of {@link AkootiApi}.
 *
 * Purpose: review and test the whole UI without compiling Rust (`vite dev` in a
 * browser, `bun test`). It is loaded only when `window.__TAURI__` is absent, so
 * a packaged build never reaches it.
 *
 * Security note: every value here is fabricated. The "password" is a fixed
 * string, the TOTP codes are a deterministic hash of the time step, and nothing
 * is written anywhere. This is a UI fixture, not a vault.
 */

import { AGENTS_MCP_FOLDER_ID, DEV_LOCAL_FOLDER_ID, PERSONAL_FOLDER_ID } from "@akooti/shared";
import type {
  Folder,
  RevealedSecret,
  RoutingRule,
  VaultItem,
  VaultItemType,
  SelectorSet,
} from "@akooti/shared";

import { IpcError } from "./ipc";
import { estimatePassword } from "./strength";
import type {
  AkootiApi,
  BackupRun,
  BackupTarget,
  BreachReport,
  BuildInfo,
  ClipboardReceipt,
  CreateItemInput,
  ExportReceipt,
  GeneratedSecret,
  HealthFinding,
  HealthReport,
  ImportFormat,
  ImportOptions,
  ImportPreview,
  ImportReport,
  ItemFilter,
  McpAuditEntry,
  McpScope,
  McpToken,
  McpTokenDraft,
  PassphraseOptions,
  PasswordOptions,
  SyncRun,
  SyncStatus,
  UpdateItemInput,
  VaultStatus,
} from "./ipc";

const PERSONAL = PERSONAL_FOLDER_ID;
const DEV_LOCAL = DEV_LOCAL_FOLDER_ID;
const AGENTS = AGENTS_MCP_FOLDER_ID;

const MASTER_PASSWORD = "akooti-demo-2026";

/** Seconds a revealed secret stays visible, mirroring the Rust default. */
const SECRET_TTL_SECONDS = 30;

interface MockItem extends VaultItem {
  password: string | null;
  totpSecret: string | null;
  notes: string | null;
  customValues: Record<string, string>;
}

interface MockState {
  hasVault: boolean;
  unlocked: boolean;
  folders: Folder[];
  items: MockItem[];
  rules: RoutingRule[];
  routingEnabled: boolean;
  tokens: McpToken[];
  audit: McpAuditEntry[];
  sync: SyncStatus;
  backupTargets: BackupTarget[];
  backupRuns: BackupRun[];
}

function now(): number {
  return Math.floor(Date.now() / 1000);
}

function newId(): string {
  return crypto.randomUUID();
}

function systemFolder(id: string, name: string, slug: string, icon: string, color: string): Folder {
  const stamp = now();
  return {
    id,
    parentId: null,
    name,
    slug,
    icon,
    color,
    isSystem: true,
    requireBiometricForMcp: false,
    createdAt: stamp,
    updatedAt: stamp,
    deletedAt: null,
  };
}

function baseFolders(): Folder[] {
  return [
    systemFolder(PERSONAL, "Personnel", "personal", "user", "#3b82f6"),
    systemFolder(DEV_LOCAL, "Dev Local", "dev-local", "terminal", "#10b981"),
    systemFolder(AGENTS, "Agents & MCP", "agents-mcp", "bot", "#8b5cf6"),
  ];
}

const EMPTY_SELECTORS: SelectorSet = {};

function makeItem(
  itemType: VaultItemType,
  title: string,
  overrides: Partial<MockItem> = {},
): MockItem {
  const stamp = now();
  return {
    id: newId(),
    itemType,
    title,
    primaryUrl: null,
    username: null,
    selectors: EMPTY_SELECTORS,
    zxcvbnScore: null,
    isCompromised: false,
    passwordUpdatedAt: null,
    createdByAgent: null,
    expiresAt: null,
    favorite: false,
    folderIds: [PERSONAL],
    uris: [],
    customFields: [],
    createdAt: stamp,
    updatedAt: stamp,
    deletedAt: null,
    password: null,
    totpSecret: null,
    notes: null,
    customValues: {},
    ...overrides,
  };
}

function seedItems(): MockItem[] {
  return [
    makeItem("login", "Blog personnel", {
      primaryUrl: "https://example.test",
      username: "herold@example.test",
      password: "correct-horse-battery",
      passwordUpdatedAt: now(),
      zxcvbnScore: 3,
      uris: [{ id: newId(), uri: "https://example.test", matchType: "domain" }],
      selectors: { username: "#email", password: "#password", submit: "button[type=submit]" },
    }),
    makeItem("login", "API locale", {
      primaryUrl: "http://localhost:8080",
      username: "admin",
      password: "dev-only-password",
      passwordUpdatedAt: now(),
      zxcvbnScore: 2,
      folderIds: [DEV_LOCAL],
      uris: [{ id: newId(), uri: "http://localhost:8080", matchType: "host" }],
      totpSecret: "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
    }),
    makeItem("api_key", "Jeton de staging", {
      username: "staging",
      notes: "Rotation tous les 90 jours.",
      favorite: true,
      folderIds: [PERSONAL, AGENTS],
      customFields: [],
    }),
  ];
}

/** Deterministic 6-digit code that changes every 30 seconds. */
function fakeTotp(seed: string, at = Date.now()): string {
  const step = Math.floor(at / 30_000);
  let hash = 0x811c9dc5;
  for (const character of `${seed}:${step}`) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return String(hash % 1_000_000).padStart(6, "0");
}

/** Entropy estimate used by the mock generator, in bits. */
function entropyBits(length: number, alphabet: number): number {
  return Math.round(length * Math.log2(alphabet) * 10) / 10;
}

function randomString(length: number, alphabet: string): string {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  let value = "";
  for (const byte of bytes) {
    value += alphabet[byte % alphabet.length] ?? alphabet[0] ?? "x";
  }
  return value;
}

const WORDS = [
  "atlas",
  "boussole",
  "cabane",
  "delta",
  "eclipse",
  "falaise",
  "givre",
  "horizon",
  "ilot",
  "jungle",
  "koala",
  "lagon",
  "meteorite",
  "nuage",
  "orage",
  "prairie",
  "quasar",
  "riviere",
  "sable",
  "tulipe",
  "uranium",
  "vallee",
  "wagon",
  "xenon",
  "yacht",
  "zephyr",
];

const IMPORT_FORMATS: ImportFormat[] = [
  { id: "auto", label: "Détection automatique" },
  { id: "bitwarden", label: "Bitwarden / Vaultwarden (.json)" },
  { id: "protonpass", label: "Proton Pass (.json, .zip)" },
  { id: "keepass", label: "KeePass / KeePassXC (.kdbx)" },
  { id: "keepass_xml", label: "KeePass XML (.xml)" },
  { id: "keepass_csv", label: "KeePass CSV (.csv)" },
  { id: "onepassword", label: "1Password (.1pux)" },
  { id: "onepassword_csv", label: "1Password CSV (.csv)" },
  { id: "buttercup", label: "Buttercup (.bcup)" },
  { id: "buttercup_csv", label: "Buttercup CSV (.csv)" },
  { id: "browser_csv", label: "Navigateur (.csv)" },
];

/** Creates an isolated mock backend. */
export function createMockApi(): AkootiApi {
  const state: MockState = {
    hasVault: false,
    unlocked: false,
    folders: baseFolders(),
    items: [],
    rules: [],
    routingEnabled: true,
    tokens: [],
    audit: [],
    sync: {
      enabled: false,
      connected: false,
      supabaseUrl: null,
      userId: null,
      cursor: null,
      lastSyncAt: null,
      lastStatus: null,
      lastError: null,
    },
    backupTargets: [],
    backupRuns: [],
  };

  function requireUnlocked(): void {
    if (!state.unlocked) {
      throw new IpcError("locked", "Le coffre est verrouillé.");
    }
  }

  function findItem(id: string): MockItem {
    const item = state.items.find((candidate) => candidate.id === id);
    if (!item) {
      throw new IpcError("not_found", "Cette entrée n'existe plus.");
    }
    return item;
  }

  function reveal(item: MockItem, field: string, customFieldId?: string): RevealedSecret {
    const expiresAt = Date.now() + SECRET_TTL_SECONDS * 1_000;
    const stamp = now();
    if (field === "password") {
      if (!item.password) {
        throw new IpcError("not_found", "Aucun mot de passe enregistré.");
      }
      return { itemId: item.id, field: "password", value: item.password, expiresAt };
    }
    if (field === "notes") {
      if (!item.notes) {
        throw new IpcError("not_found", "Aucune note enregistrée.");
      }
      return { itemId: item.id, field: "notes", value: item.notes, expiresAt };
    }
    if (field === "totp") {
      if (!item.totpSecret) {
        throw new IpcError("not_found", "Aucun secret TOTP enregistré.");
      }
      return {
        itemId: item.id,
        field: "totp",
        value: fakeTotp(item.totpSecret, stamp * 1_000),
        expiresAt: (stamp + SECRET_TTL_SECONDS) * 1_000,
      };
    }
    const value = customFieldId ? item.customValues[customFieldId] : undefined;
    if (!customFieldId || value === undefined) {
      throw new IpcError("not_found", "Ce champ n'est plus disponible.");
    }
    return { itemId: item.id, field: "custom_field", value, expiresAt };
  }

  function strip(item: MockItem): VaultItem {
    const { password, totpSecret, notes, customValues: _values, ...rest } = item;
    return {
      ...rest,
      hasPassword: password !== null,
      hasTotp: totpSecret !== null,
      hasNotes: notes !== null,
    };
  }

  return {
    async vaultStatus(): Promise<VaultStatus> {
      return {
        status: state.unlocked ? "unlocked" : "locked",
        hasVault: state.hasVault,
        autoLockMinutes: 15,
      };
    },

    async createVault(password: string): Promise<Folder[]> {
      if (password.length < 12) {
        throw new IpcError(
          "invalid_input",
          "Le mot de passe maître doit contenir au moins 12 caractères.",
        );
      }
      if (state.hasVault) {
        throw new IpcError("vault_exists", "Un coffre existe déjà sur cet appareil.");
      }
      state.hasVault = true;
      state.unlocked = true;
      state.items = seedItems();
      state.rules = defaultRules();
      return state.folders;
    },

    async unlockVault(password: string): Promise<Folder[]> {
      if (!state.hasVault) {
        throw new IpcError("no_vault", "Aucun coffre n'a été trouvé sur cet appareil.");
      }
      if (password !== MASTER_PASSWORD) {
        throw new IpcError("invalid_credentials", "Mot de passe maître incorrect.");
      }
      state.unlocked = true;
      return state.folders;
    },

    async lockVault(): Promise<void> {
      state.unlocked = false;
    },

    async biometricsStatus() {
      return {
        available: false,
        provider: "fprintd" as const,
        reason:
          "Aucun lecteur d'empreintes branché : utilisez le mot de passe maître (voir ADR 0004).",
      };
    },

    async buildInfo(): Promise<BuildInfo> {
      return {
        coreVersion: "0.1.0-mock",
        appVersion: "0.1.0",
        platform: "browser",
      };
    },

    async listFolders(): Promise<Folder[]> {
      requireUnlocked();
      return state.folders;
    },

    async createFolder(draft): Promise<Folder> {
      requireUnlocked();
      const stamp = now();
      const folder: Folder = {
        id: newId(),
        parentId: draft.parentId ?? null,
        name: draft.name,
        slug: null,
        icon: draft.icon ?? "folder",
        color: draft.color ?? null,
        isSystem: false,
        requireBiometricForMcp: draft.requireBiometricForMcp,
        createdAt: stamp,
        updatedAt: stamp,
        deletedAt: null,
      };
      state.folders = [...state.folders, folder];
      return folder;
    },

    async renameFolder(id, name): Promise<Folder> {
      requireUnlocked();
      const folder = state.folders.find((candidate) => candidate.id === id);
      if (!folder) {
        throw new IpcError("not_found", "Ce dossier n'existe plus.");
      }
      if (folder.isSystem) {
        throw new IpcError(
          "system_folder_protected",
          "Les dossiers système ne peuvent pas être renommés.",
        );
      }
      folder.name = name;
      folder.updatedAt = now();
      return folder;
    },

    async deleteFolder(id): Promise<void> {
      requireUnlocked();
      const folder = state.folders.find((candidate) => candidate.id === id);
      if (!folder) {
        throw new IpcError("not_found", "Ce dossier n'existe plus.");
      }
      if (folder.isSystem) {
        throw new IpcError(
          "system_folder_protected",
          "Les dossiers système ne peuvent pas être supprimés.",
        );
      }
      state.folders = state.folders.filter((candidate) => candidate.id !== id);
    },

    async addItemToFolder(itemId, folderId): Promise<void> {
      requireUnlocked();
      const item = findItem(itemId);
      if (!item.folderIds.includes(folderId)) {
        item.folderIds = [...item.folderIds, folderId];
      }
    },

    async removeItemFromFolder(itemId, folderId): Promise<void> {
      requireUnlocked();
      const item = findItem(itemId);
      item.folderIds = item.folderIds.filter((id) => id !== folderId);
      if (item.folderIds.length === 0) {
        item.folderIds = [PERSONAL];
      }
    },

    async listItems(filter?: ItemFilter): Promise<VaultItem[]> {
      requireUnlocked();
      const folderId = filter?.folderId;
      return state.items
        .filter((item) => (folderId ? item.folderIds.includes(folderId) : true))
        .map(strip);
    },

    async getItem(id): Promise<VaultItem> {
      requireUnlocked();
      return strip(findItem(id));
    },

    async createItem(draft: CreateItemInput): Promise<VaultItem> {
      requireUnlocked();
      const customFields = draft.customFields.map((field) => ({
        id: newId(),
        fieldName: field.fieldName,
        isSecret: field.isSecret,
        sortOrder: field.sortOrder,
      }));
      const item = makeItem(draft.itemType, draft.title, {
        primaryUrl: draft.primaryUrl ?? null,
        username: draft.username ?? null,
        selectors: draft.selectors,
        favorite: draft.favorite,
        expiresAt: draft.expiresAt ?? null,
        folderIds: draft.folderIds.length > 0 ? draft.folderIds : [PERSONAL],
        uris: draft.uris.map((uri) => ({
          id: newId(),
          uri: uri.uri,
          matchType: uri.matchType ?? "domain",
        })),
        customFields,
        password: draft.password ?? null,
        totpSecret: draft.totpSecret ?? null,
        notes: draft.notes ?? null,
        passwordUpdatedAt: draft.password ? now() : null,
        customValues: Object.fromEntries(
          customFields.map((field, index) => [field.id, draft.customFields[index]?.value ?? ""]),
        ),
      });
      state.items = [...state.items, item];
      return strip(item);
    },

    async updateItem(draft: UpdateItemInput): Promise<VaultItem> {
      requireUnlocked();
      const item = findItem(draft.id);
      item.itemType = draft.itemType;
      item.title = draft.title;
      item.primaryUrl = draft.primaryUrl ?? null;
      item.username = draft.username ?? null;
      item.selectors = draft.selectors;
      item.favorite = draft.favorite;
      item.expiresAt = draft.expiresAt ?? null;
      item.folderIds = draft.folderIds.length > 0 ? draft.folderIds : [PERSONAL];
      item.uris = draft.uris.map((uri) => ({
        id: newId(),
        uri: uri.uri,
        matchType: uri.matchType ?? "domain",
      }));
      item.updatedAt = now();
      return strip(item);
    },

    async deleteItem(id): Promise<void> {
      requireUnlocked();
      const item = findItem(id);
      item.deletedAt = now();
      state.items = state.items.filter((candidate) => candidate.id !== id);
    },

    async setItemSecret(id, field, value): Promise<void> {
      requireUnlocked();
      const item = findItem(id);
      if (field === "password") {
        item.password = value;
        item.passwordUpdatedAt = value ? now() : null;
      } else if (field === "totp") {
        item.totpSecret = value;
      } else if (field === "notes") {
        item.notes = value;
      } else {
        throw new IpcError(
          "invalid_input",
          "Les champs personnalisés sont modifiés par identifiant.",
        );
      }
    },

    async revealSecret(id, field, customFieldId): Promise<RevealedSecret> {
      requireUnlocked();
      return reveal(findItem(id), field, customFieldId);
    },

    async totpCode(id): Promise<RevealedSecret> {
      requireUnlocked();
      return reveal(findItem(id), "totp");
    },

    async addItemCustomField(itemId, field) {
      requireUnlocked();
      const item = findItem(itemId);
      const created = {
        id: newId(),
        fieldName: field.fieldName,
        isSecret: field.isSecret,
        sortOrder: field.sortOrder,
      };
      item.customFields = [...item.customFields, created];
      item.customValues[created.id] = field.value;
      return strip(item);
    },

    async removeItemCustomField(itemId, fieldId): Promise<void> {
      requireUnlocked();
      const item = findItem(itemId);
      item.customFields = item.customFields.filter((field) => field.id !== fieldId);
      delete item.customValues[fieldId];
    },

    async generatePassword(options: PasswordOptions): Promise<GeneratedSecret> {
      const sets = [
        options.lowercase ? "abcdefghijkmnopqrstuvwxyz" : "",
        options.uppercase ? "ABCDEFGHJKLMNPQRSTUVWXYZ" : "",
        options.digits ? "23456789" : "",
        options.symbols ? "!@#$%^&*-_=+?" : "",
      ].filter((set) => set.length > 0);
      if (sets.length === 0) {
        throw new IpcError("invalid_input", "Sélectionnez au moins un jeu de caractères.");
      }
      const alphabet = sets.join("");
      const value = randomString(Math.max(options.length, sets.length), alphabet);
      return { value, entropyBits: entropyBits(value.length, alphabet.length) };
    },

    async generatePassphrase(options: PassphraseOptions): Promise<GeneratedSecret> {
      const count = Math.min(Math.max(options.words, 3), 12);
      const parts: string[] = [];
      for (let index = 0; index < count; index += 1) {
        const word = WORDS[Math.floor(Math.random() * WORDS.length)] ?? "atlas";
        parts.push(options.capitalize ? word.charAt(0).toUpperCase() + word.slice(1) : word);
      }
      const value = parts.join(options.separator);
      return { value, entropyBits: entropyBits(count, WORDS.length) };
    },

    async auditHealth(): Promise<HealthReport> {
      requireUnlocked();
      const passwords = new Map<string, number>();
      for (const item of state.items) {
        if (item.password) {
          passwords.set(item.password, (passwords.get(item.password) ?? 0) + 1);
        }
      }
      const findings: HealthFinding[] = [];
      for (const item of state.items) {
        if (!item.password) {
          continue;
        }
        const score = estimatePassword(item.password).score;
        item.zxcvbnScore = score;
        findings.push({
          itemId: item.id,
          title: item.title,
          primaryUrl: item.primaryUrl,
          username: item.username,
          score,
          reused: (passwords.get(item.password) ?? 0) > 1,
          compromised: item.isCompromised,
          ageDays:
            item.passwordUpdatedAt === null
              ? null
              : Math.max(0, Math.floor((now() - item.passwordUpdatedAt) / 86_400)),
        });
      }
      const total = findings.reduce((sum, finding) => sum + finding.score, 0);
      return {
        findings,
        withoutPassword: state.items.length - findings.length,
        averageScore: findings.length > 0 ? total / findings.length : null,
      };
    },

    async checkBreaches(): Promise<BreachReport> {
      requireUnlocked();
      // The demo vault never leaves the browser, so every prefix stays unknown.
      return {
        checked: 0,
        compromised: 0,
        unknown: state.items.filter((item) => item.password !== null).length,
      };
    },

    async exportHealthReport(): Promise<string | null> {
      requireUnlocked();
      return null;
    },

    async syncStatus(): Promise<SyncStatus> {
      return state.sync;
    },

    async syncSignIn(input): Promise<SyncStatus> {
      if (!input.supabaseUrl || !input.anonKey || !input.email || !input.password) {
        throw new IpcError("invalid_input", "Tous les champs Supabase sont requis.");
      }
      state.sync = {
        ...state.sync,
        enabled: true,
        connected: true,
        supabaseUrl: input.supabaseUrl,
        userId: "demo-user",
        lastStatus: "success",
        lastSyncAt: now(),
      };
      return state.sync;
    },

    async syncConfigure(enabled, supabaseUrl, anonKey): Promise<SyncStatus> {
      void anonKey;
      state.sync = { ...state.sync, enabled, supabaseUrl };
      return state.sync;
    },

    async syncDisconnect(): Promise<SyncStatus> {
      state.sync = {
        enabled: false,
        connected: false,
        supabaseUrl: null,
        userId: null,
        cursor: null,
        lastSyncAt: null,
        lastStatus: null,
        lastError: null,
      };
      return state.sync;
    },

    async syncNow(): Promise<SyncRun> {
      requireUnlocked();
      if (!state.sync.connected) {
        throw new IpcError("sync_disabled", "Connectez-vous à Supabase pour synchroniser.");
      }
      return {
        pushed: state.items.length,
        pulled: 0,
        applied: 0,
        skipped: 0,
        at: now(),
        full: false,
      };
    },

    async syncResync(): Promise<SyncRun> {
      const run = await this.syncNow();
      return { ...run, full: true };
    },

    async supabaseSetupSql(): Promise<string> {
      return "-- script Supabase (démo)\nselect 1;\n";
    },

    async saveSupabaseSetup(): Promise<string | null> {
      return null;
    },

    async copyText(value: string): Promise<void> {
      void value;
    },

    async listBackupTargets(): Promise<BackupTarget[]> {
      requireUnlocked();
      return state.backupTargets;
    },

    async createBackupTarget(input) {
      requireUnlocked();
      const target: BackupTarget = {
        id: newId(),
        name: input.name,
        provider: input.provider,
        summary:
          input.provider === "local"
            ? (input.path ?? "/tmp/akooti-backups")
            : input.provider === "s3"
              ? `${input.endpoint ?? "https://s3.example.test"} · ${input.bucket ?? "bucket"}`
              : "Google Drive",
        intervalMinutes: input.intervalMinutes,
        retentionCount: input.retentionCount,
        enabled: input.enabled,
        lastRunAt: null,
        lastStatus: null,
      };
      state.backupTargets = [...state.backupTargets, target];
      return target;
    },

    async updateBackupTarget(id, input) {
      requireUnlocked();
      const target = state.backupTargets.find((candidate) => candidate.id === id);
      if (!target) {
        throw new IpcError("not_found", "Cette cible n'existe plus.");
      }
      target.name = input.name;
      target.enabled = input.enabled;
      target.intervalMinutes = input.intervalMinutes;
      target.retentionCount = input.retentionCount;
      return target;
    },

    async deleteBackupTarget(id): Promise<void> {
      requireUnlocked();
      state.backupTargets = state.backupTargets.filter((candidate) => candidate.id !== id);
      state.backupRuns = state.backupRuns.filter((run) => run.targetId !== id);
    },

    async setBackupTargetEnabled(id, enabled) {
      requireUnlocked();
      const target = state.backupTargets.find((candidate) => candidate.id === id);
      if (!target) {
        throw new IpcError("not_found", "Cette cible n'existe plus.");
      }
      target.enabled = enabled;
      return target;
    },

    async testBackupTarget(input): Promise<string> {
      return input.provider === "local"
        ? `Dossier accessible : ${input.path ?? "—"}`
        : "Connexion simulée : OK";
    },

    async runBackupNow(id) {
      requireUnlocked();
      const target = state.backupTargets.find((candidate) => candidate.id === id);
      if (!target) {
        throw new IpcError("not_found", "Cette cible n'existe plus.");
      }
      const stamp = now();
      const run: BackupRun = {
        id: newId(),
        targetId: id,
        startedAt: stamp,
        status: "success",
        detail: target.summary,
        sizeBytes: 4096,
        snapshotName: `akooti-${stamp}.akooti`,
      };
      state.backupRuns = [run, ...state.backupRuns];
      target.lastRunAt = stamp;
      target.lastStatus = "success";
      return run;
    },

    async listBackupRuns(targetId, limit = 50) {
      requireUnlocked();
      return state.backupRuns
        .filter((run) => !targetId || run.targetId === targetId)
        .slice(0, limit);
    },

    async pickBackupFolder(): Promise<string | null> {
      return "/tmp/akooti-backups";
    },

    async exportSnapshot(): Promise<ExportReceipt | null> {
      requireUnlocked();
      return null;
    },

    async exportPlaintext(password): Promise<ExportReceipt | null> {
      requireUnlocked();
      if (password !== MASTER_PASSWORD) {
        throw new IpcError("invalid_credentials", "Mot de passe maître incorrect.");
      }
      return null;
    },

    async restoreSnapshot(): Promise<string | null> {
      return null;
    },

    async checkUpdate(): Promise<null> {
      return null;
    },

    async installUpdate(): Promise<void> {
      throw new IpcError("update", "Le mode démonstration ne peut pas installer de mise à jour.");
    },

    async listRoutingRules(): Promise<RoutingRule[]> {
      requireUnlocked();
      return state.rules;
    },
    async addRoutingRule(rule) {
      requireUnlocked();
      const created: RoutingRule = {
        id: newId(),
        pattern: rule.pattern,
        patternType: rule.patternType,
        targetFolderId: rule.targetFolderId,
        isDefault: false,
        enabled: rule.enabled,
        createdAt: now(),
      };
      state.rules = [...state.rules, created];
      return created;
    },

    async setRoutingRuleEnabled(id, enabled): Promise<RoutingRule> {
      requireUnlocked();
      const rule = state.rules.find((candidate) => candidate.id === id);
      if (!rule) {
        throw new IpcError("not_found", "Cette règle n'existe plus.");
      }
      rule.enabled = enabled;
      return rule;
    },

    async applyRoutingRules(): Promise<number> {
      requireUnlocked();
      if (!state.routingEnabled) {
        return 0;
      }
      let added = 0;
      for (const item of state.items) {
        const url = item.primaryUrl ?? "";
        const isLocal = /localhost|127\.0\.0\.1|\.local\b|\.internal\b/.test(url);
        if (isLocal && !item.folderIds.includes(DEV_LOCAL)) {
          item.folderIds = [...item.folderIds, DEV_LOCAL];
          added += 1;
        }
      }
      return added;
    },

    async listImportFormats(): Promise<ImportFormat[]> {
      return IMPORT_FORMATS;
    },

    async pickImportFile(): Promise<string | null> {
      return "/home/utilisateur/export-bitwarden.json";
    },

    async previewImportFile(
      path: string,
      _format?: string,
      _password?: string,
    ): Promise<ImportPreview> {
      requireUnlocked();
      if (!path) {
        throw new IpcError("invalid_source", "Aucun fichier sélectionné.");
      }
      return {
        itemCount: 12,
        folderCount: 3,
        duplicates: [
          {
            importedExternalId: "import-4",
            existingItemId: state.items[0]?.id ?? "unknown",
            reason: "url_and_username",
            score: 1,
          },
        ],
      };
    },

    async importFile(
      path: string,
      _format: string | undefined,
      options: ImportOptions,
    ): Promise<ImportReport> {
      requireUnlocked();
      if (!path) {
        throw new IpcError("invalid_source", "Aucun fichier sélectionné.");
      }
      return {
        imported: options.importDuplicates ? 12 : 11,
        skipped: options.importDuplicates ? 0 : 1,
        routed: options.routeDevLocal ? 4 : 0,
        issues: [],
        cleanup: options.secureDeleteSource ? "completed" : "not_requested",
      };
    },

    async listMcpTokens(): Promise<McpToken[]> {
      requireUnlocked();
      return state.tokens;
    },

    async createMcpToken(draft: McpTokenDraft) {
      requireUnlocked();
      const name = draft.name.trim();
      if (!name) {
        throw new IpcError("invalid_input", "Le nom du jeton est requis.");
      }
      if (draft.scopes.length === 0 || draft.folderIds.length === 0) {
        throw new IpcError("invalid_input", "Choisissez au moins une portée et un dossier.");
      }
      const stamp = now();
      const token: McpToken = {
        id: newId(),
        name,
        tokenPrefix: "ak_mcp_demo0000",
        scopes: draft.scopes as McpScope[],
        folderIds: draft.folderIds,
        defaultCreateFolderId: draft.defaultCreateFolderId ?? null,
        expiresAt: draft.ttlDays && draft.ttlDays > 0 ? stamp + draft.ttlDays * 86_400 : null,
        revokedAt: null,
        lastUsedAt: null,
        createdAt: stamp,
        active: true,
      };
      state.tokens = [token, ...state.tokens];
      state.audit = [
        {
          id: newId(),
          tokenId: token.id,
          action: "access_denied",
          targetItemId: null,
          targetFolderId: null,
          targetOrigin: null,
          status: "success",
          details: "jeton créé",
          createdAt: stamp,
        },
        ...state.audit,
      ];
      return { token, secret: `ak_mcp_${crypto.randomUUID().replaceAll("-", "")}` };
    },

    async revokeMcpToken(id: string): Promise<McpToken> {
      requireUnlocked();
      const token = state.tokens.find((candidate) => candidate.id === id);
      if (!token) {
        throw new IpcError("not_found", "Ce jeton n'existe plus.");
      }
      token.revokedAt = now();
      token.active = false;
      return token;
    },

    async deleteMcpToken(id: string): Promise<void> {
      requireUnlocked();
      state.tokens = state.tokens.filter((candidate) => candidate.id !== id);
    },

    async listMcpAuditLog(limit = 50): Promise<McpAuditEntry[]> {
      requireUnlocked();
      return state.audit.slice(0, limit);
    },

    async daemonStatus() {
      return { running: false, socketPath: null, capabilities: [] };
    },

    async copySecret(value: string): Promise<ClipboardReceipt> {
      if (!value) {
        throw new IpcError("invalid_input", "Il n'y a rien à copier.");
      }
      if (typeof navigator !== "undefined" && navigator.clipboard) {
        try {
          await navigator.clipboard.writeText(value);
        } catch {
          throw new IpcError("clipboard", "Le presse-papiers est indisponible.");
        }
      }
      return { ttlSeconds: SECRET_TTL_SECONDS };
    },
  };
}

function defaultRules(): RoutingRule[] {
  const stamp = now();
  return [
    { pattern: "localhost", patternType: "suffix", isDefault: true, enabled: true },
    {
      pattern: "127.0.0.1",
      patternType: "exact_host",
      isDefault: true,
      enabled: true,
    },
    { pattern: ".local", patternType: "suffix", isDefault: true, enabled: true },
    { pattern: ".internal", patternType: "suffix", isDefault: true, enabled: true },
    {
      pattern: "10.0.0.0/8",
      patternType: "cidr",
      isDefault: true,
      enabled: true,
    },
  ].map((rule, index) => ({
    id: `default-rule-${index}`,
    pattern: rule.pattern,
    patternType: rule.patternType as RoutingRule["patternType"],
    targetFolderId: DEV_LOCAL,
    isDefault: rule.isDefault,
    enabled: rule.enabled,
    createdAt: stamp,
  }));
}
