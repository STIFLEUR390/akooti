import type { Toast } from "../hooks/useToasts";
import { Icon } from "./Icons";

interface ToastsProps {
  toasts: Toast[];
  onDismiss: (id: number) => void;
}

/** Bottom-right notification stack. `role="status"` keeps it screen-reader safe. */
export function Toasts({ toasts, onDismiss }: ToastsProps) {
  if (toasts.length === 0) {
    return null;
  }
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast--${toast.kind}`}>
          <Icon name={toast.kind === "error" ? "warn" : "check"} size={16} />
          <span>{toast.message}</span>
          <button
            type="button"
            className="icon-button icon-button--small"
            onClick={() => onDismiss(toast.id)}
            aria-label="Masquer la notification"
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
