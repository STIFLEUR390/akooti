import { describe, expect, test } from "bun:test";

import { DEV_LOCAL_FOLDER_ID, PERSONAL_FOLDER_ID } from "@akooti/shared";
import { createMockApi } from "./mock";
import { IpcError } from "./ipc";

/**
 * Front/back contract test.
 *
 * The mock implements the same {@link AkootiApi} interface as the Rust command
 * surface, so these assertions describe the behaviour the Tauri backend must
 * keep: metadata reads never carry a secret, reveal is explicit and expiring,
 * system folders are protected, routing only adds memberships.
 */

async function unlocked() {
  const api = createMockApi();
  await api.createVault("akooti-demo-2026");
  return api;
}

describe("vault lifecycle", () => {
  test("a fresh mock starts locked without a vault", async () => {
    const api = createMockApi();
    const status = await api.vaultStatus();
    expect(status).toEqual({ status: "locked", hasVault: false, autoLockMinutes: 15 });
  });

  test("creation returns the three protected folders", async () => {
    const api = await unlocked();
    const folders = await api.listFolders();
    expect(folders).toHaveLength(3);
    expect(folders.every((folder) => folder.isSystem)).toBe(true);
    expect((await api.vaultStatus()).status).toBe("unlocked");
  });

  test("creation refuses a short password and a second vault", async () => {
    const api = createMockApi();
    await expect(api.createVault("court")).rejects.toBeInstanceOf(IpcError);
    await api.createVault("akooti-demo-2026");
    await expect(api.createVault("akooti-demo-2026")).rejects.toMatchObject({
      code: "vault_exists",
    });
  });

  test("a locked vault refuses every read", async () => {
    const api = await unlocked();
    await api.lockVault();
    await expect(api.listItems({})).rejects.toMatchObject({ code: "locked" });
  });
});

describe("metadata reads never carry a secret", () => {
  test("listItems exposes presence flags only", async () => {
    const api = await unlocked();
    const items = await api.listItems({});
    expect(items.length).toBeGreaterThan(0);
    const serialized = JSON.stringify(items);
    expect(serialized).not.toContain("correct-horse-battery");
    expect(serialized).not.toContain("dev-only-password");
    expect(serialized).not.toContain("GEZDGNBVGY3TQOJQ");
    expect(items.some((item) => item.hasPassword)).toBe(true);
  });

  test("reveal is explicit and carries a bounded expiry", async () => {
    const api = await unlocked();
    const items = await api.listItems({});
    const target = items.find((item) => item.title === "Blog personnel");
    expect(target).toBeDefined();
    const revealed = await api.revealSecret(target!.id, "password");
    expect(revealed.value).toBe("correct-horse-battery");
    expect(revealed.expiresAt).toBeGreaterThan(Date.now());
    expect(revealed.expiresAt).toBeLessThanOrEqual(Date.now() + 30_000);
  });

  test("revealing a cleared secret fails with not_found", async () => {
    const api = await unlocked();
    const items = await api.listItems({});
    const target = items.find((item) => item.title === "Blog personnel")!;
    await api.setItemSecret(target.id, "password", null);
    const reloaded = await api.getItem(target.id);
    expect(reloaded.hasPassword).toBe(false);
    await expect(api.revealSecret(target.id, "password")).rejects.toMatchObject({
      code: "not_found",
    });
  });
});

describe("item mutations", () => {
  test("create then update keeps secrets out of the metadata payload", async () => {
    const api = await unlocked();
    const created = await api.createItem({
      itemType: "login",
      title: "Nouvelle",
      primaryUrl: "https://example.test",
      username: "user",
      password: "s3cret-value",
      totpSecret: null,
      notes: null,
      selectors: {},
      favorite: false,
      expiresAt: null,
      folderIds: [PERSONAL_FOLDER_ID],
      uris: [{ uri: "https://example.test", matchType: "domain" }],
      customFields: [],
    });
    expect(created.hasPassword).toBe(true);

    const updated = await api.updateItem({
      id: created.id,
      itemType: created.itemType,
      title: "Renommée",
      primaryUrl: created.primaryUrl,
      username: "user2",
      selectors: {},
      favorite: true,
      expiresAt: null,
      folderIds: created.folderIds,
      uris: [],
    });
    expect(updated.title).toBe("Renommée");
    expect(updated.hasPassword).toBe(true);
    expect((await api.revealSecret(created.id, "password")).value).toBe("s3cret-value");
  });

  test("system folders cannot be renamed or deleted", async () => {
    const api = await unlocked();
    await expect(api.renameFolder(PERSONAL_FOLDER_ID, "Autre")).rejects.toMatchObject({
      code: "system_folder_protected",
    });
    await expect(api.deleteFolder(PERSONAL_FOLDER_ID)).rejects.toMatchObject({
      code: "system_folder_protected",
    });
  });

  test("removing the last folder keeps the entry in Personnel", async () => {
    const api = await unlocked();
    const items = await api.listItems({});
    const target = items.find((item) => item.folderIds.length === 1)!;
    await api.removeItemFromFolder(target.id, target.folderIds[0]!);
    const reloaded = await api.getItem(target.id);
    expect(reloaded.folderIds).toEqual([PERSONAL_FOLDER_ID]);
  });
});

