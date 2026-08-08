import type { TrendMetric } from "../../lib/desktop";

const durationMetrics = new Set<TrendMetric>([
  "monitoredSeconds",
  "activeSeconds",
  "learningSeconds",
  "idleSeconds",
  "longestFocusSeconds",
  "linkedTaskSeconds",
]);

function formatTick(value: number) {
  return new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 2 }).format(value);
}

export function chartValue(value: number, metric: TrendMetric) {
  if (durationMetrics.has(metric)) return value / 3600;
  if (metric === "classificationCoverage") return value * 100;
  return value;
}

export function chartYAxis(metric: TrendMetric) {
  if (durationMetrics.has(metric)) {
    return { type: "value" as const, min: 0, name: "小时", axisLabel: { formatter: formatTick } };
  }
  if (metric === "classificationCoverage") {
    return { type: "value" as const, min: 0, max: 100, name: "百分比", axisLabel: { formatter: (value: number) => `${formatTick(value)}%` } };
  }
  return { type: "value" as const, min: 0, name: metric === "switchCount" ? "次" : "个", axisLabel: { formatter: formatTick } };
}
