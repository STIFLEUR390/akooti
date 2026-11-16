import { useCallback, useRef, useState } from "react";

/** A transient message shown in the bottom-right corner. */
export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  message: string;
}

/** Stable notifier pair passed to the data hooks. */
export interface Notices {
  success(message: string): void;
  failure(message: string): void;
}

/**
 * Push and retire toasts.
 *
 * The returned callbacks are referentially stable, so they are safe to list as
 * hook dependencies; only `toasts` changes on every push.
 */
export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const counter = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const push = useCallback(
    (kind: Toast["kind"], message: string) => {
      counter.current += 1;
      const id = counter.current;
      setToasts((current) => [...current.slice(-3), { id, kind, message }]);
      setTimeout(() => dismiss(id), kind === "error" ? 6_000 : 3_500);
    },
    [dismiss],
  );

  const success = useCallback(
    (message: string) => {
      push("success", message);
    },
    [push],
  );

  const failure = useCallback(
    (message: string) => {
      push("error", message);
    },
    [push],
  );

  return { toasts, dismiss, success, failure };
}
