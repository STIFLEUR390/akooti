import { defineContentScript } from "wxt/utils/define-content-script";
import { browser } from "wxt/browser";
import type { ContentReply, ContentRequest, FillReport } from "../lib/messages";
import type { FillPayload, SelectorSet } from "../lib/protocol";
import { detectForm, type FieldDescriptor } from "../lib/dom/detect";
import { clickSubmit, copyTransiently, fillInput, type InputLike } from "../lib/dom/fill";
import { buildSelector, formatCapture, type SelectorCapture } from "../lib/dom/capture";

/**
 * ISOLATED-world content script: the only place that writes into a page.
 *
 * It receives values from the service worker, injects them through the native
 * value setter, and answers with a non-revealing report. It never sends a value
 * back, never stores one, and never logs one — `docs/regles.md` SEC-2.
 */

/** Fields considered when looking for a login form. */
const CANDIDATE_SELECTOR = "input:not([type=hidden]), button, input[type=submit]";

export default defineContentScript({
  matches: ["http://*/*", "https://*/*"],
  runAt: "document_idle",
  main() {
    browser.runtime.onMessage.addListener((message: unknown) => {
      if (typeof message !== "object" || message === null) {
        return undefined;
      }
      const request = message as ContentRequest;
      switch (request.type) {
        case "akooti:inject":
          return Promise.resolve(handleInject(request));
        case "akooti:inspect":
          return Promise.resolve(handleInspect(request.mode));
        default:
          return undefined;
      }
    });
  },
});

function handleInject(request: Extract<ContentRequest, { type: "akooti:inject" }>): ContentReply {
  if (request.mode === "clipboard") {
    return handleClipboard(request.field, request.payload);
  }
  return handleFill(request.payload);
}

function handleFill(payload: FillPayload): ContentReply {
  const form = findForm();
  if (!form) {
    return { ok: false, code: "no_form", message: "no form field was found on this page" };
  }
  const detection = detectForm(form.descriptors);
  const selectors = mergeSelectors(payload, detection.selectors);
  const filled: string[] = [];
  const targets: Array<[string, string | null, string | null]> = [
    ["username", selectors.username, payload.username],
    ["password", selectors.password, payload.password],
    ["totp", selectors.totp, payload.totp],
  ];
  for (const [role, selector, value] of targets) {
    if (!selector || value === null) {
      continue;
    }
    const element = queryInput(selector);
    if (!element) {
      continue;
    }
    if (fillInput(toInputLike(element, selector), value)) {
      filled.push(role);
    }
  }
  if (filled.length === 0) {
    return { ok: false, code: "no_form", message: "no selector matched a fillable field" };
  }
  const submitted = clickSubmit(findSubmit(selectors.submit));
  const report: FillReport = { fieldsFilled: filled, submitted, detected: detection.kind };
  return { ok: true, kind: "fill", report };
}

function handleClipboard(
  field: "username" | "password" | "totp",
  payload: FillPayload,
): ContentReply {
  const value = payload[field];
  if (value === null) {
    return { ok: false, code: "no_form", message: `this item has no ${field} to copy` };
  }
  // The wipe timer lives here, next to the clipboard that owns the value.
  void copyTransiently(value, navigator.clipboard);
  return { ok: true, kind: "clipboard", field };
}

function handleInspect(mode: "detect" | "capture"): ContentReply {
  const form = findForm();
  if (!form) {
    return { ok: false, code: "no_form", message: "no form field was found on this page" };
  }
  const detection = detectForm(form.descriptors);
  if (mode === "detect") {
    return { ok: true, kind: "detect", detection, origin: location.origin };
  }
  const capture: SelectorCapture = {
    username: detection.selectors.username,
    password: detection.selectors.password,
    totp: detection.selectors.totp,
    submit: form.submitSelector,
  };
  return {
    ok: true,
    kind: "capture",
    report: { text: formatCapture(capture), detected: detection.kind },
  };
}

interface FoundForm {
  descriptors: FieldDescriptor[];
  submitSelector: string | null;
}

