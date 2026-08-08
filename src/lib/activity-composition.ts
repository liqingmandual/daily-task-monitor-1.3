import type { ActivityCategory, ActivityDisplayKey, Segment, VideoPurpose } from "./metrics";

export type { ActivityDisplayKey } from "./metrics";

export type ActivityScope = "all" | "meaningful";

export type MeaningfulReason = "core" | "workflow_link" | "excluded";

export interface ActivityCompositionItem {
  key: ActivityDisplayKey;
  category: ActivityCategory;
  videoPurpose: VideoPurpose | null;
  seconds: number;
  share: number;
  meaningfulReason: MeaningfulReason;
}

export interface ActivityComposition {
  totalSeconds: number;
  items: ActivityCompositionItem[];
}

export interface ActivityCompositions {
  all: ActivityComposition;
  meaningful: ActivityComposition;
}

export interface ActivityDisplayMeta {
  key: ActivityDisplayKey;
  category: ActivityCategory;
  videoPurpose: VideoPurpose | null;
  label: string;
  color: string;
  order: number;
  meaningfulReason: MeaningfulReason;
}

export interface ActivityCompositionDonutItem {
  key: ActivityDisplayKey;
  name: string;
  value: number;
  share: number;
  color: string;
  order: number;
}

export const ACTIVITY_SCOPE_STORAGE_KEYS = {
  today: "daily-task-monitor-today-activity-scope-v1",
  trends: "daily-task-monitor-trends-activity-scope-v1",
} as const;

export const activityCategoryValues = [
  "idle",
  "research",
  "video_input",
  "text_input",
  "game",
  "social",
  "creation_development",
  "file_management",
  "pending",
] as const satisfies readonly ActivityCategory[];

export const videoPurposeValues = ["learning", "leisure", "unknown"] as const satisfies readonly VideoPurpose[];

const activityCategorySet = new Set<string>(activityCategoryValues);
const videoPurposeSet = new Set<string>(videoPurposeValues);
const learningDisplayKeys = new Set<ActivityDisplayKey>([
  "research",
  "learning_video",
  "text_input",
  "creation_development",
]);

export const activityDisplayRegistry: readonly ActivityDisplayMeta[] = [
  { key: "idle", category: "idle", videoPurpose: null, label: "不活跃", color: "#94a3b8", order: 0, meaningfulReason: "excluded" },
  { key: "research", category: "research", videoPurpose: null, label: "搜索/调研", color: "#d97706", order: 1, meaningfulReason: "core" },
  { key: "learning_video", category: "video_input", videoPurpose: "learning", label: "学习视频", color: "#8b5cf6", order: 2, meaningfulReason: "core" },
  { key: "leisure_video", category: "video_input", videoPurpose: "leisure", label: "休闲视频", color: "#ec4899", order: 3, meaningfulReason: "excluded" },
  { key: "text_input", category: "text_input", videoPurpose: null, label: "文字信息输入", color: "#0ea5e9", order: 4, meaningfulReason: "core" },
  { key: "game", category: "game", videoPurpose: null, label: "游戏", color: "#dc2626", order: 5, meaningfulReason: "excluded" },
  { key: "social", category: "social", videoPurpose: null, label: "社交通讯", color: "#7c3aed", order: 6, meaningfulReason: "excluded" },
  { key: "creation_development", category: "creation_development", videoPurpose: null, label: "创作开发", color: "#1d4ed8", order: 7, meaningfulReason: "core" },
  { key: "file_management", category: "file_management", videoPurpose: null, label: "文件整理", color: "#0891b2", order: 8, meaningfulReason: "excluded" },
  { key: "pending", category: "pending", videoPurpose: null, label: "未分类", color: "#64748b", order: 9, meaningfulReason: "excluded" },
];

const displayMetaByKey = new Map(activityDisplayRegistry.map((meta) => [meta.key, meta]));

function canonicalDisplayKey(key: ActivityDisplayKey): ActivityDisplayKey {
  return key === "unknown_video" ? "pending" : key;
}

export function displayMetaForKey(key: ActivityDisplayKey): ActivityDisplayMeta {
  const meta = displayMetaByKey.get(canonicalDisplayKey(key));
  if (!meta) throw new Error(`Missing display metadata for ${key}`);
  return meta;
}

