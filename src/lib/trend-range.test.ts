import { describe, expect, it } from "vitest";
import {
  defaultTrendGranularityForSelectedDates,
  defaultTrendGranularity,
  moveTrendPreset,
  normalizeTrendSelectedDates,
  resolveTrendRange,
  resolveTrendSelectedDates,
  shiftTrendSelectedDates,
  shiftTrendRange,
} from "./trend-range";

describe("resolveTrendRange", () => {
  it("resolves an inclusive last seven days range", () => {
    expect(resolveTrendRange("week", "2026-07-12", "", "")).toEqual({
      startDate: "2026-07-06",
      endDate: "2026-07-12",
    });
  });

  it("resolves an inclusive last thirty days range across a month boundary", () => {
    expect(resolveTrendRange("month", "2026-07-12", "", "")).toEqual({
      startDate: "2026-06-13",
      endDate: "2026-07-12",
    });
  });

  it("accepts leap day in a valid custom range", () => {
    expect(resolveTrendRange("custom", "", "2024-02-29", "2024-03-01")).toEqual({
      startDate: "2024-02-29",
      endDate: "2024-03-01",
    });
  });

  it("rejects missing, invalid, and reversed custom dates", () => {
    expect(() => resolveTrendRange("custom", "", "", "2026-07-12")).toThrow("请选择开始和结束日期");
    expect(() => resolveTrendRange("custom", "", "2025-02-29", "2025-03-01")).toThrow("日期无效");
    expect(() => resolveTrendRange("custom", "", "2026-07-12", "2026-07-01")).toThrow("开始日期不能晚于结束日期");
  });

  it("allows 366 inclusive days and rejects 367", () => {
    expect(resolveTrendRange("custom", "", "2024-01-01", "2024-12-31")).toEqual({
      startDate: "2024-01-01",
      endDate: "2024-12-31",
    });
    expect(() => resolveTrendRange("custom", "", "2024-01-01", "2025-01-01")).toThrow("最多选择 366 天");
  });
});

describe("trend range navigation", () => {
  it("moves by exactly the current inclusive range length", () => {
    const range = { startDate: "2026-07-06", endDate: "2026-07-12" };
    expect(shiftTrendRange(range, -1)).toEqual({ startDate: "2026-06-29", endDate: "2026-07-05" });
    expect(shiftTrendRange(range, 1)).toEqual({ startDate: "2026-07-13", endDate: "2026-07-19" });
  });

  it("supports arrow, Home, and End keys for the segmented presets", () => {
    expect(moveTrendPreset("week", "ArrowRight")).toBe("month");
    expect(moveTrendPreset("week", "ArrowLeft")).toBe("custom");
    expect(moveTrendPreset("custom", "Home")).toBe("week");
    expect(moveTrendPreset("week", "End")).toBe("custom");
    expect(moveTrendPreset("month", "Enter")).toBeNull();
  });
});

describe("defaultTrendGranularity", () => {
  it.each([
    ["2026-01-01", "2026-01-31", "day"],
    ["2026-01-01", "2026-02-01", "week"],
    ["2026-01-01", "2026-04-30", "week"],
    ["2026-01-01", "2026-05-01", "month"],
    ["2024-01-01", "2024-12-31", "month"],
  ] as const)("uses the documented range boundary for %s through %s", (startDate, endDate, expected) => {
    expect(defaultTrendGranularity({ startDate, endDate })).toBe(expected);
  });

  it("rejects an invalid range instead of silently selecting a granularity", () => {
    expect(() => defaultTrendGranularity({ startDate: "2026-01-02", endDate: "2026-01-01" })).toThrow(
      "开始日期不能晚于结束日期",
    );
  });
});

describe("specific-date trend selection", () => {
  it("normalizes selected dates to a sorted unique list and resolves its envelope", () => {
    expect(normalizeTrendSelectedDates(["2026-07-12", "2026-07-06", "2026-07-12", "2026-07-08"])).toEqual([
      "2026-07-06",
      "2026-07-08",
      "2026-07-12",
    ]);
    expect(resolveTrendSelectedDates(["2026-07-12", "2026-07-06", "2026-07-08"])).toEqual({
      range: { startDate: "2026-07-06", endDate: "2026-07-12" },
      selectedDates: ["2026-07-06", "2026-07-08", "2026-07-12"],
    });
  });

  it("requires one date and rejects an envelope longer than 366 days", () => {
    expect(() => resolveTrendSelectedDates([])).toThrow("请至少选择 1 天");
    expect(() => resolveTrendSelectedDates(["2024-01-01", "2025-01-01"])).toThrow("日期跨度最多 366 天");
  });

  it("derives default granularity from selected date count rather than envelope length", () => {
    expect(defaultTrendGranularityForSelectedDates(["2026-01-01", "2026-12-31"])).toBe("day");
    expect(defaultTrendGranularityForSelectedDates([
      ...Array.from({ length: 31 }, (_, index) => `2026-01-${String(index + 1).padStart(2, "0")}`),
      "2026-02-01",
    ])).toBe("week");
  });

  it("shifts every selected date by the inclusive envelope span while preserving its shape", () => {
    expect(shiftTrendSelectedDates(["2026-07-06", "2026-07-08", "2026-07-12"], 1)).toEqual([
      "2026-07-13",
      "2026-07-15",
      "2026-07-19",
    ]);
    expect(shiftTrendSelectedDates(["2026-07-06", "2026-07-08", "2026-07-12"], -1)).toEqual([
      "2026-06-29",
      "2026-07-01",
      "2026-07-05",
    ]);
  });
});
