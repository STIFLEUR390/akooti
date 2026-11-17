/**
 * Selector capture: what the extension remembers about a form so the next fill
 * is exact instead of heuristic.
 *
 * The bridge cannot write to the vault (`docs/architecture.md` §2: the browser
 * bridge never writes), so a capture is kept in the extension, shown in the
 * popup, and pasted by the user into the item's selector fields. That keeps one
 * writer — the desktop app — and still gives the user a one-step copy.
 */

/** Facts needed to build a stable CSS selector. */
export interface SelectorSource {
  /** Lower-case tag name (`input`, `button`…). */
  tag: string;
  id: string | null;
  name: string | null;
  type: string | null;
  /** Position among its siblings of the same tag, 1-based. */
  index: number;
  /** True when `querySelectorAll` already returned exactly one match. */
  uniqueById: boolean;
  /** True when `[name="…"]` returned exactly one match. */
  uniqueByName: boolean;
}

/**
 * Builds the most specific, shortest selector the page allows.
 *
 * An id wins, then a unique name, then a tag + type + position fallback. The
 * result is escaped so a name with quotes or brackets cannot break the
 * selector the injection will evaluate later.
 */
export function buildSelector(source: SelectorSource): string {
  if (source.id && source.uniqueById) {
    return `#${escapeIdentifier(source.id)}`;
  }
  if (source.name && source.uniqueByName) {
    return `${source.tag}[name="${escapeAttribute(source.name)}"]`;
  }
  const type = source.type ? `[type="${escapeAttribute(source.type)}"]` : "";
  return `${source.tag}${type}:nth-of-type(${source.index})`;
}

/** Escapes one CSS identifier (`#…`). */
export function escapeIdentifier(value: string): string {
  return value.replace(/[^a-zA-Z0-9_-]/g, (character) => `\\${character}`);
}

/** Escapes one CSS attribute value. */
export function escapeAttribute(value: string): string {
  return value.replace(/["\\]/g, (character) => `\\${character}`);
}

/** A capture ready to be pasted into the vault. */
export interface SelectorCapture {
  username: string | null;
  password: string | null;
  totp: string | null;
  submit: string | null;
}

/** Serialises a capture as the four lines the item editor expects. */
export function formatCapture(capture: SelectorCapture): string {
  return [
    `username: ${capture.username ?? ""}`,
    `password: ${capture.password ?? ""}`,
    `totp: ${capture.totp ?? ""}`,
    `submit: ${capture.submit ?? ""}`,
  ].join("\n");
}
