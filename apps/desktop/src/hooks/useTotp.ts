import { useCallback, useEffect, useState } from "react";

import type { RevealedSecret } from "../lib/ipc";

interface TotpPort {
  totpCode(id: string): Promise<RevealedSecret>;
}

/** Seconds left in the current 30-second TOTP window. */
export function totpSecondsLeft(nowMs = Date.now()): number {
  const period = 30;
  const seconds = Math.floor(nowMs / 1_000) % period;
  return period - seconds;
}

/**
 * Keeps a live TOTP code for one item.
 *
 * The code is a short-lived secret: it lives in component state only, is
 * refetched when the RFC 6238 window rolls over, and is dropped as soon as the
 * item changes or the caller disables the hook.
 */
export function useTotp(api: TotpPort, itemId: string | null, enabled: boolean) {
  const [code, setCode] = useState<string | null>(null);
  const [seconds, setSeconds] = useState(() => totpSecondsLeft());

  const refresh = useCallback(async () => {
    if (!itemId || !enabled) {
      setCode(null);
      return;
    }
    try {
      const secret = await api.totpCode(itemId);
      setCode(secret.value);
    } catch {
      // No TOTP seed, an invalid seed, or a locked vault: no code to show.
      setCode(null);
    }
  }, [api, enabled, itemId]);

  useEffect(() => {
    setCode(null);
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!enabled) {
      return;
    }
    const tick = () => {
      const left = totpSecondsLeft();
      setSeconds(left);
      if (left >= 30) {
        // A new window just opened: ask the backend for the next code.
        void refresh();
      }
    };
    const interval = setInterval(tick, 1_000);
    return () => clearInterval(interval);
  }, [enabled, refresh]);

  return { code, seconds, refresh };
}