export function activityDisplayKeyFor(category: ActivityCategory, videoPurpose: VideoPurpose): ActivityDisplayKey {
  if (category !== "video_input") return category;
  if (videoPurpose === "learning") return "learning_video";
  if (videoPurpose === "leisure") return "leisure_video";
  return "pending";
}

export function displayMetaForActivity(category: ActivityCategory, videoPurpose: VideoPurpose): ActivityDisplayMeta {
  return displayMetaForKey(activityDisplayKeyFor(category, videoPurpose));
}

export function isActivityCategory(value: unknown): value is ActivityCategory {
  return typeof value === "string" && activityCategorySet.has(value);
}

export function isVideoPurpose(value: unknown): value is VideoPurpose {
  return typeof value === "string" && videoPurposeSet.has(value);
}

export function isLearningActivity(category: ActivityCategory, videoPurpose: VideoPurpose): boolean {
  return learningDisplayKeys.has(activityDisplayKeyFor(category, videoPurpose));
}

export function getCompositionLearningSeconds(composition: ActivityComposition): number {
  return composition.items.reduce(
    (total, item) => total + (learningDisplayKeys.has(item.key) ? item.seconds : 0),
    0,
  );
}

export function isActivityScope(value: unknown): value is ActivityScope {
  return value === "all" || value === "meaningful";
}

export function parsePersistedActivityScope(value: unknown): ActivityScope {
  if (value === "non_entertainment") return "meaningful";
  return isActivityScope(value) ? value : "all";
}

export function serializeActivityScope(scope: ActivityScope): string {
  return scope;
}

export function compositionToDonutItems(composition: ActivityComposition): ActivityCompositionDonutItem[] {
  const merged = new Map<ActivityDisplayKey, ActivityCompositionDonutItem>();
  for (const item of composition.items) {
    const meta = displayMetaByKey.get(canonicalDisplayKey(item.key));
    if (!meta) throw new Error(`Missing display metadata for ${item.key}`);
    const current = merged.get(meta.key);
    if (current) {
      current.value += item.seconds;
      current.share += item.share;
      continue;
    }
    merged.set(meta.key, {
      key: meta.key,
      name: meta.label,
      value: item.seconds,
      share: item.share,
      color: meta.color,
      order: meta.order,
    });
  }
  return [...merged.values()].sort((left, right) => right.value - left.value || left.order - right.order);
}

export function buildFallbackActivityCompositions(
  segments: readonly Segment[],
  _linkedActivityIds: ReadonlySet<string> = new Set(),
): ActivityCompositions {
  const all = new Map<ActivityDisplayKey, ActivityCompositionItem>();
  const meaningful = new Map<ActivityDisplayKey, ActivityCompositionItem>();
  for (const segment of segments) {
    const seconds = Math.max(0, Math.round((segment.endMs - segment.startMs) / 1_000));
    if (!seconds) continue;
    const meta = displayMetaForActivity(segment.category, segment.videoPurpose);
    addCompositionSeconds(all, segment, meta, seconds);
    const included = meta.meaningfulReason === "core";
    if (included) addCompositionSeconds(meaningful, segment, meta, seconds);
  }
  return {
    all: finishComposition(all),
    meaningful: finishComposition(meaningful),
  };
}

function addCompositionSeconds(
  target: Map<ActivityDisplayKey, ActivityCompositionItem>,
  segment: Segment,
  meta: ActivityDisplayMeta,
  seconds: number,
): void {
  const current = target.get(meta.key);
  if (current) {
    current.seconds += seconds;
    return;
  }
  target.set(meta.key, {
    key: meta.key,
    category: meta.key === "pending" ? "pending" : segment.category,
    videoPurpose: meta.key === "pending" ? null : segment.category === "video_input" ? segment.videoPurpose : null,
    seconds,
    share: 0,
    meaningfulReason: meta.meaningfulReason,
  });
}

function finishComposition(source: Map<ActivityDisplayKey, ActivityCompositionItem>): ActivityComposition {
  const totalSeconds = [...source.values()].reduce((sum, item) => sum + item.seconds, 0);
  const items = [...source.values()]
    .map((item) => ({ ...item, share: totalSeconds ? item.seconds / totalSeconds : 0 }))
    .sort((left, right) => right.seconds - left.seconds
      || (displayMetaByKey.get(left.key)?.order ?? 0) - (displayMetaByKey.get(right.key)?.order ?? 0));
  return { totalSeconds, items };
}
