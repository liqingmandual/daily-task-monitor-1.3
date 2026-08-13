import { displayMetaForActivity } from "../../lib/activity-composition";
import type { ClassificationSource, Segment } from "../../lib/metrics";

const sourceLabels: Record<ClassificationSource, string> = {
  manual: "人工确认",
  idle: "空闲检测",
  rule: "本地规则",
  behavior: "本地行为推断",
  ai: "AI 推断",
  pending: "待分类",
};

function durationLabel(segment: Segment): string {
  const seconds = Math.max(0, Math.round((segment.endMs - segment.startMs) / 1_000));
  const minutes = Math.floor(seconds / 60);
  return minutes > 0 ? `${minutes} 分 ${seconds % 60} 秒` : `${seconds} 秒`;
}

export function ActivityExplanationLayers({ segment }: { segment: Segment }) {
  const classification = displayMetaForActivity(segment.category, segment.videoPurpose);
  const source = segment.classificationSource ?? (segment.needsReview ? "pending" : "rule");
  const confidence = Math.round(Math.max(0, Math.min(1, segment.confidence)) * 100);
  const suggestion = segment.needsReview || segment.category === "pending"
    ? "建议人工复核；确认前不会把推断当作事实。"
    : source === "manual"
      ? "已由人工确认，没有待处理建议。"
      : "当前无需复核；仍可手动修正这条分类。";

  return <dl className="activity-explanation-layers" aria-label="活动解释分层">
    <div data-layer="fact"><dt>原始事实</dt><dd>{segment.app} · {segment.title || "无窗口标题"}</dd></div>
    <div data-layer="derived"><dt>派生统计</dt><dd>持续 {durationLabel(segment)}</dd></div>
    <div data-layer="inference"><dt>分类推断</dt><dd>{classification.label} · {sourceLabels[source]} · 置信度 {confidence}%{segment.classificationReason ? ` · ${segment.classificationReason}` : ""}</dd></div>
    <div data-layer="suggestion"><dt>建议</dt><dd>{suggestion}</dd></div>
  </dl>;
}
