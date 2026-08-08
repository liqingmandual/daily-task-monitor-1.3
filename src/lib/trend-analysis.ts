import type { TrendDay, TrendMetric, TrendPayload, TrendWorkbenchPayload } from "./desktop";
import type { ActivityScope } from "./activity-composition";

export type TrendDirection = "increasing" | "decreasing" | "stable" | "unavailable" | "insufficient";
export type FocusConsistency = "consistent" | "variable" | "insufficient";
export type ClassificationQuality = "high" | "moderate" | "limited";
export type TrendEvidenceKey =
  | "effectiveActivityDayCount"
  | "minimumEffectiveActivityDayCount"
  | "activeCoefficientOfVariation"
  | "focusCoefficientOfVariation"
  | "learningDeltaPercent"
  | "activeDeltaPercent"
  | "switchingDeltaPercent"
  | "classificationCoverage"
  | "directionThresholdPercent"
  | "selectedMean"
  | "selectedMedian"
  | "selectedSampleStandardDeviation"
  | "selectedCoefficientOfVariation";

export interface TrendStatisticsDto {
  metric: TrendMetric;
  mean: number;
  median: number;
  sampleStandardDeviation: number | null;
  coefficientOfVariation: number | null;
  effectiveActivityDayCount: number;
  classificationCoverage: number;
}

export interface TrendObservation {
  text: string;
  evidenceKeys: TrendEvidenceKey[];
}

export interface RecordedDayExtreme {
  date: string;
  activeSeconds: number;
}

export interface TrendLocalAnalysis {
  status: "ready" | "insufficient";
  observations: TrendObservation[];
  suggestions: string[];
  statistics: {
    selectedMetric: TrendMetric;
    selectedMean: number;
    selectedMedian: number;
    selectedSampleStandardDeviation: number | null;
    selectedCoefficientOfVariation: number | null;
    activeMeanSeconds: number;
    activeMedianSeconds: number;
    activeSampleStandardDeviationSeconds: number | null;
    activeCoefficientOfVariation: number | null;
    learningMeanSeconds: number;
    learningMedianSeconds: number;
    learningSampleStandardDeviationSeconds: number | null;
    learningCoefficientOfVariation: number | null;
    focusSampleStandardDeviationSeconds: number | null;
    focusCoefficientOfVariation: number | null;
    bestEffectiveActivityDay: RecordedDayExtreme | null;
    weakestEffectiveActivityDay: RecordedDayExtreme | null;
    learningDeltaPercent: number | null;
    activeDeltaPercent: number | null;
    switchingDeltaPercent: number | null;
    learningDirection: TrendDirection;
    activeDirection: TrendDirection;
    switchingPressure: TrendDirection;
    focusConsistency: FocusConsistency;
    classificationCoverage: number;
    classificationQuality: ClassificationQuality;
    effectiveActivityDayCount: number;
    minimumEffectiveActivityDayCount: number;
    directionThresholdPercent: number;
  };
}

interface DescriptiveStatistics {
  mean: number;
  median: number;
  sampleStandardDeviation: number | null;
  coefficientOfVariation: number | null;
}

function descriptiveStatistics(values: number[]): DescriptiveStatistics {
  if (values.length === 0) {
    return { mean: 0, median: 0, sampleStandardDeviation: null, coefficientOfVariation: null };
  }
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
  if (values.length < 2) {
    return { mean, median, sampleStandardDeviation: null, coefficientOfVariation: null };
  }
  const variance = values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / (values.length - 1);
  const sampleStandardDeviation = Math.sqrt(variance);
  return {
    mean,
    median,
    sampleStandardDeviation,
    coefficientOfVariation: mean === 0 ? null : sampleStandardDeviation / mean,
  };
}

const DIRECTION_THRESHOLD_PERCENT = 8;
const MINIMUM_EFFECTIVE_ACTIVITY_DAYS = 3;

function direction(delta: number | null, enoughEvidence: boolean): TrendDirection {
  if (!enoughEvidence) return "insufficient";
  if (delta === null) return "unavailable";
  if (delta >= DIRECTION_THRESHOLD_PERCENT) return "increasing";
  if (delta <= -DIRECTION_THRESHOLD_PERCENT) return "decreasing";
  return "stable";
}

function directionLabel(value: TrendDirection): string {
  if (value === "increasing") return "增加";
  if (value === "decreasing") return "减少";
  if (value === "unavailable") return "不可用";
  return "稳定";
}

