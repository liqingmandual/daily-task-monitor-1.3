import { displayMetaForActivity, displayMetaForKey, type ActivityDisplayKey } from "./activity-composition";
import {
  isLearningSegment,
  type ActivityCategory,
  type Segment,
  type TimeSeriesKey,
  type VideoPurpose,
} from "./metrics";

export type TimelineFilter =
  | { mode: "all" }
  | { mode: "active" | "idle" | "learning" }
  | { mode: "category"; category: ActivityCategory; videoPurpose?: VideoPurpose }
  | { mode: "displayCategory"; key: ActivityDisplayKey }
  | { mode: "app"; app: string; appPath?: string }
  | { mode: "segment"; segmentId: string }
  | { mode: "timeBucket"; startHour: number; endHour: number; series: TimeSeriesKey };

export function matchesTimelineFilter(segment: Segment, filter: TimelineFilter): boolean {
  if (filter.mode === "all") return true;
  if (filter.mode === "active") return segment.category !== "idle";
  if (filter.mode === "idle") return segment.category === "idle";
  if (filter.mode === "learning") return isLearningSegment(segment);
  if (filter.mode === "category") {
    return segment.category === filter.category
      && (!filter.videoPurpose || segment.videoPurpose === filter.videoPurpose);
  }
  if (filter.mode === "displayCategory") {
    return displayMetaForActivity(segment.category, segment.videoPurpose).key === filter.key;
  }
  if (filter.mode === "app") {
    return segment.app === filter.app
      && (!filter.appPath || (segment.appPath ?? "") === filter.appPath);
  }
  if (filter.mode === "segment") return segment.id === filter.segmentId;
  if (filter.mode !== "timeBucket") return false;
  const bucketStart = filter.startHour * 3_600_000;
  const bucketEnd = filter.endHour * 3_600_000;
  const overlaps = segment.endMs > bucketStart && segment.startMs < bucketEnd;
  if (!overlaps) return false;
  if (filter.series === "active") return segment.category !== "idle";
  if (filter.series === "learning") return isLearningSegment(segment);
  if (filter.series === "learning_video") return segment.category === "video_input" && segment.videoPurpose === "learning";
  if (filter.series === "leisure_video") return segment.category === "video_input" && segment.videoPurpose === "leisure";
  if (filter.series === "unknown_video") return segment.category === "video_input" && segment.videoPurpose === "unknown";
  return segment.category === filter.series;
}

export function timelineFilterLabel(filter: TimelineFilter): string {
  switch (filter.mode) {
    case "all": return "全部记录";
    case "active": return "活跃记录";
    case "idle": return "不活跃记录";
    case "learning": return "学习记录";
    case "category": return `活动分类：${displayMetaForActivity(
      filter.category,
      filter.videoPurpose ?? "unknown",
    ).label}`;
    case "displayCategory": return `活动分类：${displayMetaForKey(filter.key).label}`;
    case "app": return `应用：${filter.app}`;
    case "segment": return "最长专注片段";
    case "timeBucket": {
      const series = filter.series === "active" ? "活跃"
        : filter.series === "learning" ? "学习"
          : displayMetaForKey(filter.series).label;
      return `${String(filter.startHour).padStart(2, "0")}-${String(filter.endHour).padStart(2, "0")} ${series}`;
    }
  }
}
