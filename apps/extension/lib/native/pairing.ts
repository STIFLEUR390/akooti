/**
 * Token pairing between the desktop app and the extension.
 *
 * The user creates a token in **Accès IA** (scopes `read_metadata` and
 * `blind_fill`, restricted to the folders the extension may read) and pastes it
 * in the popup. It is the extension's only credential: the daemon authenticates
 * it on every request, so an extension left unattended cannot outlive a
 * revocation.
 *
 * The token is a secret (`docs/regles.md` SEC-9): it is stored in
 * `storage.local`, never logged, never shown again after pairing, and removed
 * by `clearPairing` on unpair. It is never sent anywhere but the local daemon.
 */

/** Key used in `browser.storage.local`. */
export const PAIRING_KEY = "akooti.pairing";

/** Prefix of every token, mirroring `akooti_core::TOKEN_PREFIX`. */
const TOKEN_PREFIX = "ak_mcp_";
const TOKEN_HEX_CHARS = 64;

/** One paired token. */
export interface Pairing {
  token: string;
  /** User-facing label, usually the token name shown by the desktop app. */
  label: string | null;
  /** Pairing time, epoch milliseconds. */
  pairedAt: number;
}

/** The three calls the pairing code needs from a key/value store. */
export interface PairingStore {
  get(keys: string): Promise<Record<string, unknown>>;
  set(items: Record<string, unknown>): Promise<void>;
  remove(key: string): Promise<void>;
}

/** Whether a pasted value looks like an Akooti token (shape only, no network). */
export function isTokenShaped(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed.startsWith(TOKEN_PREFIX)) {
    return false;
  }
  const suffix = trimmed.slice(TOKEN_PREFIX.length);
  return suffix.length === TOKEN_HEX_CHARS && /^[0-9a-f]+$/.test(suffix);
}

/**
 * Returns the display form of a token: prefix and first characters only.
 *
 * Used by the popup so the full secret never sits in the DOM after pairing.
 */
export function maskedToken(token: string): string {
  if (!isTokenShaped(token)) {
    return "—";
  }
  return `${token.slice(0, TOKEN_PREFIX.length + 4)}…${token.slice(-4)}`;
}

/** Reads the pairing, or `null` when the extension is not paired. */
export async function readPairing(store: PairingStore): Promise<Pairing | null> {
  const stored = await store.get(PAIRING_KEY);
  const value = stored[PAIRING_KEY];
  if (!isPairing(value)) {
    return null;
  }
  return value;
}

/** Stores a pairing after validating the token shape. */
export async function writePairing(
  store: PairingStore,
  token: string,
  options: { label?: string | null; now?: number } = {},
): Promise<Pairing> {
  const trimmed = token.trim();
  if (!isTokenShaped(trimmed)) {
    throw new Error("this is not an Akooti token (expected ak_mcp_ followed by 64 hex characters)");
  }
  const pairing: Pairing = {
    token: trimmed,
    label: options.label?.trim() || null,
    pairedAt: options.now ?? Date.now(),
  };
  await store.set({ [PAIRING_KEY]: pairing });
  return pairing;
}

/** Forgets the pairing. */
export async function clearPairing(store: PairingStore): Promise<void> {
  await store.remove(PAIRING_KEY);
}

function isPairing(value: unknown): value is Pairing {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as Partial<Pairing>;
  return (
    typeof candidate.token === "string" &&
    isTokenShaped(candidate.token) &&
    (candidate.label === null || typeof candidate.label === "string") &&
    typeof candidate.pairedAt === "number"
  );
}
