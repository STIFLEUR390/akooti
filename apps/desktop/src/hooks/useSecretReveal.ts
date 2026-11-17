import { useCallback, useEffect, useRef, useState } from "react";

import type { SecretField } from "@akooti/shared";

import { remainingSeconds } from "../lib/format";
import type { ClipboardReceipt, RevealedSecret } from "../lib/ipc";
import { toIpcError } from "../lib/ipc";

/** A secret currently on screen, with the deadline that hides it again. */
export interface RevealedView {
  field: SecretField;
  value: string;
  expiresAt: number;
  customFieldId?: string;
}

interface SecretPorts {
  revealSecret(id: string, field: SecretField, customFieldId?: string): Promise<RevealedSecret>;
  copySecret(value: string): Promise<ClipboardReceipt>;
}

interface SecretNotices {
  success(message: string): void;
  failure(message: string): void;
}

/**
 * Owns the only path that puts a plaintext secret on screen.
 *
 * A revealed value is dropped when its deadline passes, when another secret is
 * revealed, when the vault locks, or when the caller hides it. Copying goes
 * through the backend so the 30-second clipboard purge is enforced in Rust.
 */
export function useSecretReveal(api: SecretPorts, notices: SecretNotices) {
  const [revealed, setRevealed] = useState<RevealedView | null>(null);
  const [remaining, setRemaining] = useState(0);
  const [pending, setPending] = useState(false);
  const [clipboardSeconds, setClipboardSeconds] = useState(0);
  const clipboardDeadline = useRef(0);

  const hide = useCallback(() => {
    setRevealed(null);
    setRemaining(0);
  }, []);

  // Countdown for the visible secret.
  useEffect(() => {
    if (!revealed) {
      return;
    }
    const tick = () => {
      const seconds = remainingSeconds(revealed.expiresAt);
      setRemaining(seconds);
      if (seconds === 0) {
        setRevealed(null);
      }
    };
    tick();
    const interval = setInterval(tick, 250);
    return () => clearInterval(interval);
  }, [revealed]);

  // Countdown for the clipboard purge, driven by the backend receipt.
  useEffect(() => {
    if (clipboardSeconds === 0) {
      return;
    }
    const interval = setInterval(() => {
      const seconds = remainingSeconds(clipboardDeadline.current);
      setClipboardSeconds(seconds);
    }, 500);
    return () => clearInterval(interval);
  }, [clipboardSeconds]);

  const reveal = useCallback(
    async (itemId: string, field: SecretField, customFieldId?: string) => {
      setPending(true);
      try {
        const secret = await api.revealSecret(itemId, field, customFieldId);
        setRevealed({
          field: secret.field,
          value: secret.value,
          expiresAt: secret.expiresAt,
          customFieldId,
        });
        setRemaining(remainingSeconds(secret.expiresAt));
      } catch (error) {
        notices.failure(toIpcError(error).message);
      } finally {
        setPending(false);
      }
    },
    [api, notices],
  );

  const copy = useCallback(
    async (value: string, label: string) => {
      if (value.length === 0) {
        notices.failure("Il n'y a rien à copier.");
        return;
      }
      try {
        const receipt = await api.copySecret(value);
        clipboardDeadline.current = Date.now() + receipt.ttlSeconds * 1_000;
        setClipboardSeconds(receipt.ttlSeconds);
        notices.success(
          `${label} copié — effacement du presse-papiers dans ${receipt.ttlSeconds} s.`,
        );
      } catch (error) {
        notices.failure(toIpcError(error).message);
      }
    },
    [api, notices],
  );

  return {
    revealed,
    remaining,
    pending,
    clipboardSeconds,
    reveal,
    hide,
    copy,
  };
}
