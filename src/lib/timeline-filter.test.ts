import { describe, expect, it } from "vitest";
import type { Segment } from "./metrics";
import { matchesTimelineFilter, timelineFilterLabel, type TimelineFilter } from "./timeline-filter";

const hour = 3_600_000;

function makeSegment(
  id: string,
  startHour: number,
  endHour: number,
  category: Segment["category"],
  app: string,
  videoPurpose: Segment["videoPurpose"] = "unknown",
): Segment {
  return {
    id,
    startMs: startHour * hour,
    endMs: endHour * hour,
    app,
    title: id,
    category,
    videoPurpose,
    confidence: 0.9,
    needsReview: false,
  };
}

const researchSegment = makeSegment("research", 8, 9, "research", "Chrome");
const gameSegment = makeSegment("game", 8, 9, "game", "Game");
const lateResearchSegment = makeSegment("late-research", 10, 11, "research", "Chrome");

describe("matchesTimelineFilter", () => {
  it("treats pending and unknown-purpose video as the same unclassified display filter", () => {
    const pending = makeSegment("pending", 1, 2, "pending", "Unknown");
    const unknownVideo = makeSegment("unknown-video", 1, 2, "video_input", "Browser", "unknown");
    const leisureVideo = makeSegment("leisure-video", 1, 2, "video_input", "Browser", "leisure");

    expect(matchesTimelineFilter(pending, { mode: "displayCategory", key: "pending" })).toBe(true);
    expect(matchesTimelineFilter(unknownVideo, { mode: "displayCategory", key: "pending" })).toBe(true);
    expect(matchesTimelineFilter(leisureVideo, { mode: "displayCategory", key: "pending" })).toBe(false);
    expect(timelineFilterLabel({ mode: "displayCategory", key: "pending" })).toBe("活动分类：未分类");
  });
  it("filters a two-hour learning bucket by time and learning semantics", () => {
    const filter: TimelineFilter = { mode: "timeBucket", startHour: 8, endHour: 10, series: "learning" };
    expect(matchesTimelineFilter(researchSegment, filter)).toBe(true);
    expect(matchesTimelineFilter(gameSegment, filter)).toBe(false);
    expect(matchesTimelineFilter(lateResearchSegment, filter)).toBe(false);
  });

  it("matches unified category, app, segment, activity and all filters", () => {
    const idleSegment = makeSegment("idle", 12, 13, "idle", "Idle");
    expect(matchesTimelineFilter(researchSegment, { mode: "all" })).toBe(true);
    expect(matchesTimelineFilter(researchSegment, { mode: "active" })).toBe(true);
    expect(matchesTimelineFilter(idleSegment, { mode: "active" })).toBe(false);
    expect(matchesTimelineFilter(idleSegment, { mode: "idle" })).toBe(true);
    expect(matchesTimelineFilter(researchSegment, { mode: "learning" })).toBe(true);
    expect(matchesTimelineFilter(gameSegment, { mode: "category", category: "game" })).toBe(true);
    expect(matchesTimelineFilter(researchSegment, { mode: "app", app: "Chrome" })).toBe(true);
    expect(matchesTimelineFilter(researchSegment, { mode: "segment", segmentId: "research" })).toBe(true);
  });

  it("matches time bucket overlap for active, idle and atomic category series", () => {
    const idleSegment = makeSegment("idle", 9, 11, "idle", "Idle");
    expect(matchesTimelineFilter(researchSegment, { mode: "timeBucket", startHour: 9, endHour: 10, series: "active" })).toBe(false);
    expect(matchesTimelineFilter(idleSegment, { mode: "timeBucket", startHour: 10, endHour: 12, series: "idle" })).toBe(true);
    expect(matchesTimelineFilter(researchSegment, { mode: "timeBucket", startHour: 8, endHour: 10, series: "research" })).toBe(true);
    expect(matchesTimelineFilter(researchSegment, { mode: "timeBucket", startHour: 10, endHour: 12, series: "research" })).toBe(false);
  });

  it("matches path-specific app filters while preserving legacy name-only filters", () => {
    const installed = { ...researchSegment, app: "Editor", appPath: "C:\\Apps\\Editor.exe" };
    const portable = { ...researchSegment, app: "Editor", appPath: "D:\\Portable\\Editor.exe" };

    expect(matchesTimelineFilter(installed, { mode: "app", app: "Editor", appPath: installed.appPath })).toBe(true);
    expect(matchesTimelineFilter(portable, { mode: "app", app: "Editor", appPath: installed.appPath })).toBe(false);
    expect(matchesTimelineFilter(installed, { mode: "app", app: "Editor" })).toBe(true);
    expect(matchesTimelineFilter(portable, { mode: "app", app: "Editor" })).toBe(true);
  });

  it("distinguishes the three display slices of video activity", () => {
    const learningVideo = makeSegment("learning-video", 8, 9, "video_input", "Browser", "learning");
    const leisureVideo = makeSegment("leisure-video", 8, 9, "video_input", "Browser", "leisure");

    expect(matchesTimelineFilter(learningVideo, {
      mode: "category",
      category: "video_input",
      videoPurpose: "learning",
    })).toBe(true);
    expect(matchesTimelineFilter(leisureVideo, {
      mode: "category",
      category: "video_input",
      videoPurpose: "learning",
    })).toBe(false);
    expect(timelineFilterLabel({
      mode: "category",
      category: "video_input",
      videoPurpose: "learning",
    })).toBe("活动分类：学习视频");
    expect(matchesTimelineFilter(learningVideo, {
      mode: "timeBucket",
      startHour: 8,
      endHour: 10,
      series: "learning_video",
    })).toBe(true);
    expect(matchesTimelineFilter(leisureVideo, {
      mode: "timeBucket",
      startHour: 8,
      endHour: 10,
      series: "learning_video",
    })).toBe(false);
    expect(timelineFilterLabel({
      mode: "timeBucket",
      startHour: 8,
      endHour: 10,
      series: "learning_video",
    })).toBe("08-10 学习视频");
  });

  it("labels every shared filter mode without a legacy drilldown contract", () => {
    expect(timelineFilterLabel({ mode: "all" })).toBe("全部记录");
    expect(timelineFilterLabel({ mode: "active" })).toBe("活跃记录");
    expect(timelineFilterLabel({ mode: "category", category: "research" })).toBe("活动分类：搜索/调研");
    expect(timelineFilterLabel({ mode: "app", app: "Chrome" })).toBe("应用：Chrome");
    expect(timelineFilterLabel({ mode: "segment", segmentId: "research" })).toBe("最长专注片段");
    expect(timelineFilterLabel({ mode: "timeBucket", startHour: 8, endHour: 10, series: "learning" })).toBe("08-10 学习");
  });
});
