import { describe, expect, test } from "bun:test";

import { detectForm, isTotp, isUsername, type FieldDescriptor } from "./detect";

function field(partial: Partial<FieldDescriptor> & { selector: string }): FieldDescriptor {
  return {
    type: "text",
    name: null,
    id: null,
    autocomplete: null,
    label: null,
    ...partial,
  };
}

describe("form classification", () => {
  test("a password plus an identifier is a login", () => {
    const detection = detectForm([
      field({ selector: "#email", type: "email", autocomplete: "username" }),
      field({ selector: "#pass", type: "password", autocomplete: "current-password" }),
    ]);
    expect(detection.kind).toBe("login");
    expect(detection.selectors.username).toBe("#email");
    expect(detection.selectors.password).toBe("#pass");
    expect(detection.selectors.totp).toBeNull();
    expect(detection.score).toBe(3);
  });

  test("a lone password field is still a login step", () => {
    const detection = detectForm([field({ selector: "#pass", type: "password" })]);
    expect(detection.kind).toBe("login");
    expect(detection.selectors.username).toBeNull();
  });

  test("two password fields mean a subscription", () => {
    const detection = detectForm([
      field({ selector: "#email", type: "email" }),
      field({ selector: "#pass", type: "password" }),
      field({ selector: "#confirm", type: "password" }),
    ]);
    expect(detection.kind).toBe("signup");
    expect(detection.selectors.password).toBe("#pass");
  });

  test("new-password alone means a subscription", () => {
    const detection = detectForm([
      field({ selector: "#email", type: "email" }),
      field({ selector: "#pass", type: "password", autocomplete: "new-password" }),
    ]);
    expect(detection.kind).toBe("signup");
  });

  test("a confirmation label is enough to spot a subscription", () => {
    const detection = detectForm([
      field({ selector: "#pass", type: "password" }),
      field({ selector: "#pass2", type: "password", label: "repeat password" }),
    ]);
    expect(detection.kind).toBe("signup");
    expect(detection.score).toBe(3);
  });

  test("a one-time code without a password is a second step", () => {
    const detection = detectForm([
      field({ selector: "#code", type: "text", autocomplete: "one-time-code" }),
    ]);
    expect(detection.kind).toBe("totp");
    expect(detection.selectors.totp).toBe("#code");
  });

  test("hidden and submit inputs are ignored", () => {
    expect(
      detectForm([
        field({ selector: "#csrf", type: "hidden" }),
        field({ selector: "#go", type: "submit" }),
      ]),
    ).toEqual({
      kind: "unknown",
      score: 0,
      selectors: { username: null, password: null, totp: null, submit: null },
    });
  });
});

describe("field roles", () => {
  test("an email type is the identifier", () => {
    expect(isUsername(field({ selector: "#a", type: "email" }))).toBe(true);
  });

  test("a login-like name is the identifier", () => {
    expect(isUsername(field({ selector: "#a", name: "user_login" }))).toBe(true);
    // The content script lower-cases what it collects; the pattern is literal.
    expect(isUsername(field({ selector: "#a", label: "identifiant" }))).toBe(true);
    expect(isUsername(field({ selector: "#a", name: "company" }))).toBe(false);
  });

  test("a password is never the identifier nor a one-time code", () => {
    expect(isUsername(field({ selector: "#a", type: "password", name: "user" }))).toBe(false);
    expect(isTotp(field({ selector: "#a", type: "password", name: "otp" }))).toBe(false);
  });

  test("otp-like names and labels are one-time codes", () => {
    expect(isTotp(field({ selector: "#a", name: "otp" }))).toBe(true);
    expect(isTotp(field({ selector: "#a", label: "code de vérification" }))).toBe(true);
    expect(isTotp(field({ selector: "#a", type: "tel", autocomplete: "one-time-code" }))).toBe(
      true,
    );
    expect(isTotp(field({ selector: "#a", name: "search" }))).toBe(false);
  });
});
