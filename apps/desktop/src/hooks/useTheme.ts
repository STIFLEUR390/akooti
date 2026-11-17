import { useCallback, useEffect, useState } from "react";

/** Theme modes the interface supports. */
export type ThemeMode = "dark" | "light";

const STORAGE_KEY = "akooti.theme";

function storedTheme(): ThemeMode | null {
  if (typeof localStorage === "undefined") {
    return null;
  }
  const value = localStorage.getItem(STORAGE_KEY);
  return value === "dark" || value === "light" ? value : null;
}

/**
 * Dark is the default (`docs/regles.md` §3.1); the choice is remembered locally
 * and applied through `data-theme` on `<html>`.
 */
export function useTheme() {
  const [theme, setTheme] = useState<ThemeMode>(() => storedTheme() ?? "dark");

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
    try {
      localStorage.setItem(STORAGE_KEY, theme);
    } catch {
      // Private browsing or a locked storage: the theme still applies.
    }
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }, []);

  return { theme, setTheme, toggle };
}
