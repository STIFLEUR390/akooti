import { describe, expect, test } from "bun:test";

import { buildSelector, escapeAttribute, escapeIdentifier, formatCapture } from "./capture";

const BASE = {
  tag: "input",
  id: null,
  name: null,
  type: null,
  index: 1,
  uniqueById: false,
  uniqueByName: false,
};

describe("selector building", () => {
  test("an id wins when it is unique", () => {
    expect(buildSelector({ ...BASE, id: "password", name: "pass", uniqueById: true })).toBe(
      "#password",
    );
  });

  test("a unique name is next", () => {
    expect(buildSelector({ ...BASE, name: "user[login]", uniqueByName: true })).toBe(
      'input[name="user[login]"]',
    );
  });

  test("a non-unique id falls back to the name", () => {
    expect(buildSelector({ ...BASE, id: "pass", name: "password", uniqueByName: true })).toBe(
      'input[name="password"]',
    );
  });

  test("without an id or a unique name, the position decides", () => {
    expect(buildSelector({ ...BASE, type: "password", index: 2 })).toBe(
      'input[type="password"]:nth-of-type(2)',
    );
    expect(buildSelector({ ...BASE, index: 3 })).toBe("input:nth-of-type(3)");
  });

  test("quotes in an attribute value cannot break the selector", () => {
    expect(buildSelector({ ...BASE, name: 'a"b\\c', uniqueByName: true })).toBe(
      'input[name="a\\"b\\\\c"]',
    );
  });
});

describe("escaping", () => {
  test("identifiers escape everything unusual", () => {
    expect(escapeIdentifier("a b:c")).toBe("a\\ b\\:c");
    expect(escapeIdentifier("user-1_a")).toBe("user-1_a");
  });

  test("attribute values escape quotes and backslashes", () => {
    expect(escapeAttribute('x"y')).toBe('x\\"y');
    expect(escapeAttribute("x\\y")).toBe("x\\\\y");
  });
});

describe("capture formatting", () => {
  test("renders the four lines the item editor expects", () => {
    expect(
      formatCapture({
        username: "#email",
        password: "#pass",
        totp: null,
        submit: 'button[type="submit"]',
      }),
    ).toBe(
      ["username: #email", "password: #pass", "totp: ", 'submit: button[type="submit"]'].join("\n"),
    );
  });
});
