import type { TimeBucketMetric, TimeSeriesKey } from "../../lib/metrics";
import { TWO_HOUR_BUCKET_MAX_SECONDS, getTimeBucketSeriesSeconds } from "../../lib/metrics";
import type { TimelineFilter } from "../../lib/timeline-filter";
import { formatChartDuration } from "../../lib/presentation";
import { displayMetaForKey } from "../../lib/activity-composition";

export const DEFAULT_TIME_SERIES: TimeSeriesKey[] = ["active", "learning"];

export function timeSeriesLabel(series: TimeSeriesKey) {
  if (series === "active") return "活跃";
  if (series === "learning") return "学习";
  return displayMetaForKey(series).label;
}

export function timeSeriesColor(series: TimeSeriesKey): string | undefined {
  if (series === "active" || series === "learning") return undefined;
  return displayMetaForKey(series).color;
}

export function toggleTimeSeries(value: TimeSeriesKey[], series: TimeSeriesKey): TimeSeriesKey[] {
  const next = value.includes(series) ? value.filter((item) => item !== series) : [...value, series];
  return next.length ? next : DEFAULT_TIME_SERIES;
}

export function timeBucketFilter(bucket: TimeBucketMetric, series: TimeSeriesKey): TimelineFilter {
  return { mode: "timeBucket", startHour: bucket.startHour, endHour: bucket.endHour, series };
}

export function getTimeBarTooltip(bucket: TimeBucketMetric, series: TimeSeriesKey) {
  const seconds = getTimeBucketSeriesSeconds(bucket, series);
  const isIdle = series === "idle";
  const denominatorSeconds = isIdle ? bucket.activeSeconds + bucket.idleSeconds : bucket.activeSeconds;
  return {
    range: bucket.range,
    seriesLabel: timeSeriesLabel(series),
    seconds,
    duration: formatChartDuration(seconds),
    percentOfWindow: seconds / TWO_HOUR_BUCKET_MAX_SECONDS * 100,
    denominatorLabel: isIdle ? "桶内监测" : "桶内活跃",
    percentOfDenominator: denominatorSeconds ? seconds / denominatorSeconds * 100 : 0,
  };
}

export function getTimeChartMinimumWidth(bucketCount: number, seriesCount: number) {
  const bucketWidth = Math.max(48, 12 + Math.max(seriesCount, 1) * 4);
  return 32 + bucketCount * bucketWidth + Math.max(bucketCount, 1) * 3;
}
