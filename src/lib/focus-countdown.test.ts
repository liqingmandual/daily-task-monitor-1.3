import { describe, expect, it } from "vitest";
import { nextFocusTickDelay, remainingFocusSeconds } from "./focus-countdown";

describe("focus countdown timing", () => {
  it("counts down on the actual end-time second boundary", () => {
    expect(remainingFocusSeconds(84_000, 1_000)).toBe(83);
    expect(remainingFocusSeconds(84_000, 1_250)).toBe(83);
    expect(nextFocusTickDelay(84_000, 1_250)).toBe(750);
    expect(remainingFocusSeconds(84_000, 2_000)).toBe(82);
  });

  it("stops scheduling once expired", () => {
    expect(remainingFocusSeconds(84_000, 84_001)).toBe(0);
    expect(nextFocusTickDelay(84_000, 84_001)).toBeNull();
  });
});
