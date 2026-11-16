import { useEffect, useRef } from "react";

/** Shortcut table: `mod+k`, `mod+shift+c`, `escape`, `/`, `alt+arrowup`… */
export type ShortcutMap = Record<string, () => void>;

/**
 * Normalises a keyboard event to the combo spelling used in {@link ShortcutMap}.
 *
 * `mod` is Cmd on macOS and Ctrl elsewhere, so the table stays OS agnostic.
 */
export function comboOf(event: KeyboardEvent): string {
  const parts: string[] = [];
  const isApple = navigator.userAgent.includes("Mac");
  const modifier = isApple ? event.metaKey : event.ctrlKey;
  if (modifier) {
    parts.push("mod");
  }
  if (event.altKey) {
    parts.push("alt");
  }
  if (event.shiftKey) {
    parts.push("shift");
  }
  const key = event.key === " " ? "space" : event.key.toLowerCase();
  parts.push(key);
  return parts.join("+");
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  return ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/**
 * Binds the application shortcuts.
 *
 * Unmodified combos (like `/`) are ignored while the user types in a field, so
 * searching never swallows a literal slash. Modified combos always fire.
 */
export function useShortcuts(map: ShortcutMap, enabled = true): void {
  const table = useRef(map);
  table.current = map;

  useEffect(() => {
    if (!enabled) {
      return;
    }
    const listener = (event: KeyboardEvent) => {
      if (event.defaultPrevented) {
        return;
      }
      const combo = comboOf(event);
      const handler = table.current[combo];
      if (!handler) {
        return;
      }
      const modified = combo.startsWith("mod+") || combo.startsWith("alt+");
      if (!modified && isTypingTarget(event.target)) {
        return;
      }
      event.preventDefault();
      handler();
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [enabled]);
}
