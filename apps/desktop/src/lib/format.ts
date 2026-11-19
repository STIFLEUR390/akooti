/**
 * Pure presentation helpers. No React, no IPC: everything here is unit-tested
 * by `bun test`.
 */

import type { Folder, VaultItem } from "@akooti/shared";

/** Clamp a countdown to a non-negative whole number of seconds. */
export function remainingSeconds(deadlineMs: number, nowMs = Date.now()): number {
  const delta = deadlineMs - nowMs;
  return delta <= 0 ? 0 : Math.ceil(delta / 1_000);
}

/** Human label for a zxcvbn score (0-4). */
export function strengthLabel(score: number | null): string {
  switch (score) {
    case 0:
      return "Très faible";
    case 1:
      return "Faible";
    case 2:
      return "Moyen";
    case 3:
      return "Fort";
    case 4:
      return "Très fort";
    default:
      return "Non évalué";
  }
}

/** Bucket an entropy estimate into the same 0-4 scale as zxcvbn. */
export function entropyScore(bits: number): number {
  if (bits < 28) {
    return 0;
  }
  if (bits < 45) {
    return 1;
  }
  if (bits < 70) {
    return 2;
  }
  if (bits < 100) {
    return 3;
  }
  return 4;
}

/** "1 entrée", "2 entrées" — French keeps the singular for 0 and 1. */
export function countLabel(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count > 1 ? plural : singular}`;
}

/** "il y a 3 jours", "à l'instant", … */
export function relativeTime(epochSeconds: number | null, nowSeconds = Date.now() / 1_000): string {
  if (epochSeconds === null) {
    return "jamais";
  }
  const delta = Math.floor(nowSeconds - epochSeconds);
  if (delta < 60) {
    return "à l'instant";
  }
  const minutes = Math.floor(delta / 60);
  if (minutes < 60) {
    return `il y a ${minutes} min`;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `il y a ${hours} h`;
  }
  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `il y a ${days} j`;
  }
  const months = Math.floor(days / 30);
  if (months < 12) {
    return `il y a ${months} mois`;
  }
  return `il y a ${Math.floor(months / 12)} an${months >= 24 ? "s" : ""}`;
}

/** Absolute date for tooltips. */
export function absoluteDate(epochSeconds: number | null): string {
  if (epochSeconds === null) {
    return "—";
  }
  return new Date(epochSeconds * 1_000).toLocaleString("fr-FR", {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/** Item type label shown in lists. */
export function itemTypeLabel(itemType: VaultItem["itemType"]): string {
  switch (itemType) {
    case "login":
      return "Identifiant";
    case "api_key":
      return "Clé d'API";
    case "note":
      return "Note sécurisée";
    case "env_set":
      return "Variables d'env.";
  }
}

/** Import issue codes rendered in French. */
export function describeImportIssue(kind: string): string {
  switch (kind) {
    case "invalid_row":
      return "Ligne illisible";
    case "missing_title":
      return "Titre manquant";
    case "unsupported_item":
      return "Type non pris en charge";
    case "protected_xml_value":
      return "Champ protégé illisible";
    case "invalid_totp":
      return "Secret TOTP invalide";
    case "invalid_folder":
      return "Dossier introuvable";
    case "secure_delete_failed":
      return "Effacement de la source impossible";
    default:
      return kind;
  }
}

/** Cleanup outcome rendered in the import report. */
export function describeCleanup(cleanup: string): string {
  switch (cleanup) {
    case "completed":
      return "Fichier source effacé";
    case "failed":
      return "Fichier source conservé (effacement impossible)";
    default:
      return "Fichier source conservé";
  }
}

/** First letter of a title, for the avatar bubble. */
export function initials(title: string): string {
  const trimmed = title.trim();
  if (trimmed.length === 0) {
    return "?";
  }
  return trimmed.charAt(0).toUpperCase();
}

/** Folder name lookup that never throws on an unknown id. */
export function folderName(folders: Folder[], id: string): string {
  return folders.find((folder) => folder.id === id)?.name ?? "Dossier supprimé";
}

/** Host of an item's primary URL, or the raw value when it does not parse. */
export function hostOf(url: string | null): string {
  if (!url) {
    return "";
  }
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** Case- and accent-insensitive search over the fields the list shows. */
export function matchesQuery(item: VaultItem, query: string): boolean {
  const needle = normalize(query);
  if (needle.length === 0) {
    return true;
  }
  const haystack = normalize(
    [item.title, item.username ?? "", item.primaryUrl ?? "", itemTypeLabel(item.itemType)].join(
      " ",
    ),
  );
  return haystack.includes(needle);
}

function normalize(value: string): string {
  return value
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase()
    .trim();
}

/** Groups items by their first folder so the list order is stable. */
export function sortItems(items: VaultItem[]): VaultItem[] {
  return [...items].sort((left, right) => {
    if (left.favorite !== right.favorite) {
      return left.favorite ? -1 : 1;
    }
    return left.title.localeCompare(right.title, "fr", { sensitivity: "base" });
  });
}
