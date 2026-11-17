/**
 * Injection of a value into a form field, and the event dance that makes
 * reactive frameworks notice it.
 *
 * React, Vue and Angular keep an internal value on the element: assigning
 * `input.value` directly updates the DOM but not the framework state, so the
 * submitted form still carries the old (empty) value. The recipe below is the
 * one the daemon already uses over CDP: go through the prototype setter, then
 * dispatch `input`, `change` and `blur` so every listener runs.
 *
 * The functions operate on a tiny interface rather than on `HTMLInputElement`,
 * so the sequence is unit-testable without a browser. No value is ever logged.
 */

/** Minimal surface of an input, as the content script adapts it. */
export interface InputLike {
  /** CSS selector that reached this input. */
  readonly selector: string;
  /** Current value. */
  readonly value: string;
  /** Assigns through the native prototype setter. */
  setValue(value: string): void;
  /** Dispatches a bubbling event of that type. */
  dispatch(type: string): void;
  /** Focuses the element, when it can be focused. */
  focus?(): void;
}

/** Fills one input and reports whether the value actually changed. */
export function fillInput(input: InputLike, value: string): boolean {
  input.focus?.();
  const previous = input.value;
  input.setValue(value);
  input.dispatch("input");
  input.dispatch("change");
  input.dispatch("blur");
  return previous !== value;
}

/** Clicks a submit control, if the caller found one. */
export function clickSubmit(control: { click(): void } | null): boolean {
  if (!control) {
    return false;
  }
  control.click();
  return true;
}

/**
 * Copies a value to the clipboard and schedules its erasure.
 *
 * `docs/regles.md` UX-4: a copied secret is wiped after 30 seconds. The timer
 * is injected so the behaviour is testable, and the value is never kept in a
 * closure longer than the wipe.
 */
export function copyTransiently(
  value: string,
  clipboard: { writeText(text: string): Promise<void> },
  schedule: (handler: () => void, milliseconds: number) => unknown = setTimeout,
  delayMs = 30_000,
): Promise<void> {
  schedule(() => {
    void clipboard.writeText("");
  }, delayMs);
  return clipboard.writeText(value);
}
