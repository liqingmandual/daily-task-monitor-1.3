import type { TimeBucketMetric } from "./metrics";

export function formatChartDuration(seconds: number): string {
  const totalMinutes = Math.floor(Math.max(0, seconds) / 60);
  if (totalMinutes < 1) return "不足 1 分钟";
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (!hours) return `${minutes} 分钟`;
  if (!minutes) return `${hours} 小时`;
  return `${hours} 小时 ${minutes} 分钟`;
}

export function formatDonutTooltip(name: string, seconds: number, percent: number): string {
  return `${name}\n${formatChartDuration(seconds)} · ${percent.toFixed(1)}%`;
}

export function consumePendingTimelineScroll(pendingRequest: number | null, dashboardReady: boolean) {
  if (pendingRequest === null || !dashboardReady) {
    return { pendingRequest, shouldScroll: false };
  }
  return { pendingRequest: null, shouldScroll: true };
}

export function buildTimeLayerSelection(bucket: TimeBucketMetric, layer: "learning" | "otherActive") {
  const seconds = layer === "learning" ? bucket.learningSeconds : bucket.otherActiveSeconds;
  return {
    name: `${bucket.range} ${layer === "learning" ? "学习时间" : "其他活跃"}`,
    seconds,
    percent: seconds / 7_200 * 100,
  };
}
