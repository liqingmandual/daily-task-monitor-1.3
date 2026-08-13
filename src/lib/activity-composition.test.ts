import { describe, expect, it } from "vitest";
import * as activityTaxonomy from "./activity-composition";
import {
  ACTIVITY_SCOPE_STORAGE_KEYS,
  activityDisplayRegistry,
  buildFallbackActivityCompositions,
  compositionToDonutItems,
  displayMetaForActivity,
  getCompositionLearningSeconds,
  parsePersistedActivityScope,
  type ActivityComposition,
} from "./activity-composition";
import type { Segment } from "./metrics";

describe("activity composition taxonomy", () => {
  it("validates classification values from the shared runtime taxonomy", () => {
    const taxonomy = activityTaxonomy as typeof activityTaxonomy & {
      isActivityCategory: (value: unknown) => boolean;
      isVideoPurpose: (value: unknown) => boolean;
    };

    expect(taxonomy.isActivityCategory("research")).toBe(true);
    expect(taxonomy.isActivityCategory("work")).toBe(false);
    expect(taxonomy.isVideoPurpose("learning")).toBe(true);
    expect(taxonomy.isVideoPurpose("watching")).toBe(false);
  });

  it("centralizes the display categories that count toward learning time", () => {
    const taxonomy = activityTaxonomy as typeof activityTaxonomy & {
      isLearningActivity: (category: string, videoPurpose: string) => boolean;
    };

    expect(taxonomy.isLearningActivity("research", "unknown")).toBe(true);
    expect(taxonomy.isLearningActivity("video_input", "learning")).toBe(true);
    expect(taxonomy.isLearningActivity("video_input", "leisure")).toBe(false);
    expect(taxonomy.isLearningActivity("social", "unknown")).toBe(false);
  });

  it("sums only taxonomy-defined learning display items in a composition", () => {
    const total = getCompositionLearningSeconds({
      totalSeconds: 2_100,
      items: [
        { key: "research", category: "research", videoPurpose: null, seconds: 600, share: 2 / 7, meaningfulReason: "core" },
        { key: "learning_video", category: "video_input", videoPurpose: "learning", seconds: 900, share: 3 / 7, meaningfulReason: "core" },
        { key: "social", category: "social", videoPurpose: null, seconds: 600, share: 2 / 7, meaningfulReason: "workflow_link" },
      ],
    });

    expect(total).toBe(1_500);
  });

  it.each([
    ["learning", "learning_video", "学习视频"],
    ["leisure", "leisure_video", "休闲视频"],
    ["unknown", "pending", "未分类"],
  ] as const)("maps video purpose %s to the %s display item", (videoPurpose, key, label) => {
    expect(displayMetaForActivity("video_input", videoPurpose)).toMatchObject({ key, label });
  });

  it("provides ten canonical display entries with unique Chinese labels", () => {
    const labels = activityDisplayRegistry.map((item) => item.label);

    expect(activityDisplayRegistry).toHaveLength(10);
    expect(new Set(labels).size).toBe(10);
    expect(labels).toEqual([
      "不活跃",
      "搜索/调研",
      "学习视频",
      "休闲视频",
      "文字信息输入",
      "游戏",
      "社交通讯",
      "创作开发",
      "文件整理",
      "未分类",
    ]);
  });
});

describe("activity composition UI persistence", () => {
  it("uses separate storage keys for today and trends", () => {
    expect(ACTIVITY_SCOPE_STORAGE_KEYS.today).not.toBe(ACTIVITY_SCOPE_STORAGE_KEYS.trends);
    expect(ACTIVITY_SCOPE_STORAGE_KEYS).toEqual({
      today: "daily-task-monitor-today-activity-scope-v1",
      trends: "daily-task-monitor-trends-activity-scope-v1",
    });
  });

  it.each([null, "", "MEANINGFUL", "{\"scope\":\"all\"}"])("falls back to all for invalid persisted scope %j", (value) => {
    expect(parsePersistedActivityScope(value)).toBe("all");
  });

  it("accepts each persisted activity scope exactly", () => {
    expect(parsePersistedActivityScope("all")).toBe("all");
    expect(parsePersistedActivityScope("active")).toBe("active");
    expect(parsePersistedActivityScope("meaningful")).toBe("meaningful");
    expect(parsePersistedActivityScope("non_entertainment")).toBe("meaningful");
  });
});