function deltaEvidence(
  label: string,
  delta: number | null,
  value: TrendDirection,
  deltaKey: "learningDeltaPercent" | "activeDeltaPercent",
): TrendObservation {
  const evidenceKeys: TrendEvidenceKey[] = [deltaKey, "directionThresholdPercent"];
  if (delta === null) return { text: `${label}较上期无可比基数，方向阈值为 ${DIRECTION_THRESHOLD_PERCENT}%。`, evidenceKeys };
  const signed = `${delta > 0 ? "+" : ""}${delta.toFixed(1)}%`;
  return { text: `${label}较上期 ${signed}，按 ${DIRECTION_THRESHOLD_PERCENT}% 阈值判定为${directionLabel(value)}。`, evidenceKeys };
}

function extreme(days: TrendDay[], mode: "best" | "weakest"): RecordedDayExtreme | null {
  if (days.length === 0) return null;
  const selected = days.reduce((current, candidate) => {
    if (mode === "best") return candidate.activeSeconds > current.activeSeconds ? candidate : current;
    return candidate.activeSeconds < current.activeSeconds ? candidate : current;
  });
  return { date: selected.date, activeSeconds: selected.activeSeconds };
}

export function buildTrendStatisticsDto(
  payload: TrendWorkbenchPayload,
  activityScope: ActivityScope = "all",
): TrendStatisticsDto {
  if (activityScope === "meaningful" && !payload.analysisSummary) {
    const values = payload.buckets.map((bucket) => (
      bucket.activityComposition?.meaningful.totalSeconds ?? 0
    ));
    const statistics = descriptiveStatistics(values);
    return {
      metric: "activeSeconds",
      mean: statistics.mean,
      median: statistics.median,
      sampleStandardDeviation: statistics.sampleStandardDeviation,
      coefficientOfVariation: statistics.coefficientOfVariation,
      effectiveActivityDayCount: values.filter((value) => value > 0).length,
      classificationCoverage: values.some((value) => value > 0) ? 1 : 0,
    };
  }
  const summary = activityScope === "meaningful"
    ? (payload.analysisSummary ?? payload.summary)
    : payload.summary;
  const metric = payload.metric;
  const effectiveActivityDayCount = summary.effectiveActivityDayCount;
  return {
    metric,
    mean: summary.meanPerBucket[metric],
    median: summary.dailyMedian[metric],
    sampleStandardDeviation: summary.dailySampleStddev[metric],
    coefficientOfVariation: summary.dailyCoefficientOfVariation[metric],
    effectiveActivityDayCount,
    classificationCoverage: summary.classificationCoverage,
  };
}

function buildLocalTrendAnalysisFromStatistics(input: TrendStatisticsDto): TrendLocalAnalysis {
  const enoughEvidence = input.effectiveActivityDayCount >= MINIMUM_EFFECTIVE_ACTIVITY_DAYS;
  const classificationQuality: ClassificationQuality = input.classificationCoverage >= 0.9
    ? "high"
    : input.classificationCoverage >= 0.7
      ? "moderate"
      : "limited";
  const statistics: TrendLocalAnalysis["statistics"] = {
    selectedMetric: input.metric,
    selectedMean: input.mean,
    selectedMedian: input.median,
    selectedSampleStandardDeviation: input.sampleStandardDeviation,
    selectedCoefficientOfVariation: input.coefficientOfVariation,
    activeMeanSeconds: input.metric === "activeSeconds" ? input.mean : 0,
    activeMedianSeconds: input.metric === "activeSeconds" ? input.median : 0,
    activeSampleStandardDeviationSeconds: input.metric === "activeSeconds" ? input.sampleStandardDeviation : null,
    activeCoefficientOfVariation: input.metric === "activeSeconds" ? input.coefficientOfVariation : null,
    learningMeanSeconds: input.metric === "learningSeconds" ? input.mean : 0,
    learningMedianSeconds: input.metric === "learningSeconds" ? input.median : 0,
    learningSampleStandardDeviationSeconds: input.metric === "learningSeconds" ? input.sampleStandardDeviation : null,
    learningCoefficientOfVariation: input.metric === "learningSeconds" ? input.coefficientOfVariation : null,
    focusSampleStandardDeviationSeconds: input.metric === "longestFocusSeconds" ? input.sampleStandardDeviation : null,
    focusCoefficientOfVariation: input.metric === "longestFocusSeconds" ? input.coefficientOfVariation : null,
    bestEffectiveActivityDay: null,
    weakestEffectiveActivityDay: null,
    learningDeltaPercent: null,
    activeDeltaPercent: null,
    switchingDeltaPercent: null,
    learningDirection: enoughEvidence ? "unavailable" : "insufficient",
    activeDirection: enoughEvidence ? "unavailable" : "insufficient",
    switchingPressure: enoughEvidence ? "unavailable" : "insufficient",
    focusConsistency: "insufficient",
    classificationCoverage: input.classificationCoverage,
    classificationQuality,
    effectiveActivityDayCount: input.effectiveActivityDayCount,
    minimumEffectiveActivityDayCount: MINIMUM_EFFECTIVE_ACTIVITY_DAYS,
    directionThresholdPercent: DIRECTION_THRESHOLD_PERCENT,
  };
  if (!enoughEvidence) {
    return {
      status: "insufficient",
      statistics,
      observations: [{
        text: `仅有 ${input.effectiveActivityDayCount} 个有效活动日，少于统计解释所需的 ${MINIMUM_EFFECTIVE_ACTIVITY_DAYS} 天。`,
        evidenceKeys: ["effectiveActivityDayCount", "minimumEffectiveActivityDayCount"],
      }],
      suggestions: [],
    };
  }
  return {
    status: "ready",
    statistics,
    observations: [{
      text: `当前指标均值为 ${input.mean.toFixed(2)}，中位数为 ${input.median.toFixed(2)}。`,
      evidenceKeys: ["selectedMean", "selectedMedian"],
    }],
    suggestions: [],
  };
}

