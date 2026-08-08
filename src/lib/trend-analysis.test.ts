import { describe, expect, it } from "vitest";
import type { TrendDay, TrendPayload, TrendWorkbenchPayload } from "./desktop";
import {
  buildLocalTrendAnalysis,
  buildTrendStatisticsDto,
  type TrendStatisticsDto,
} from "./trend-analysis";

function statisticalValues(overrides: Record<string, number> = {}) {
  return {
    monitoredSeconds: 0,
    activeSeconds: 0,
    learningSeconds: 0,
    idleSeconds: 0,
    switchCount: 0,
    longestFocusSeconds: 0,
    classificationCoverage: 0,
    completedTaskCount: 0,
    linkedTaskSeconds: 0,
    ...overrides,
  };
}

function workbenchStatistics(
  metric: TrendWorkbenchPayload["metric"],
  overrides: Record<string, unknown> = {},
): TrendWorkbenchPayload {
  return {
    metric,
    summary: {
      totals: statisticalValues({ classificationCoverage: 0.99, longestFocusSeconds: 9_999 }),
      meanPerBucket: statisticalValues({ classificationCoverage: 0.42, longestFocusSeconds: 222 }),
      dailyMedian: statisticalValues({ classificationCoverage: 0.4, longestFocusSeconds: 200 }),
      dailyMax: statisticalValues(),
      dailySampleStddev: statisticalValues({ classificationCoverage: 0.08, longestFocusSeconds: 25 }),
      dailyCoefficientOfVariation: statisticalValues({ classificationCoverage: 0.19, longestFocusSeconds: 0.11 }),
      recordedDayCount: 9,
      effectiveActivityDayCount: 3,
      missingDayCount: 0,
      classifiedSeconds: 1,
      classificationCoverage: 0.77,
      lowConfidenceSeconds: 0,
      pendingSeconds: 0,
      evidenceIds: [],
      ...overrides,
    },
  } as unknown as TrendWorkbenchPayload;
}

function day(
  date: string,
  activeSeconds: number,
  learningSeconds: number,
  overrides: Partial<TrendDay> = {},
): TrendDay {
  return {
    date,
    label: date,
    monitoredSeconds: activeSeconds,
    activeSeconds,
    idleSeconds: 0,
    learningSeconds,
    switchCount: 0,
    longestFocusSeconds: activeSeconds,
    classificationCoverage: activeSeconds > 0 ? 1 : 0,
    topCategory: null,
    topApp: null,
    ...overrides,
  };
}

