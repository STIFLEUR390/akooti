import { useEffect, useId, useRef } from "react";
import type { ReactNode } from "react";

import { Icon } from "./Icons";

interface ModalProps {
  title: string;
  description?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** Wider layout for the item editor and the generator. */
  wide?: boolean;
}

/**
 * Accessible dialog.
 *
 * Focus moves to the first focusable control on open, `Escape` closes it, and
 * the page behind is inert (`aria-modal` + a real overlay). No portal library
 * is needed for an app this size.
 */
export function Modal({ title, description, onClose, children, footer, wide }: ModalProps) {
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const previous = document.activeElement;
    const first = panel.current?.querySelector<HTMLElement>(
      "input, select, textarea, button, [tabindex]:not([tabindex='-1'])",
    );
    first?.focus();
    return () => {
      if (previous instanceof HTMLElement) {
        previous.focus();
      }
    };
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !panel.current) {
        return;
      }
      // Keep Tab inside the dialog: the page behind is not inert.
      const focusables = Array.from(
        panel.current.querySelectorAll<HTMLElement>(
          "input, select, textarea, button, [href], [tabindex]:not([tabindex='-1'])",
        ),
      ).filter((element) => !element.hasAttribute("disabled") && element.offsetParent !== null);
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (!first || !last) {
        return;
      }
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", listener, true);
    return () => window.removeEventListener("keydown", listener, true);
  }, [onClose]);

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={onClose}>
      <div
        className={wide ? "modal modal--wide" : "modal"}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
        ref={panel}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="modal__header">
          <div>
            <h2 id={titleId}>{title}</h2>
            {description ? (
              <p id={descriptionId} className="modal__description">
                {description}
              </p>
            ) : null}
          </div>
          <button type="button" className="icon-button" onClick={onClose} aria-label="Fermer">
            <Icon name="close" />
          </button>
        </header>
        <div className="modal__body">{children}</div>
        {footer ? <footer className="modal__footer">{footer}</footer> : null}
      </div>
    </div>
  );
}
