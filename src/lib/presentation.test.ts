import { describe, expect, it } from "vitest";
import {
  consumePendingTimelineScroll,
  buildTimeLayerSelection,
  formatChartDuration,
  formatDonutTooltip,
} from "./presentation";

describe("chart presentation", () => {
  it("never renders raw seconds in chart durations", () => {
    expect(formatChartDuration(42)).toBe("不足 1 分钟");
    expect(formatChartDuration(3_660)).toBe("1 小时 1 分钟");
    expect(formatDonutTooltip("搜索/调研", 3_660, 25)).toBe("搜索/调研\n1 小时 1 分钟 · 25.0%");
  });

  it("consumes a pending timeline scroll exactly once after the dashboard remounts", () => {
    const whileGraphIsOpen = consumePendingTimelineScroll(4, false);
    expect(whileGraphIsOpen).toEqual({ pendingRequest: 4, shouldScroll: false });

    const afterDashboardRemount = consumePendingTimelineScroll(whileGraphIsOpen.pendingRequest, true);
    expect(afterDashboardRemount).toEqual({ pendingRequest: null, shouldScroll: true });

    expect(consumePendingTimelineScroll(afterDashboardRemount.pendingRequest, true)).toEqual({
      pendingRequest: null,
      shouldScroll: false,
    });
  });

  it("builds the same click-selection model for a time-bar layer", () => {
    const selection = buildTimeLayerSelection({
      key: "08-10",
      range: "08-10",
      startHour: 8,
      endHour: 10,
      activeSeconds: 5_400,
      learningSeconds: 3_600,
      otherActiveSeconds: 1_800,
      idleSeconds: 1_800,
      categorySeconds: {},
      appSeconds: { Chrome: 5_400 },
    }, "learning");

    expect(selection).toEqual({ name: "08-10 学习时间", seconds: 3_600, percent: 50 });
  });
});