function payload(
  days: TrendDay[],
  overrides: {
    learningDelta?: number | null;
    activeDelta?: number | null;
    switchingDelta?: number | null;
    classificationCoverage?: number;
  } = {},
): TrendPayload {
  const activeSeconds = days.reduce((total, item) => total + item.activeSeconds, 0);
  const learningSeconds = days.reduce((total, item) => total + item.learningSeconds, 0);
  const recordedDayCount = days.filter((item) => item.monitoredSeconds > 0).length;
  const coverage = overrides.classificationCoverage ?? 0.9;
  return {
    range: { startMs: 0, endMs: 1, startDate: "2026-07-01", endDate: "2026-07-07", dayCount: days.length },
    days,
    summary: {
      monitoredSeconds: activeSeconds,
      activeSeconds,
      idleSeconds: 0,
      learningSeconds,
      switchCount: 0,
      longestFocusSeconds: Math.max(0, ...days.map((item) => item.longestFocusSeconds)),
      averageMonitoredSeconds: days.length ? activeSeconds / days.length : 0,
      averageActiveSeconds: days.length ? activeSeconds / days.length : 0,
      averageIdleSeconds: 0,
      averageLearningSeconds: days.length ? learningSeconds / days.length : 0,
      averageSwitchCount: 0,
      learningRatio: activeSeconds ? learningSeconds / activeSeconds : 0,
      switchesPerActiveHour: 2,
      productiveDayCount: recordedDayCount,
      focusDayCount: recordedDayCount,
      categoryBreakdown: [],
      appBreakdown: [],
    },
    comparison: {
      previousRange: { startMs: -1, endMs: 0, startDate: "2026-06-24", endDate: "2026-06-30", dayCount: days.length },
      dayCount: days.length,
      previousMonitoredSeconds: 0,
      previousActiveSeconds: 0,
      previousIdleSeconds: 0,
      previousLearningSeconds: 0,
      previousSwitchCount: 0,
      previousLongestFocusSeconds: 0,
      previousLearningRatio: 0,
      previousSwitchesPerActiveHour: 2,
      previousClassificationCoverage: 0.9,
      previousCategoryBreakdown: [],
      previousAppBreakdown: [],
      monitoredSecondsDeltaPercent: null,
      activeSecondsDeltaPercent: overrides.activeDelta === undefined ? 0 : overrides.activeDelta,
      idleSecondsDeltaPercent: null,
      learningSecondsDeltaPercent: overrides.learningDelta === undefined ? 0 : overrides.learningDelta,
      switchCountDeltaPercent: null,
      longestFocusSecondsDeltaPercent: null,
      learningRatioDeltaPercent: null,
      switchesPerActiveHourDeltaPercent: overrides.switchingDelta === undefined ? 0 : overrides.switchingDelta,
      classificationCoverageDeltaPercent: 0,
    },
    quality: {
      recordedDayCount,
      missingDayCount: days.length - recordedDayCount,
      classifiedSeconds: activeSeconds * coverage,
      pendingSeconds: activeSeconds * (1 - coverage),
      lowConfidenceSeconds: 0,
      classificationCoverage: coverage,
    },
    workLedger: { startMs: 0, endMs: 1, projects: [], tasks: [] },
    evidenceHash: "test-evidence",
  };
}

const threeDays = [
  day("2026-07-01", 100, 20),
  day("2026-07-02", 200, 40),
  day("2026-07-03", 300, 60),
];

describe("buildTrendStatisticsDto", () => {
  it("uses authoritative workbench fields for classification coverage", () => {
    const result = buildTrendStatisticsDto(workbenchStatistics("classificationCoverage"));

    expect(result).toEqual({
      metric: "classificationCoverage",
      mean: 0.42,
      median: 0.4,
      sampleStandardDeviation: 0.08,
      coefficientOfVariation: 0.19,
      effectiveActivityDayCount: 3,
      classificationCoverage: 0.77,
    });
  });

  it("does not derive longest-focus statistics from totals or recorded days", () => {
    const result = buildTrendStatisticsDto(workbenchStatistics("longestFocusSeconds"));

    expect(result.mean).toBe(222);
    expect(result.median).toBe(200);
    expect(result.sampleStandardDeviation).toBe(25);
    expect(result.coefficientOfVariation).toBe(0.11);
    expect(result.effectiveActivityDayCount).toBe(3);
  });

  it("uses the scope-specific summary for meaningful-activity local facts", () => {
    const full = workbenchStatistics("activeSeconds");
    const scoped = workbenchStatistics("activeSeconds", {
      effectiveActivityDayCount: 2,
      classificationCoverage: 1,
    }).summary;
    scoped.meanPerBucket.activeSeconds = 600;
    scoped.dailyMedian.activeSeconds = 540;
    scoped.dailySampleStddev.activeSeconds = 90;
    scoped.dailyCoefficientOfVariation.activeSeconds = 0.15;
    full.analysisActivityScope = "meaningful";
    full.analysisSummary = scoped;

    expect(buildTrendStatisticsDto(full, "meaningful")).toEqual({
      metric: "activeSeconds",
      mean: 600,
      median: 540,
      sampleStandardDeviation: 90,
      coefficientOfVariation: 0.15,
      effectiveActivityDayCount: 2,
      classificationCoverage: 1,
    });
  });
});

