import { describe, expect, test } from "bun:test";

import { estimatePassword } from "./strength";

describe("offline strength estimate", () => {
  test("empty input scores zero", () => {
    expect(estimatePassword("")).toEqual({ score: 0, entropyBits: 0, warnings: [] });
  });

  test("grows with length and alphabet", () => {
    const short = estimatePassword("abc123");
    const longer = estimatePassword("abc123def456ghi789");
    const mixed = estimatePassword("abc123def456ghi789!@#");

    expect(short.score).toBeLessThan(longer.score);
    expect(longer.score).toBeLessThanOrEqual(mixed.score);
    expect(short.entropyBits).toBeLessThan(longer.entropyBits);
    expect(longer.entropyBits).toBeLessThan(mixed.entropyBits);
  });

  test("penalises a dictionary word", () => {
    const plain = estimatePassword("motdepasse");
    const random = estimatePassword("vq7xk2mp");
    expect(plain.entropyBits).toBeLessThan(random.entropyBits);
    expect(plain.warnings).toContain("Motif courant détecté");
  });

  test("penalises keyboard runs and triple repeats", () => {
    expect(estimatePassword("azertyuiop1234").warnings).toContain("Suite de caractères prévisible");
    expect(estimatePassword("aaabbbccc111").warnings).toContain("Caractère répété trois fois");
  });

  test("always gives actionable warnings below the minimum length", () => {
    const estimate = estimatePassword("Court1");
    expect(estimate.warnings).toContain("12 caractères minimum");
    expect(estimate.score).toBeLessThanOrEqual(1);
  });

  test("a long random passphrase reaches the top score", () => {
    const estimate = estimatePassword("correct-horse-battery-staple-42");
    expect(estimate.score).toBe(4);
    expect(estimate.entropyBits).toBeGreaterThanOrEqual(100);
  });
});