export function buildLocalTrendAnalysis(payload: TrendPayload): TrendLocalAnalysis;
export function buildLocalTrendAnalysis(statistics: TrendStatisticsDto): TrendLocalAnalysis;
export function buildLocalTrendAnalysis(payload: TrendPayload | TrendStatisticsDto): TrendLocalAnalysis {
  if (!("days" in payload)) return buildLocalTrendAnalysisFromStatistics(payload);
  const effectiveActivityDays = payload.days.filter((item) => item.activeSeconds > 0);
  const effectiveActivityDayCount = effectiveActivityDays.length;
  const enoughEvidence = effectiveActivityDayCount >= MINIMUM_EFFECTIVE_ACTIVITY_DAYS;
  const active = descriptiveStatistics(effectiveActivityDays.map((item) => item.activeSeconds));
  const learning = descriptiveStatistics(effectiveActivityDays.map((item) => item.learningSeconds));
  const focus = descriptiveStatistics(effectiveActivityDays.map((item) => item.longestFocusSeconds));
  const learningDirection = direction(payload.comparison.learningSecondsDeltaPercent, enoughEvidence);
  const activeDirection = direction(payload.comparison.activeSecondsDeltaPercent, enoughEvidence);
  const switchingPressure = direction(payload.comparison.switchesPerActiveHourDeltaPercent, enoughEvidence);
  const focusConsistency: FocusConsistency = !enoughEvidence || focus.coefficientOfVariation === null
    ? "insufficient"
    : focus.coefficientOfVariation <= 0.25
      ? "consistent"
      : "variable";
  const classificationCoverage = payload.quality.classificationCoverage;
  const classificationQuality: ClassificationQuality = classificationCoverage >= 0.9
    ? "high"
    : classificationCoverage >= 0.7
      ? "moderate"
      : "limited";

  const statistics: TrendLocalAnalysis["statistics"] = {
    selectedMetric: "activeSeconds",
    selectedMean: active.mean,
    selectedMedian: active.median,
    selectedSampleStandardDeviation: active.sampleStandardDeviation,
    selectedCoefficientOfVariation: active.coefficientOfVariation,
    activeMeanSeconds: active.mean,
    activeMedianSeconds: active.median,
    activeSampleStandardDeviationSeconds: active.sampleStandardDeviation,
    activeCoefficientOfVariation: active.coefficientOfVariation,
    learningMeanSeconds: learning.mean,
    learningMedianSeconds: learning.median,
    learningSampleStandardDeviationSeconds: learning.sampleStandardDeviation,
    learningCoefficientOfVariation: learning.coefficientOfVariation,
    focusSampleStandardDeviationSeconds: focus.sampleStandardDeviation,
    focusCoefficientOfVariation: focus.coefficientOfVariation,
    bestEffectiveActivityDay: extreme(effectiveActivityDays, "best"),
    weakestEffectiveActivityDay: extreme(effectiveActivityDays, "weakest"),
    learningDeltaPercent: payload.comparison.learningSecondsDeltaPercent,
    activeDeltaPercent: payload.comparison.activeSecondsDeltaPercent,
    switchingDeltaPercent: payload.comparison.switchesPerActiveHourDeltaPercent,
    learningDirection,
    activeDirection,
    switchingPressure,
    focusConsistency,
    classificationCoverage,
    classificationQuality,
    effectiveActivityDayCount,
    minimumEffectiveActivityDayCount: MINIMUM_EFFECTIVE_ACTIVITY_DAYS,
    directionThresholdPercent: DIRECTION_THRESHOLD_PERCENT,
  };

  if (!enoughEvidence) {
    return {
      status: "insufficient",
      statistics,
      observations: [{
        text: `仅有 ${effectiveActivityDayCount} 个有效活动日，少于方向判断所需的 ${MINIMUM_EFFECTIVE_ACTIVITY_DAYS} 天。`,
        evidenceKeys: ["effectiveActivityDayCount", "minimumEffectiveActivityDayCount"],
      }],
      suggestions: ["尝试先记录至少 3 个活动日，再比较学习与活跃时长的变化。"],
    };
  }

  const activeCv = active.coefficientOfVariation ?? 0;
  const focusCv = focus.coefficientOfVariation;
  const switchingDelta = payload.comparison.switchesPerActiveHourDeltaPercent;
  const observations: TrendObservation[] = [
    {
      text: `${effectiveActivityDayCount} 个有效活动日的活跃时长变异系数为 ${activeCv.toFixed(2)}。`,
      evidenceKeys: ["effectiveActivityDayCount", "activeCoefficientOfVariation"],
    },
    {
      text: focusCv === null
        ? "专注时长变异系数暂无，当前缺少可计算的专注时长离散证据。"
        : `专注时长变异系数 ${focusCv.toFixed(2)}，专注时长${focusConsistency === "consistent" ? "较一致" : "波动较大"}。`,
      evidenceKeys: ["focusCoefficientOfVariation"],
    },
    deltaEvidence("学习时长", payload.comparison.learningSecondsDeltaPercent, learningDirection, "learningDeltaPercent"),
    deltaEvidence("活跃时长", payload.comparison.activeSecondsDeltaPercent, activeDirection, "activeDeltaPercent"),
    {
      text: switchingDelta === null
        ? `每小时切换较上期无可比基数，方向阈值为 ${DIRECTION_THRESHOLD_PERCENT}%。`
        : `每小时切换较上期 ${switchingDelta > 0 ? "+" : ""}${switchingDelta.toFixed(1)}%，按 ${DIRECTION_THRESHOLD_PERCENT}% 阈值判定为${directionLabel(switchingPressure)}。`,
      evidenceKeys: ["switchingDeltaPercent", "directionThresholdPercent"],
    },
    {
      text: `分类覆盖 ${(classificationCoverage * 100).toFixed(0)}%，当前质量等级为${classificationQuality === "high" ? "高" : classificationQuality === "moderate" ? "中" : "有限"}。`,
      evidenceKeys: ["classificationCoverage"],
    },
  ];

  const suggestions: string[] = [];
  if (focusConsistency === "variable") {
    suggestions.push(`尝试连续 7 天记录最长专注时长，并观察专注时长变异系数 ${focusCv?.toFixed(2)} 是否下降。`);
  } else if (focusConsistency === "consistent") {
    suggestions.push(`保持记录最长专注时长 7 天，并观察专注时长变异系数 ${focusCv?.toFixed(2)} 是否继续不高于 0.25。`);
  } else {
    suggestions.push("尝试记录至少 3 天的最长专注时长，再比较专注时长离散程度。");
  }
  if (switchingPressure === "increasing") {
    suggestions.push("尝试在下个周期减少一个高频通知来源，并记录每小时切换次数。");
  }
  if (learningDirection === "decreasing") {
    suggestions.push("尝试每天安排一个固定学习时段，并记录下个周期的学习总时长。");
  }
  if (classificationQuality !== "high") {
    suggestions.push("尝试补充分类型规则，并记录分类覆盖是否达到 90%。");
  }

  return { status: "ready", statistics, observations, suggestions };
}
