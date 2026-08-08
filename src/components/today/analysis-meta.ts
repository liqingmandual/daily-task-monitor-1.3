import {
  displayMetaForActivity,
  type ActivityDisplayMeta,
} from "../../lib/activity-composition";
import type { ActivityCategory } from "../../lib/metrics";

const categories: ActivityCategory[] = [
  "idle",
  "research",
  "video_input",
  "text_input",
  "game",
  "social",
  "creation_development",
  "file_management",
  "pending",
];

export const categoryMeta = Object.fromEntries(categories.map((category) => {
  const meta = displayMetaForActivity(category, "unknown");
  return [category, { label: meta.label, color: meta.color }];
})) as Record<ActivityCategory, Pick<ActivityDisplayMeta, "label" | "color">>;
