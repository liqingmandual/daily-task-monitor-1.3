import { isLearningActivity } from "./activity-composition";
import { canonicalizeOverlappingSegments } from "./segment-overlap";

export type ActivityCategory =
  | "idle"
  | "research"
  | "video_input"
  | "text_input"
  | "game"
  | "social"
  | "creation_development"
  | "file_management"
  | "pending";

export type ActivityDisplayKey =
  | Exclude<ActivityCategory, "video_input">
  | "learning_video"
  | "leisure_video"
  | "unknown_video";

export type VideoPurpose = "learning" | "leisure" | "unknown";
export type InactivityReason = "input_idle" | "continuity_gap" | "legacy_gap_repair";

export interface Segment {
  id: string;
  startMs: number;
  endMs: number;
  app: string;
  appPath?: string;
  title: string;
  category: ActivityCategory;
  videoPurpose: VideoPurpose;
  confidence: number;
  needsReview: boolean;
  inactivityReason?: InactivityReason | null;
}

export interface ShareItem {
  key: ActivityCategory;
  seconds: number;
  share: number;
}

export interface AppShareItem {
  name: string;
  appPath: string;
  seconds: number;
  share: number;
}

export interface PeriodMetric {
  key: "night" | "morning" | "afternoon" | "evening";
  range: string;
  activeSeconds: number;
  learningSeconds: number;
  idleSeconds: number;
  categorySeconds: Partial<Record<ActivityCategory, number>>;
}

export interface TimeBucketMetric {
  key: string;
  startHour: number;
  endHour: number;
  range: string;
  activeSeconds: number;
  learningSeconds: number;
  otherActiveSeconds: number;
  idleSeconds: number;
  categorySeconds: Partial<Record<ActivityCategory, number>>;
  videoPurposeSeconds?: Partial<Record<VideoPurpose, number>>;
  appSeconds: Record<string, number>;
}

export interface DashboardMetrics {
  monitoredSeconds: number;
  activeSeconds: number;
  idleSeconds: number;
  learningSeconds: number;
  categories: ShareItem[];
  apps: AppShareItem[];
  periods: PeriodMetric[];
  timeBuckets: TimeBucketMetric[];
}

export const TWO_HOUR_BUCKET_MAX_SECONDS = 2 * 60 * 60;

export type TimeSeriesKey = "active" | "learning" | ActivityDisplayKey;

export function getTimeBucketSeriesSeconds(bucket: TimeBucketMetric, key: TimeSeriesKey): number {
  if (key === "active") return bucket.activeSeconds;
  if (key === "learning") return bucket.learningSeconds;
  if (key === "idle") return bucket.idleSeconds;
  if (key === "learning_video") return bucket.videoPurposeSeconds?.learning ?? 0;
  if (key === "leisure_video") return bucket.videoPurposeSeconds?.leisure ?? 0;
  if (key === "unknown_video") return bucket.videoPurposeSeconds?.unknown ?? 0;
  if (key === "pending") {
    return (bucket.categorySeconds.pending ?? 0) + (bucket.videoPurposeSeconds?.unknown ?? 0);
  }
  return bucket.categorySeconds[key] ?? 0;
}

const periodDefinitions: Array<Pick<PeriodMetric, "key" | "range"> & { startHour: number; endHour: number }> = [
  { key: "night", range: "00-06", startHour: 0, endHour: 6 },
  { key: "morning", range: "06-12", startHour: 6, endHour: 12 },
  { key: "afternoon", range: "12-18", startHour: 12, endHour: 18 },
  { key: "evening", range: "18-24", startHour: 18, endHour: 24 },
];

const timeBucketDefinitions: Array<Pick<TimeBucketMetric, "key" | "range" | "startHour" | "endHour">> = Array.from(
  { length: 12 },
  (_, index) => {
    const startHour = index * 2;
    const endHour = startHour + 2;
    const range = `${String(startHour).padStart(2, "0")}-${String(endHour).padStart(2, "0")}`;
    return { key: range, range, startHour, endHour };
  },
);

export function isLearningSegment(segment: Segment): boolean {
  return isLearningActivity(segment.category, segment.videoPurpose);
}

function getTimeBucketOverlapSeconds(segment: Segment, dayStartMs: number): number[] {
  const overlaps = timeBucketDefinitions.map((definition, index) => {
    const bucketStart = dayStartMs + definition.startHour * 3_600_000;
    const bucketEnd = dayStartMs + definition.endHour * 3_600_000;
    return {
      index,
      overlapMs: Math.max(0, Math.min(segment.endMs, bucketEnd) - Math.max(segment.startMs, bucketStart)),
    };
  });
  const seconds = overlaps.map(({ overlapMs }) => Math.floor(overlapMs / 1_000));
  let remainingSeconds = Math.round(overlaps.reduce((total, overlap) => total + overlap.overlapMs, 0) / 1_000);
  remainingSeconds -= seconds.reduce((total, secondsInBucket) => total + secondsInBucket, 0);

  for (const { index } of [...overlaps].sort((a, b) => b.overlapMs % 1_000 - (a.overlapMs % 1_000) || a.index - b.index)) {
    if (!remainingSeconds) break;
    seconds[index] += 1;
    remainingSeconds -= 1;
  }

  return seconds;
}

