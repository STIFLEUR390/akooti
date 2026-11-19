import { describe, expect, test } from "bun:test";

import {
  countLabel,
  describeCleanup,
  describeImportIssue,
  entropyScore,
  hostOf,
  initials,
  itemTypeLabel,
  matchesQuery,
  relativeTime,
  remainingSeconds,
  sortItems,
  strengthLabel,
} from "./format";
import type { VaultItem } from "@akooti/shared";

function item(partial: Partial<VaultItem> & Pick<VaultItem, "id" | "title">): VaultItem {
  return {
    itemType: "login",
    primaryUrl: null,
    username: null,
    selectors: {},
    zxcvbnScore: null,
    isCompromised: false,
    passwordUpdatedAt: null,
    createdByAgent: null,
    expiresAt: null,
    favorite: false,
    folderIds: [],
    uris: [],
    customFields: [],
    createdAt: 0,
    updatedAt: 0,
    deletedAt: null,
    ...partial,
  };
}

describe("countLabel", () => {
  test("keeps the singular for 0 and 1", () => {
    expect(countLabel(0, "dossier")).toBe("0 dossier");
    expect(countLabel(1, "dossier")).toBe("1 dossier");
    expect(countLabel(3, "dossier")).toBe("3 dossiers");
    expect(countLabel(2, "appliqué")).toBe("2 appliqués");
  });
});

describe("clipboard countdown", () => {
  test("never goes negative", () => {
    expect(remainingSeconds(1_000, 1_000)).toBe(0);
    expect(remainingSeconds(500, 1_000)).toBe(0);
  });

  test("rounds up so the badge never shows 0 while a second remains", () => {
    expect(remainingSeconds(1_001, 1_000)).toBe(1);
    expect(remainingSeconds(30_000, 0)).toBe(30);
    expect(remainingSeconds(15_400, 0)).toBe(16);
  });
});

describe("strength scale", () => {
  test("maps every score to a French label", () => {
    expect(["Très faible", "Faible", "Moyen", "Fort", "Très fort"]).toEqual(
      [0, 1, 2, 3, 4].map((score) => strengthLabel(score)),
    );
    expect(strengthLabel(null)).toBe("Non évalué");
  });

  test("buckets entropy on the zxcvbn scale", () => {
    expect(entropyScore(0)).toBe(0);
    expect(entropyScore(28)).toBe(1);
    expect(entropyScore(45)).toBe(2);
    expect(entropyScore(70)).toBe(3);
    expect(entropyScore(100)).toBe(4);
  });
});

describe("relative time", () => {
  const now = 1_700_000_000;

  test("describes recent and old timestamps", () => {
    expect(relativeTime(null)).toBe("jamais");
    expect(relativeTime(now, now)).toBe("à l'instant");
    expect(relativeTime(now - 90, now)).toBe("il y a 1 min");
    expect(relativeTime(now - 7_200, now)).toBe("il y a 2 h");
    expect(relativeTime(now - 172_800, now)).toBe("il y a 2 j");
    expect(relativeTime(now - 90 * 86_400, now)).toBe("il y a 3 mois");
    expect(relativeTime(now - 800 * 86_400, now)).toBe("il y a 2 ans");
  });
});

describe("item presentation", () => {
  test("extracts a host without throwing on garbage", () => {
    expect(hostOf("https://app.example.test/login")).toBe("app.example.test");
    expect(hostOf("not a url")).toBe("not a url");
    expect(hostOf(null)).toBe("");
  });

  test("labels every item type", () => {
    expect(itemTypeLabel("login")).toBe("Identifiant");
    expect(itemTypeLabel("api_key")).toBe("Clé d'API");
    expect(itemTypeLabel("note")).toBe("Note sécurisée");
    expect(itemTypeLabel("env_set")).toBe("Variables d'env.");
  });

  test("builds an avatar initial", () => {
    expect(initials("  akooti")).toBe("A");
    expect(initials("")).toBe("?");
  });
});

describe("search", () => {
  const entry = item({
    id: "1",
    title: "Café interne",
    username: "herold@example.test",
    primaryUrl: "http://service.internal",
  });

  test("is accent and case insensitive", () => {
    expect(matchesQuery(entry, "cafe")).toBe(true);
    expect(matchesQuery(entry, "CAFÉ")).toBe(true);
  });

  test("matches username, url and type", () => {
    expect(matchesQuery(entry, "herold")).toBe(true);
    expect(matchesQuery(entry, "internal")).toBe(true);
    expect(matchesQuery(entry, "identifiant")).toBe(true);
  });

  test("rejects an unrelated query", () => {
    expect(matchesQuery(entry, "banque")).toBe(false);
  });

  test("an empty query matches everything", () => {
    expect(matchesQuery(entry, "   ")).toBe(true);
  });
});

describe("sorting", () => {
  test("puts favourites first then sorts by title", () => {
    const sorted = sortItems([
      item({ id: "a", title: "Zèbre" }),
      item({ id: "b", title: "abricot" }),
      item({ id: "c", title: "Bravo", favorite: true }),
    ]);
    expect(sorted.map((entry) => entry.id)).toEqual(["c", "b", "a"]);
  });
});

describe("import vocabulary", () => {
  test("translates issue kinds and cleanup outcomes", () => {
    expect(describeImportIssue("invalid_totp")).toBe("Secret TOTP invalide");
    expect(describeImportIssue("mystery")).toBe("mystery");
    expect(describeCleanup("completed")).toBe("Fichier source effacé");
    expect(describeCleanup("failed")).toBe("Fichier source conservé (effacement impossible)");
    expect(describeCleanup("not_requested")).toBe("Fichier source conservé");
  });
});
