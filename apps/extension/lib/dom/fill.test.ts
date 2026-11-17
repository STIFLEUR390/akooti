import { describe, expect, test } from "bun:test";

import { clickSubmit, copyTransiently, fillInput, type InputLike } from "./fill";

/** Input double: records the event dance. */
class FakeInput implements InputLike {
  readonly events: string[] = [];
  focused = 0;
  current: string;

  constructor(
    readonly selector: string,
    initial = "",
  ) {
    this.current = initial;
  }

  get value(): string {
    return this.current;
  }

  setValue(value: string): void {
    this.current = value;
  }

  dispatch(type: string): void {
    this.events.push(type);
  }

  focus(): void {
    this.focused += 1;
  }
}

describe("blind fill", () => {
  test("focuses, sets the value through the setter, then fires input, change and blur", () => {
    const input = new FakeInput("#pass");
    expect(fillInput(input, "s3cret")).toBe(true);
    expect(input.focused).toBe(1);
    expect(input.value).toBe("s3cret");
    expect(input.events).toEqual(["input", "change", "blur"]);
  });

  test("reports false when the field already held that value", () => {
    const input = new FakeInput("#pass", "s3cret");
    expect(fillInput(input, "s3cret")).toBe(false);
    expect(input.events).toEqual(["input", "change", "blur"]);
  });

  test("works without a focus method", () => {
    const input = new FakeInput("#pass");
    const withoutFocus: InputLike = {
      selector: input.selector,
      get value() {
        return input.value;
      },
      setValue: (value: string) => input.setValue(value),
      dispatch: (type: string) => input.dispatch(type),
    };
    expect(fillInput(withoutFocus, "value")).toBe(true);
    expect(input.value).toBe("value");
  });
});

describe("submission", () => {
  test("clicks the control it is handed", () => {
    let clicks = 0;
    expect(clickSubmit({ click: () => (clicks += 1) })).toBe(true);
    expect(clicks).toBe(1);
  });

  test("does nothing without a control", () => {
    expect(clickSubmit(null)).toBe(false);
  });
});

describe("transient clipboard", () => {
  test("writes the value, then wipes it after the delay", async () => {
    const writes: string[] = [];
    const scheduled: Array<() => void> = [];
    const clipboard = {
      writeText: async (text: string) => {
        writes.push(text);
      },
    };
    await copyTransiently(
      "s3cret",
      clipboard,
      (handler) => {
        scheduled.push(handler);
        return scheduled.length;
      },
      30_000,
    );
    expect(writes).toEqual(["s3cret"]);
    expect(scheduled).toHaveLength(1);
    scheduled[0]?.();
    await Promise.resolve();
    expect(writes).toEqual(["s3cret", ""]);
  });
});