export function buildDashboardMetrics(segments: Segment[], dayStartMs = 0): DashboardMetrics {
  const categoryTotals = new Map<ActivityCategory, number>();
  const appTotals = new Map<string, { name: string; appPath: string; seconds: number }>();
  const periods: PeriodMetric[] = periodDefinitions.map(({ key, range }) => ({
    key,
    range,
    activeSeconds: 0,
    learningSeconds: 0,
    idleSeconds: 0,
    categorySeconds: {},
  }));
  const timeBuckets: TimeBucketMetric[] = timeBucketDefinitions.map(({ key, range, startHour, endHour }) => ({
    key,
    range,
    startHour,
    endHour,
    activeSeconds: 0,
    learningSeconds: 0,
    otherActiveSeconds: 0,
    idleSeconds: 0,
    categorySeconds: {},
    videoPurposeSeconds: {},
    appSeconds: {},
  }));
  let monitoredSeconds = 0;
  let idleSeconds = 0;
  let learningSeconds = 0;

  for (const segment of canonicalizeOverlappingSegments(segments)) {
    const durationSeconds = Math.max(0, Math.round((segment.endMs - segment.startMs) / 1_000));
    if (!durationSeconds) continue;
    monitoredSeconds += durationSeconds;
    categoryTotals.set(segment.category, (categoryTotals.get(segment.category) ?? 0) + durationSeconds);

    if (segment.category === "idle") {
      idleSeconds += durationSeconds;
    } else {
      const appPath = segment.appPath ?? "";
      const key = `${segment.app}\n${appPath}`;
      const current = appTotals.get(key) ?? { name: segment.app, appPath, seconds: 0 };
      current.seconds += durationSeconds;
      appTotals.set(key, current);
    }
    if (isLearningSegment(segment)) learningSeconds += durationSeconds;

    periodDefinitions.forEach((definition, index) => {
      const periodStart = dayStartMs + definition.startHour * 3_600_000;
      const periodEnd = dayStartMs + definition.endHour * 3_600_000;
      const overlapMs = Math.max(0, Math.min(segment.endMs, periodEnd) - Math.max(segment.startMs, periodStart));
      const overlapSeconds = Math.round(overlapMs / 1_000);
      if (!overlapSeconds) return;
      periods[index].categorySeconds[segment.category] =
        (periods[index].categorySeconds[segment.category] ?? 0) + overlapSeconds;
      if (segment.category === "idle") {
        periods[index].idleSeconds += overlapSeconds;
      } else {
        periods[index].activeSeconds += overlapSeconds;
      }
      if (isLearningSegment(segment)) periods[index].learningSeconds += overlapSeconds;
    });

    const timeBucketOverlapSeconds = getTimeBucketOverlapSeconds(segment, dayStartMs);
    timeBucketDefinitions.forEach((_, index) => {
      const overlapSeconds = timeBucketOverlapSeconds[index];
      if (!overlapSeconds) return;

      const bucket = timeBuckets[index];
      bucket.categorySeconds[segment.category] = (bucket.categorySeconds[segment.category] ?? 0) + overlapSeconds;
      if (segment.category === "video_input") {
        const purpose = segment.videoPurpose ?? "unknown";
        const purposeSeconds = bucket.videoPurposeSeconds ?? (bucket.videoPurposeSeconds = {});
        purposeSeconds[purpose] = (purposeSeconds[purpose] ?? 0) + overlapSeconds;
      }
      if (segment.category === "idle") {
        bucket.idleSeconds += overlapSeconds;
      } else {
        bucket.activeSeconds += overlapSeconds;
        bucket.appSeconds[segment.app] = (bucket.appSeconds[segment.app] ?? 0) + overlapSeconds;
      }
      if (isLearningSegment(segment)) bucket.learningSeconds += overlapSeconds;
    });
  }

  for (const bucket of timeBuckets) {
    bucket.otherActiveSeconds = bucket.activeSeconds - bucket.learningSeconds;
  }

  const activeSeconds = Math.max(0, monitoredSeconds - idleSeconds);
  const categories = [...categoryTotals.entries()]
    .map(([key, seconds]) => ({ key, seconds, share: monitoredSeconds ? seconds / monitoredSeconds : 0 }))
    .sort((a, b) => b.seconds - a.seconds);
  const apps = [...appTotals.values()]
    .map(({ name, appPath, seconds }) => ({ name, appPath, seconds, share: activeSeconds ? seconds / activeSeconds : 0 }))
    .sort((a, b) => b.seconds - a.seconds);

  return { monitoredSeconds, activeSeconds, idleSeconds, learningSeconds, categories, apps, periods, timeBuckets };
}
