import { describe, expect, it } from "vitest";
import type { TimeBucketMetric } from "../../lib/metrics";
import {
  DEFAULT_TIME_SERIES,
  getTimeBarTooltip,
  timeSeriesLabel,
  timeBucketFilter,
  toggleTimeSeries,
} from "./time-distribution-controller";

const bucket: TimeBucketMetric = {
  key: "08-10",
  startHour: 8,
  endHour: 10,
  range: "08-10",
  activeSeconds: 3_600,
  learningSeconds: 900,
  otherActiveSeconds: 2_700,
  idleSeconds: 1_800,
  categorySeconds: { research: 1_200, idle: 1_800 },
  appSeconds: { Chrome: 2_400, Obsidian: 1_200 },
};

describe("time distribution controller", () => {
  it("restores the default series when the final selected series is removed", () => {
    expect(toggleTimeSeries(["active"], "active")).toEqual(DEFAULT_TIME_SERIES);
    expect(toggleTimeSeries(["active", "learning"], "research")).toEqual(["active", "learning", "research"]);
  });

  it("builds tooltip data with window and relevant bucket percentages", () => {
    const learning = getTimeBarTooltip(bucket, "learning");
    const idle = getTimeBarTooltip(bucket, "idle");

    expect(learning).toMatchObject({
      range: "08-10",
      seriesLabel: "学习",
      seconds: 900,
      percentOfWindow: 12.5,
      denominatorLabel: "桶内活跃",
      percentOfDenominator: 25,
    });
    expect(idle).toMatchObject({
      seriesLabel: "不活跃",
      seconds: 1_800,
      percentOfWindow: 25,
      denominatorLabel: "桶内监测",
    });
    expect(idle.percentOfDenominator).toBeCloseTo(33.333, 2);
  });

  it("creates a time-bucket TimelineFilter for the selected series", () => {
    expect(timeBucketFilter(bucket, "research")).toEqual({
      mode: "timeBucket",
      startHour: 8,
      endHour: 10,
      series: "research",
    });
  });

  it("uses the shared display names for all three video purposes", () => {
    expect(timeSeriesLabel("learning_video")).toBe("学习视频");
    expect(timeSeriesLabel("leisure_video")).toBe("休闲视频");
    expect(timeSeriesLabel("unknown_video")).toBe("未分类");
  });
});