describe("compositionToDonutItems", () => {
  it("preserves backend seconds and share without recomputing a meaningful subset", () => {
    const composition: ActivityComposition = {
      totalSeconds: 142,
      items: [
        { key: "learning_video", category: "video_input", videoPurpose: "learning", seconds: 125, share: 0.42, meaningfulReason: "core" },
        { key: "research", category: "research", videoPurpose: null, seconds: 17, share: 0.05, meaningfulReason: "core" },
      ],
    };

    expect(compositionToDonutItems(composition)).toEqual([
      { key: "learning_video", name: "学习视频", value: 125, share: 0.42, color: "#8b5cf6", order: 2 },
      { key: "research", name: "搜索/调研", value: 17, share: 0.05, color: "#d97706", order: 1 },
    ]);
  });

  it("merges a legacy unknown-video slice into unclassified without double counting", () => {
    const composition: ActivityComposition = {
      totalSeconds: 300,
      items: [
        { key: "pending", category: "pending", videoPurpose: null, seconds: 120, share: 0.4, meaningfulReason: "excluded" },
        { key: "unknown_video", category: "video_input", videoPurpose: "unknown", seconds: 180, share: 0.6, meaningfulReason: "excluded" },
      ],
    };

    expect(compositionToDonutItems(composition)).toEqual([
      { key: "pending", name: "未分类", value: 300, share: 1, color: "#64748b", order: 9 },
    ]);
  });
});

describe("legacy/preview composition fallback", () => {
  it("keeps contextual activity out until a formal activity id is supplied", () => {
    const segments: Segment[] = [
      { id: "research", startMs: 0, endMs: 600_000, app: "Browser", title: "Research", category: "research", videoPurpose: "unknown", confidence: 1, needsReview: false },
      { id: "linked-file", startMs: 600_000, endMs: 1_200_000, app: "Explorer", title: "Files", category: "file_management", videoPurpose: "unknown", confidence: 1, needsReview: false },
      { id: "unlinked-file", startMs: 1_200_000, endMs: 1_800_000, app: "Explorer", title: "Files", category: "file_management", videoPurpose: "unknown", confidence: 1, needsReview: false },
      { id: "unknown-video", startMs: 1_800_000, endMs: 2_400_000, app: "Browser", title: "Video", category: "video_input", videoPurpose: "unknown", confidence: 1, needsReview: false },
    ];

    const result = buildFallbackActivityCompositions(segments, new Set(["linked-file"]));

    expect(result.all.totalSeconds).toBe(2_400);
    expect(result.meaningful.totalSeconds).toBe(600);
    expect(result.meaningful.items.map((item) => item.key)).toEqual(["research"]);
    expect(result.all.items.some((item) => item.key === "unknown_video")).toBe(false);
    expect(result.all.items.find((item) => item.key === "pending")?.seconds).toBe(600);
  });

  it("counts overlapping collector segments once and prefers activity over idle", () => {
    const base: Segment = { id: "idle-a", startMs: 0, endMs: 14_400_000, app: "Idle", title: "Away", category: "idle", videoPurpose: "unknown", confidence: 1, needsReview: false };
    const result = buildFallbackActivityCompositions([
      base,
      { ...base, id: "idle-b" },
      { ...base, id: "research", startMs: 3_600_000, endMs: 7_200_000, app: "Chrome", title: "Research", category: "research" },
    ]);

    expect(result.all.totalSeconds).toBe(4 * 3_600);
    expect(result.active.totalSeconds).toBe(3_600);
    expect(result.active.items.map((item) => item.key)).toEqual(["research"]);
    expect(result.active.items[0].share).toBe(1);
    expect(result.all.items.find((item) => item.key === "idle")?.seconds).toBe(3 * 3_600);
    expect(result.all.items.find((item) => item.key === "research")?.seconds).toBe(3_600);
    expect(result.meaningful.totalSeconds).toBe(3_600);
  });
});