describe("buildLocalTrendAnalysis", () => {
  it("consumes the workbench statistics DTO as the factual layer", () => {
    const statistics: TrendStatisticsDto = {
      metric: "activeSeconds",
      mean: 1_200,
      median: 1_100,
      sampleStandardDeviation: 200,
      coefficientOfVariation: 1 / 6,
      effectiveActivityDayCount: 5,
      classificationCoverage: 0.84,
    };

    const result = buildLocalTrendAnalysis(statistics);

    expect(result.status).toBe("ready");
    expect(result.statistics.selectedMetric).toBe("activeSeconds");
    expect(result.statistics.selectedMean).toBe(1_200);
    expect(result.statistics.selectedMedian).toBe(1_100);
    expect(result.statistics.selectedSampleStandardDeviation).toBe(200);
    expect(result.statistics.selectedCoefficientOfVariation).toBeCloseTo(1 / 6);
    expect(result.statistics.effectiveActivityDayCount).toBe(5);
  });

  it("keeps factual statistics visible when the workbench sample is insufficient", () => {
    const result = buildLocalTrendAnalysis({
      metric: "learningSeconds",
      mean: 600,
      median: 600,
      sampleStandardDeviation: null,
      coefficientOfVariation: null,
      effectiveActivityDayCount: 2,
      classificationCoverage: 0.7,
    } satisfies TrendStatisticsDto);

    expect(result.status).toBe("insufficient");
    expect(result.statistics.selectedMean).toBe(600);
    expect(result.statistics.selectedMedian).toBe(600);
    expect(result.observations[0].evidenceKeys).toEqual([
      "effectiveActivityDayCount",
      "minimumEffectiveActivityDayCount",
    ]);
  });

  it("returns insufficient evidence with fewer than three recorded activity days", () => {
    const result = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 100, 20),
      day("2026-07-02", 0, 0),
      day("2026-07-03", 200, 40),
    ], { learningDelta: 30, activeDelta: -30 }));

    expect(result.status).toBe("insufficient");
    expect(result.statistics.effectiveActivityDayCount).toBe(2);
    expect(result.statistics.learningDirection).toBe("insufficient");
    expect(result.statistics.activeDirection).toBe("insufficient");
    expect(result.observations.map((item) => item.text).join(" ")).toContain("2 个有效活动日");
  });

  it("does not count a monitored but fully inactive day as an activity day", () => {
    const inactive = day("2026-07-03", 0, 0, { monitoredSeconds: 300, idleSeconds: 300 });
    const result = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 100, 20),
      day("2026-07-02", 200, 40),
      inactive,
    ], { learningDelta: 30 }));

    expect(result.status).toBe("insufficient");
    expect(result.statistics.effectiveActivityDayCount).toBe(2);
    expect(result.statistics.learningDirection).toBe("insufficient");
  });

  it("links insufficient evidence to both the activity count and displayed minimum sample", () => {
    const result = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 100, 20),
      day("2026-07-02", 200, 40),
    ]));

    expect(result.statistics.minimumEffectiveActivityDayCount).toBe(3);
    expect(result.observations[0].evidenceKeys).toEqual([
      "effectiveActivityDayCount",
      "minimumEffectiveActivityDayCount",
    ]);
  });

  it.each([
    [8, "increasing"],
    [-8, "decreasing"],
    [7.99, "stable"],
    [-7.99, "stable"],
    [null, "unavailable"],
  ] as const)("classifies a learning delta of %s as %s", (delta, direction) => {
    const result = buildLocalTrendAnalysis(payload(threeDays, { learningDelta: delta }));
    expect(result.statistics.learningDirection).toBe(direction);
  });

  it("classifies active learning and switching deltas independently", () => {
    const result = buildLocalTrendAnalysis(payload(threeDays, {
      learningDelta: 12.5,
      activeDelta: -9,
      switchingDelta: 10,
    }));

    expect(result.statistics.learningDeltaPercent).toBe(12.5);
    expect(result.statistics.learningDirection).toBe("increasing");
    expect(result.statistics.activeDirection).toBe("decreasing");
    expect(result.statistics.switchingPressure).toBe("increasing");
  });

  it("marks every unavailable comparison direction consistently", () => {
    const result = buildLocalTrendAnalysis(payload(threeDays, {
      learningDelta: null,
      activeDelta: null,
      switchingDelta: null,
    }));

    expect(result.statistics.learningDirection).toBe("unavailable");
    expect(result.statistics.activeDirection).toBe("unavailable");
    expect(result.statistics.switchingPressure).toBe("unavailable");
    expect(result.observations.filter((item) => item.text.includes("较上期")).map((item) => item.text))
      .toEqual(expect.arrayContaining([
        expect.stringContaining("学习时长较上期无可比基数"),
        expect.stringContaining("活跃时长较上期无可比基数"),
        expect.stringContaining("每小时切换较上期无可比基数"),
      ]));
    expect(result.observations.filter((item) => item.text.includes("无可比基数")).map((item) => item.text).join(" ")).not.toContain("稳定");
  });

  it("calculates means medians sample deviations and coefficients of variation", () => {
    const result = buildLocalTrendAnalysis(payload(threeDays));

    expect(result.statistics.activeMeanSeconds).toBe(200);
    expect(result.statistics.activeMedianSeconds).toBe(200);
    expect(result.statistics.activeSampleStandardDeviationSeconds).toBe(100);
    expect(result.statistics.activeCoefficientOfVariation).toBe(0.5);
    expect(result.statistics.learningMeanSeconds).toBe(40);
    expect(result.statistics.learningMedianSeconds).toBe(40);
    expect(result.statistics.learningSampleStandardDeviationSeconds).toBe(20);
    expect(result.statistics.learningCoefficientOfVariation).toBe(0.5);
  });

  it("uses the midpoint median and identifies deterministic best and weakest recorded days", () => {
    const result = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 400, 40),
      day("2026-07-02", 100, 10),
      day("2026-07-03", 300, 30),
      day("2026-07-04", 200, 20),
    ]));

    expect(result.statistics.activeMedianSeconds).toBe(250);
    expect(result.statistics.bestEffectiveActivityDay).toEqual({ date: "2026-07-01", activeSeconds: 400 });
    expect(result.statistics.weakestEffectiveActivityDay).toEqual({ date: "2026-07-02", activeSeconds: 100 });
    expect(result.statistics).not.toHaveProperty("bestRecordedDay");
    expect(result.statistics).not.toHaveProperty("weakestRecordedDay");
  });

  it("derives focus consistency from longest focus duration rather than active duration", () => {
    const consistent = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 100, 20, { longestFocusSeconds: 50 }),
      day("2026-07-02", 100, 20, { longestFocusSeconds: 50 }),
      day("2026-07-03", 100, 20, { longestFocusSeconds: 50 }),
    ]));
    const variable = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 100, 20, { longestFocusSeconds: 10 }),
      day("2026-07-02", 100, 20, { longestFocusSeconds: 50 }),
      day("2026-07-03", 100, 20, { longestFocusSeconds: 90 }),
    ]));

    expect(consistent.statistics.focusConsistency).toBe("consistent");
    expect(consistent.statistics.activeCoefficientOfVariation).toBe(0);
    expect(consistent.statistics.focusCoefficientOfVariation).toBe(0);
    expect(variable.statistics.focusConsistency).toBe("variable");
    expect(variable.statistics.activeCoefficientOfVariation).toBe(0);
    expect(variable.statistics.focusCoefficientOfVariation).toBe(0.8);
    expect(variable.observations.map((item) => item.text).join(" ")).toContain("专注时长变异系数 0.80");
    expect(consistent.suggestions).toEqual([
      expect.stringMatching(/最长专注时长.*专注时长变异系数 0\.00/),
    ]);
    expect(variable.suggestions).toEqual([
      expect.stringMatching(/最长专注时长.*专注时长变异系数 0\.80/),
    ]);
    expect([...consistent.suggestions, ...variable.suggestions].join(" ")).not.toMatch(/活跃|active/i);
  });

  it("keeps focus consistency insufficient when activity days contain no focus evidence", () => {
    const result = buildLocalTrendAnalysis(payload([
      day("2026-07-01", 100, 20, { longestFocusSeconds: 0 }),
      day("2026-07-02", 100, 20, { longestFocusSeconds: 0 }),
      day("2026-07-03", 100, 20, { longestFocusSeconds: 0 }),
    ]));

    expect(result.statistics.focusCoefficientOfVariation).toBeNull();
    expect(result.statistics.focusConsistency).toBe("insufficient");
    expect(result.observations.map((item) => item.text).join(" ")).toContain("专注时长变异系数暂无");
  });

  it.each([
    [0.92, "high"],
    [0.75, "moderate"],
    [0.49, "limited"],
  ] as const)("classifies %.2f coverage as %s quality", (coverage, quality) => {
    const result = buildLocalTrendAnalysis(payload(threeDays, { classificationCoverage: coverage }));
    expect(result.statistics.classificationCoverage).toBe(coverage);
    expect(result.statistics.classificationQuality).toBe(quality);
    expect(result.observations.map((item) => item.text).join(" ")).toContain(`分类覆盖 ${(coverage * 100).toFixed(0)}%`);
  });

  it("handles empty and zero data without NaN or invented extrema", () => {
    const result = buildLocalTrendAnalysis(payload([]));

    expect(result.status).toBe("insufficient");
    expect(result.statistics.activeMeanSeconds).toBe(0);
    expect(result.statistics.activeMedianSeconds).toBe(0);
    expect(result.statistics.activeSampleStandardDeviationSeconds).toBeNull();
    expect(result.statistics.activeCoefficientOfVariation).toBeNull();
    expect(result.statistics.bestEffectiveActivityDay).toBeNull();
    expect(result.statistics.weakestEffectiveActivityDay).toBeNull();
    expect(JSON.stringify(result)).not.toContain("NaN");
  });

  it("keeps observations numerical and suggestions operational without personal judgments or AI wording", () => {
    const result = buildLocalTrendAnalysis(payload(threeDays, {
      learningDelta: 12.5,
      switchingDelta: 15,
      classificationCoverage: 0.75,
    }));
    const text = [...result.observations.map((item) => item.text), ...result.suggestions].join(" ");

    expect(result.observations.every((item) => /\d/.test(item.text))).toBe(true);
    expect(result.observations.every((item) => item.evidenceKeys.length > 0)).toBe(true);
    expect(result.suggestions.length).toBeGreaterThan(0);
    expect(text).not.toMatch(/心情|性格|健康|能力|聪明|懒惰|焦虑|AI|人工智能/i);
    expect(result.suggestions.every((item) => /尝试|记录|安排|减少|保持/.test(item))).toBe(true);
  });

  it("declares the exact statistic keys supporting every observation", () => {
    const result = buildLocalTrendAnalysis(payload(threeDays, {
      learningDelta: 12.5,
      activeDelta: -9,
      switchingDelta: 10,
      classificationCoverage: 0.75,
    }));

    expect(result.observations.map((item) => item.evidenceKeys)).toEqual([
      ["effectiveActivityDayCount", "activeCoefficientOfVariation"],
      ["focusCoefficientOfVariation"],
      ["learningDeltaPercent", "directionThresholdPercent"],
      ["activeDeltaPercent", "directionThresholdPercent"],
      ["switchingDeltaPercent", "directionThresholdPercent"],
      ["classificationCoverage"],
    ]);
  });
});
