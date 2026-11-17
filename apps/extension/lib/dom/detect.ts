/**
 * Heuristic form detection.
 *
 * The content script collects a few facts about each visible input and the pure
 * functions below decide what the form is and which field plays which role.
 * Keeping the decision out of the DOM makes it testable without a browser and
 * easy to widen without touching the injection code.
 *
 * The heuristics only *name* fields, they never read, store or log a value.
 */
import { EMPTY_SELECTORS, type SelectorSet } from "../protocol";

/** Facts the content script gathers for one input. */
export interface FieldDescriptor {
  /** CSS selector that reaches this input again. */
  selector: string;
  /** Lower-case `input.type` (`password`, `email`, `text`, `tel`, `hidden`…). */
  type: string;
  /** Lower-case `name`. */
  name: string | null;
  /** Lower-case `id`. */
  id: string | null;
  /** Lower-case `autocomplete`. */
  autocomplete: string | null;
  /** Lower-case placeholder, aria-label or `<label>` text. */
  label: string | null;
}

/** What the form is for. */
export type FormKind = "login" | "signup" | "totp" | "unknown";

/** Detection result for one form. */
export interface FormDetection {
  kind: FormKind;
  /** Number of corroborating signals; a higher score is a safer guess. */
  score: number;
  /** Roles reached by the descriptors. */
  selectors: SelectorSet;
}

const USERNAME_HINTS = /user|email|e-mail|login|ident|account|mail/;
const TOTP_HINTS = /otp|totp|2fa|mfa|one.?time|auth|verif|code/;
const CONFIRM_HINTS = /confirm|repeat|again|verify|retype|repet/;

/**
 * Classifies one form from its fields.
 *
 * A password field is the strongest signal: one password plus a username is a
 * login, two passwords (or a `new-password` autocomplete, or a "confirm" field)
 * is a subscription. A one-time-code field without a password is a TOTP step.
 */
export function detectForm(fields: FieldDescriptor[]): FormDetection {
  const usable = fields.filter((field) => field.type !== "hidden" && field.type !== "submit");
  const passwords = usable.filter((field) => field.type === "password");
  const username = usable.find(isUsername);
  const totp = usable.find(isTotp);
  const hasNewPassword =
    passwords.some((field) => field.autocomplete === "new-password") ||
    passwords.some((field) => matches(field, CONFIRM_HINTS));

  let score = 0;
  if (passwords.length > 0) {
    score += 2;
  }
  if (username) {
    score += 1;
  }
  if (totp) {
    score += 2;
  }

  const selectors: SelectorSet = {
    ...EMPTY_SELECTORS,
    username: username?.selector ?? null,
    password: passwords[0]?.selector ?? null,
    totp: totp?.selector ?? null,
  };

  if (passwords.length >= 2 || hasNewPassword) {
    return { kind: "signup", score: score + 1, selectors };
  }
  if (passwords.length === 1) {
    // A lone password field is a login step: either the identifier comes first
    // on the same form, or the account is already known by the page.
    return { kind: "login", score, selectors };
  }
  if (totp) {
    return { kind: "totp", score, selectors };
  }
  return { kind: "unknown", score, selectors };
}

/** Whether a field looks like the account identifier. */
export function isUsername(field: FieldDescriptor): boolean {
  if (field.type === "password" || field.type === "hidden") {
    return false;
  }
  if (field.autocomplete === "username" || field.autocomplete === "email") {
    return true;
  }
  if (field.type === "email") {
    return true;
  }
  return field.type === "text" && matches(field, USERNAME_HINTS);
}

/** Whether a field looks like a one-time code input. */
export function isTotp(field: FieldDescriptor): boolean {
  if (field.type === "password" || field.type === "hidden") {
    return false;
  }
  if (field.autocomplete === "one-time-code") {
    return true;
  }
  return matches(field, TOTP_HINTS);
}

function matches(field: FieldDescriptor, pattern: RegExp): boolean {
  return [field.name, field.id, field.label, field.autocomplete].some(
    (candidate) => typeof candidate === "string" && pattern.test(candidate),
  );
}
