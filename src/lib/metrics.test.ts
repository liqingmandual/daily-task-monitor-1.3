import { describe, expect, it } from "vitest";
import {
  buildDashboardMetrics,
  getTimeBucketSeriesSeconds,
  TWO_HOUR_BUCKET_MAX_SECONDS,
  type Segment,
} from "./metrics";

const hour = 3_600_000;

function segment(
  startHour: number,
  endHour: number,
  category: Segment["category"],
  app: string,
  videoPurpose: Segment["videoPurpose"] = "unknown",
): Segment {
  return {
    id: `${app}-${startHour}`,
    startMs: startHour * hour,
    endMs: endHour * hour,
    app,
    title: app,
    category,
    videoPurpose,
    confidence: 0.9,
    needsReview: false,
  };
}

describe("buildDashboardMetrics", () => {
  it("calculates monitored active idle and learning totals", () => {
    const metrics = buildDashboardMetrics([
      segment(0, 1, "idle", "Idle"),
      segment(1, 2, "research", "Chrome"),
      segment(2, 3, "video_input", "Chrome", "learning"),
      segment(3, 4, "video_input", "Chrome", "leisure"),
      segment(4, 5, "social", "WeChat"),
    ]);

    expect(metrics.monitoredSeconds).toBe(5 * 3_600);
    expect(metrics.activeSeconds).toBe(4 * 3_600);
    expect(metrics.idleSeconds).toBe(3_600);
    expect(metrics.learningSeconds).toBe(2 * 3_600);
  });

  it("does not double count overlapping collector segments", () => {
    const metrics = buildDashboardMetrics([
      segment(0, 4, "idle", "Idle A"),
      segment(0, 4, "idle", "Idle B"),
      segment(1, 2, "research", "Chrome"),
    ]);

    expect(metrics.monitoredSeconds).toBe(4 * 3_600);
    expect(metrics.idleSeconds).toBe(3 * 3_600);
    expect(metrics.activeSeconds).toBe(3_600);
    expect(metrics.learningSeconds).toBe(3_600);
  });

  it("splits segments across the four six-hour periods", () => {
    const metrics = buildDashboardMetrics([
      segment(5, 7, "creation_development", "Codex"),
      segment(12, 13, "game", "Game"),
    ]);

    expect(metrics.periods[0]).toMatchObject({ activeSeconds: 3_600, learningSeconds: 3_600 });
    expect(metrics.periods[1]).toMatchObject({ activeSeconds: 3_600, learningSeconds: 3_600 });
    expect(metrics.periods[2]).toMatchObject({ activeSeconds: 3_600, learningSeconds: 0 });
  });

  it("returns category and app shares against the correct totals", () => {
    const metrics = buildDashboardMetrics([
      segment(0, 1, "idle", "Idle"),
      segment(1, 3, "research", "Chrome"),
      segment(3, 4, "creation_development", "Codex"),
    ]);

    expect(metrics.categories.find((item) => item.key === "research")?.share).toBe(0.5);
    expect(metrics.apps.find((item) => item.name === "Chrome")?.share).toBeCloseTo(2 / 3);
  });

  it("keeps same-name executables distinct in app ranking metrics", () => {
    const installed = { ...segment(1, 2, "creation_development", "Editor"), appPath: "C:\\Apps\\Editor.exe" };
    const portable = { ...segment(2, 3, "creation_development", "Editor"), appPath: "D:\\Portable\\Editor.exe" };

    const metrics = buildDashboardMetrics([installed, portable]);

    expect(metrics.apps).toEqual([
      expect.objectContaining({ name: "Editor", appPath: installed.appPath }),
      expect.objectContaining({ name: "Editor", appPath: portable.appPath }),
    ]);
  });

  it("splits a segment crossing a two-hour bucket boundary", () => {
    const metrics = buildDashboardMetrics([segment(1, 3, "research", "Chrome")]);

    expect(metrics.timeBuckets[0]).toMatchObject({
      key: "00-02",
      startHour: 0,
      endHour: 2,
      range: "00-02",
      activeSeconds: 3_600,
      learningSeconds: 3_600,
      otherActiveSeconds: 0,
      idleSeconds: 0,
      categorySeconds: { research: 3_600 },
      appSeconds: { Chrome: 3_600 },
    });
    expect(metrics.timeBuckets[1]).toMatchObject({
      key: "02-04",
      activeSeconds: 3_600,
      learningSeconds: 3_600,
      appSeconds: { Chrome: 3_600 },
    });
  });

  it("makes all twelve time buckets sum to the daily totals", () => {
    const metrics = buildDashboardMetrics([
      segment(0, 2, "idle", "Idle"),
      segment(2, 8, "research", "Chrome"),
      segment(8, 14, "social", "WeChat"),
      segment(14, 20, "video_input", "Player", "learning"),
      segment(20, 24, "game", "Game"),
    ]);

    expect(metrics.timeBuckets).toHaveLength(12);
    expect(metrics.timeBuckets.reduce((total, bucket) => total + bucket.activeSeconds, 0)).toBe(metrics.activeSeconds);
    expect(metrics.timeBuckets.reduce((total, bucket) => total + bucket.idleSeconds, 0)).toBe(metrics.idleSeconds);
    expect(metrics.timeBuckets.reduce((total, bucket) => total + bucket.learningSeconds, 0)).toBe(metrics.learningSeconds);
  });

  it("preserves daily totals when a sub-second segment crosses a bucket boundary", () => {
    const metrics = buildDashboardMetrics([
      {
        ...segment(0, 1, "research", "Chrome"),
        startMs: 2 * hour - 500,
        endMs: 2 * hour + 500,
      },
    ]);

    expect(metrics.activeSeconds).toBe(1);
    expect(metrics.timeBuckets.reduce((total, bucket) => total + bucket.activeSeconds, 0)).toBe(1);
  });

  it("accounts for active time as learning plus other active time in every bucket", () => {
    const metrics = buildDashboardMetrics([
      segment(0, 2, "research", "Chrome"),
      segment(2, 4, "social", "WeChat"),
      segment(4, 6, "video_input", "Player", "learning"),
      segment(6, 8, "game", "Game"),
    ]);

    for (const bucket of metrics.timeBuckets) {
      expect(bucket.learningSeconds + bucket.otherActiveSeconds).toBe(bucket.activeSeconds);
    }
  });

  it("maps active, learning, idle and atomic categories without double counting", () => {
    const metrics = buildDashboardMetrics([
      segment(8, 9, "research", "Chrome"),
      segment(9, 10, "idle", "Idle"),
    ]);
    const bucket = metrics.timeBuckets[4];
    expect(getTimeBucketSeriesSeconds(bucket, "active")).toBe(bucket.activeSeconds);
    expect(getTimeBucketSeriesSeconds(bucket, "learning")).toBe(bucket.learningSeconds);
    expect(getTimeBucketSeriesSeconds(bucket, "idle")).toBe(bucket.idleSeconds);
    expect(getTimeBucketSeriesSeconds(bucket, "creation_development"))
      .toBe(bucket.categorySeconds.creation_development ?? 0);
  });

  it("keeps the three video purposes separate in hourly display series", () => {
    const metrics = buildDashboardMetrics([
      segment(8, 9, "video_input", "Course", "learning"),
      segment(9, 10, "video_input", "Player", "leisure"),
      segment(10, 11, "video_input", "Browser", "unknown"),
    ]);

    expect(getTimeBucketSeriesSeconds(metrics.timeBuckets[4], "learning_video")).toBe(3_600);
    expect(getTimeBucketSeriesSeconds(metrics.timeBuckets[4], "leisure_video")).toBe(3_600);
    expect(getTimeBucketSeriesSeconds(metrics.timeBuckets[5], "unknown_video")).toBe(3_600);
  });

  it("includes unknown-purpose video in the canonical unclassified hourly series", () => {
    const metrics = buildDashboardMetrics([
      segment(8, 8.5, "pending", "Unknown"),
      segment(8.5, 9, "video_input", "Browser", "unknown"),
    ]);

    expect(getTimeBucketSeriesSeconds(metrics.timeBuckets[4], "pending")).toBe(3_600);
  });

  it("uses a two-hour chart maximum", () => {
    expect(TWO_HOUR_BUCKET_MAX_SECONDS).toBe(7_200);
  });
});