function findForm(): FoundForm | null {
  const elements = Array.from(document.querySelectorAll<HTMLElement>(CANDIDATE_SELECTOR)).filter(
    isVisible,
  );
  const descriptors: FieldDescriptor[] = [];
  let submitSelector: string | null = null;
  for (const element of elements) {
    if (!(element instanceof HTMLInputElement) || element.type === "submit") {
      continue;
    }
    descriptors.push(describe(element));
  }
  const button = elements.find(
    (element) =>
      element instanceof HTMLButtonElement ||
      (element instanceof HTMLInputElement && element.type === "submit"),
  );
  if (button) {
    submitSelector = selectorFor(button);
  }
  if (descriptors.length === 0) {
    return null;
  }
  return { descriptors, submitSelector };
}

function describe(element: HTMLInputElement): FieldDescriptor {
  return {
    selector: selectorFor(element),
    type: element.type.toLowerCase(),
    name: lower(element.name),
    id: lower(element.id),
    autocomplete: lower(element.autocomplete),
    label: lower(
      element.placeholder || element.getAttribute("aria-label") || associatedLabel(element) || "",
    ),
  };
}

function associatedLabel(element: HTMLInputElement): string {
  if (!element.labels) {
    return "";
  }
  return Array.from(element.labels)
    .map((label) => label.textContent ?? "")
    .join(" ");
}

function selectorFor(element: HTMLElement): string {
  const tag = element.tagName.toLowerCase();
  const id = element.id || null;
  const name = element.getAttribute("name");
  const type = element.getAttribute("type");
  return buildSelector({
    tag,
    id,
    name,
    type,
    index: siblingIndex(element),
    uniqueById: id !== null && document.querySelectorAll(`#${CSS.escape(id)}`).length === 1,
    uniqueByName:
      name !== null && document.querySelectorAll(`${tag}[name="${CSS.escape(name)}"]`).length === 1,
  });
}

function siblingIndex(element: HTMLElement): number {
  const parent = element.parentElement;
  if (!parent) {
    return 1;
  }
  const siblings = Array.from(parent.children).filter(
    (sibling) => sibling.tagName === element.tagName,
  );
  return siblings.indexOf(element) + 1;
}

function isVisible(element: HTMLElement): boolean {
  if (element.hidden) {
    return false;
  }
  const style = window.getComputedStyle(element);
  return style.display !== "none" && style.visibility !== "hidden";
}

function queryInput(selector: string): HTMLInputElement | null {
  try {
    const found = document.querySelector(selector);
    return found instanceof HTMLInputElement ? found : null;
  } catch {
    // A selector captured on another site may not compile here.
    return null;
  }
}

function findSubmit(selector: string | null): HTMLElement | null {
  if (!selector) {
    return null;
  }
  try {
    return document.querySelector<HTMLElement>(selector);
  } catch {
    return null;
  }
}

/**
 * Adapts a real input to the injectable interface.
 *
 * `setValue` goes through the prototype setter so React's internal value
 * tracker notices the change (a plain assignment does not fire its listeners).
 */
function toInputLike(element: HTMLInputElement, selector: string): InputLike {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
  return {
    selector,
    get value() {
      return element.value;
    },
    setValue(value: string) {
      if (setter) {
        setter.call(element, value);
      } else {
        element.value = value;
      }
    },
    dispatch(type: string) {
      element.dispatchEvent(new Event(type, { bubbles: true }));
    },
    focus() {
      element.focus();
    },
  };
}

function mergeSelectors(payload: FillPayload, detected: SelectorSet): SelectorSet {
  return {
    username: payload.selectors.username ?? detected.username,
    password: payload.selectors.password ?? detected.password,
    totp: payload.selectors.totp ?? detected.totp,
    submit: payload.selectors.submit ?? detected.submit,
  };
}

function lower(value: string | null | undefined): string | null {
  return typeof value === "string" && value.length > 0 ? value.toLowerCase() : null;
}