describe("Dev Local routing", () => {
  test("the retroactive pass only adds memberships", async () => {
    const api = await unlocked();
    const created = await api.createItem({
      itemType: "login",
      title: "Service local",
      primaryUrl: "http://localhost:9999",
      username: null,
      password: null,
      totpSecret: null,
      notes: null,
      selectors: {},
      favorite: false,
      expiresAt: null,
      folderIds: [PERSONAL_FOLDER_ID],
      uris: [],
      customFields: [],
    });
    expect(created.folderIds).toEqual([PERSONAL_FOLDER_ID]);

    const added = await api.applyRoutingRules();
    expect(added).toBeGreaterThanOrEqual(1);

    const reloaded = await api.getItem(created.id);
    expect(reloaded.folderIds).toContain(DEV_LOCAL_FOLDER_ID);
    expect(reloaded.folderIds).toContain(PERSONAL_FOLDER_ID);
  });

  test("a disabled rule is ignored", async () => {
    const api = await unlocked();
    const rules = await api.listRoutingRules();
    expect(rules.length).toBe(5);
    expect(rules.every((rule) => rule.isDefault)).toBe(true);
  });
});

describe("import wizard", () => {
  test("preview reports counts without writing", async () => {
    const api = await unlocked();
    const before = (await api.listItems({})).length;
    const preview = await api.previewImportFile("/tmp/export.json", "auto");
    expect(preview.itemCount).toBeGreaterThan(0);
    expect(preview.duplicates.length).toBeGreaterThan(0);
    expect((await api.listItems({})).length).toBe(before);
  });

  test("report reflects the chosen options", async () => {
    const api = await unlocked();
    const report = await api.importFile("/tmp/export.json", "auto", {
      routeDevLocal: false,
      importDuplicates: true,
      secureDeleteSource: false,
    });
    expect(report.skipped).toBe(0);
    expect(report.routed).toBe(0);
    expect(report.cleanup).toBe("not_requested");
  });

  test("an empty path is refused", async () => {
    const api = await unlocked();
    await expect(
      api.importFile("", "auto", {
        routeDevLocal: true,
        importDuplicates: false,
        secureDeleteSource: false,
      }),
    ).rejects.toMatchObject({ code: "invalid_source" });
  });
});

describe("generator", () => {
  test("honours the requested length and character classes", async () => {
    const api = await createMockApi();
    const digits = await api.generatePassword({
      length: 32,
      lowercase: false,
      uppercase: false,
      digits: true,
      symbols: false,
      excludeAmbiguous: true,
    });
    expect(digits.value).toHaveLength(32);
    expect(/^[2-9]+$/.test(digits.value)).toBe(true);
    expect(digits.entropyBits).toBeGreaterThan(0);
  });

  test("rejects an empty alphabet", async () => {
    const api = createMockApi();
    await expect(
      api.generatePassword({
        length: 20,
        lowercase: false,
        uppercase: false,
        digits: false,
        symbols: false,
        excludeAmbiguous: true,
      }),
    ).rejects.toMatchObject({ code: "invalid_input" });
  });

  test("tokens: the secret is returned once and never listed", async () => {
    const api = await unlocked();
    const issued = await api.createMcpToken({
      name: "Agent local",
      scopes: ["read_metadata"],
      folderIds: [DEV_LOCAL_FOLDER_ID],
      ttlDays: 30,
    });
    expect(issued.secret.startsWith("ak_mcp_")).toBe(true);
    expect(issued.token.active).toBe(true);

    const listed = await api.listMcpTokens();
    expect(listed).toHaveLength(1);
    expect(JSON.stringify(listed)).not.toContain(issued.secret);
    expect(listed[0]?.tokenPrefix).not.toBe(issued.secret);
    expect(listed[0]?.folderIds).toEqual([DEV_LOCAL_FOLDER_ID]);

    const revoked = await api.revokeMcpToken(issued.token.id);
    expect(revoked.active).toBe(false);
    expect(revoked.revokedAt).not.toBeNull();

    const audit = await api.listMcpAuditLog(10);
    expect(audit[0]?.details).toBe("jeton créé");
    expect(JSON.stringify(audit)).not.toContain(issued.secret);
  });

  test("tokens: creation needs a name, a scope and a folder", async () => {
    const api = await unlocked();
    await expect(
      api.createMcpToken({
        name: " ",
        scopes: ["read_metadata"],
        folderIds: [DEV_LOCAL_FOLDER_ID],
      }),
    ).rejects.toMatchObject({ code: "invalid_input" });
    await expect(
      api.createMcpToken({ name: "Agent", scopes: [], folderIds: [DEV_LOCAL_FOLDER_ID] }),
    ).rejects.toMatchObject({ code: "invalid_input" });
    await expect(
      api.createMcpToken({ name: "Agent", scopes: ["read_metadata"], folderIds: [] }),
    ).rejects.toMatchObject({ code: "invalid_input" });
    await expect(
      api.createMcpToken({
        name: "Agent",
        scopes: ["read_metadata"],
        folderIds: [PERSONAL_FOLDER_ID],
      }),
    ).resolves.toBeTruthy();
  });

  test("passphrases respect the word count", async () => {
    const api = createMockApi();
    const phrase = await api.generatePassphrase({ words: 5, separator: ".", capitalize: false });
    expect(phrase.value.split(".")).toHaveLength(5);
  });
});
