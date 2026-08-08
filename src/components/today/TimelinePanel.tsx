import { forwardRef, useMemo } from "react";
import { Clock3, Search, Sparkles, X } from "lucide-react";
import { AppIcon } from "../AppIcon";
import { identityFor, type AppIdentity } from "../../lib/app-identity";
import {
  activityDisplayRegistry,
  displayMetaForActivity,
  type ActivityDisplayMeta,
} from "../../lib/activity-composition";
import type { ActivityCategory, Segment, VideoPurpose } from "../../lib/metrics";
import { matchesTimelineFilter, timelineFilterLabel, type TimelineFilter } from "../../lib/timeline-filter";
import { PanelHeading } from "./analysis-shared";

const emptyReviewSubjectIds: ReadonlySet<string> = new Set();

function manualClassificationValue(meta: ActivityDisplayMeta): string {
  return meta.category === "video_input"
    ? `video_input:${meta.videoPurpose ?? "unknown"}`
    : meta.category;
}

export function sortTimelineNewestFirst(segments: readonly Segment[]): Segment[] {
  return [...segments].sort((left, right) =>
    right.startMs - left.startMs
    || right.endMs - left.endMs
    || left.id.localeCompare(right.id));
}

export function scrollIntoViewWithHeaderOffset(element: HTMLElement, header: HTMLElement, scrollContainer?: HTMLElement | null): void {
  const breathingRoom = 24;
  const headerHeight = header.getBoundingClientRect().height;
  if (scrollContainer) {
    const top = element.getBoundingClientRect().top - scrollContainer.getBoundingClientRect().top
      + scrollContainer.scrollTop - headerHeight - breathingRoom;
    scrollContainer.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
    return;
  }

  const top = element.getBoundingClientRect().top + window.scrollY - headerHeight - breathingRoom;
  window.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
}

function formatDuration(seconds: number): string {
  const rounded = Math.max(0, Math.round(seconds));
  const hours = Math.floor(rounded / 3_600);
  const minutes = Math.floor((rounded % 3_600) / 60);
  if (hours) return `${hours} 小时 ${minutes} 分钟`;
  if (minutes) return `${minutes} 分钟`;
  return `${rounded} 秒`;
}

function formatClock(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

const inactivityReasonLabels = {
  input_idle: "无输入",
  continuity_gap: "监控断档",
  legacy_gap_repair: "历史推断",
} as const;

export const TimelinePanel = forwardRef<HTMLDivElement, {
  segments: Segment[];
  identities?: ReadonlyMap<string, AppIdentity>;
  filter: TimelineFilter;
  onFilterChange: (filter: TimelineFilter) => void;
  onChangeClassification: (
    segmentId: string,
    category: ActivityCategory,
    videoPurpose: VideoPurpose,
  ) => void;
  reviewSubjectIds?: ReadonlySet<string>;
  onOpenAiReview?: (subjectId: string) => void;
  pulse?: boolean;
  onPulseEnd?: () => void;
}>(function TimelinePanel({ segments, identities = new Map(), filter, onFilterChange, onChangeClassification, reviewSubjectIds = emptyReviewSubjectIds, onOpenAiReview, pulse = false, onPulseEnd }, ref) {
  const visibleSegments = useMemo(
    () => sortTimelineNewestFirst(segments.filter((segment) => matchesTimelineFilter(segment, filter))),
    [segments, filter],
  );

  return <section id="activity-timeline" className={`panel timeline-panel ${pulse ? "target-pulse" : ""}`} onAnimationEnd={onPulseEnd}>
    <div id="activity-timeline-heading" ref={ref} className="timeline-scroll-anchor" data-scroll-offset="header">
      <div className="timeline-head">
        <PanelHeading eyebrow="TIMELINE" title="活动时间线" icon={<Clock3 size={18} />} />
        <div className="filter-row">
          <Search size={17} />
          <select
            aria-label="筛选活动分类"
            value={filter.mode === "category"
              ? displayMetaForActivity(filter.category, filter.videoPurpose ?? "unknown").key
              : "all"}
            onChange={(event) => {
              if (event.target.value === "all") {
                onFilterChange({ mode: "all" });
                return;
              }
              const selected = activityDisplayRegistry.find((meta) => meta.key === event.target.value);
              if (!selected) return;
              onFilterChange({
                mode: "category",
                category: selected.category,
                ...(selected.videoPurpose ? { videoPurpose: selected.videoPurpose } : {}),
              });
            }}
          >
            <option value="all">全部分类</option>
            {activityDisplayRegistry.map((meta) => <option key={meta.key} value={meta.key}>{meta.label}</option>)}
          </select>
          {filter.mode !== "all" && <span className="active-filter">{timelineFilterLabel(filter)} <button aria-label="清除时间线筛选" onClick={() => onFilterChange({ mode: "all" })}><X size={13} /></button></span>}
        </div>
      </div>
    </div>
    <div className="timeline-list">
      {visibleSegments.map((item) => {
        const identity = identityFor(identities, item.app, item.appPath);
        const displayMeta = displayMetaForActivity(item.category, item.videoPurpose);
        return <div className="timeline-row" key={item.id} style={{ "--row-color": displayMeta.color } as React.CSSProperties}>
          <time>{formatClock(item.startMs)} - {formatClock(item.endMs)}</time>
          <div><strong>{item.title}</strong><span style={{ display: "flex", alignItems: "center", gap: 6 }}>{identity.displayName}<AppIcon identity={identity} /></span></div>
          <span className="category-tag">
            <i />{displayMeta.label}
            {item.category === "idle" && item.inactivityReason
              ? <small>· {inactivityReasonLabels[item.inactivityReason]}</small>
              : null}
          </span>
          <span className={`confidence ${item.needsReview ? "review" : ""}`}>{Math.round(item.confidence * 100)}%</span>
          <b>{formatDuration((item.endMs - item.startMs) / 1_000)}</b>
          {reviewSubjectIds.has(item.id) && onOpenAiReview && <button
            type="button"
            className="ai-review-marker timeline-review-marker"
            aria-label={`在 AI 审核中心查看 ${item.title}`}
            title="打开 AI 审核中心"
            data-ai-review-subject={item.id}
            onClick={() => onOpenAiReview(item.id)}
          ><Sparkles size={14} /><span>审核</span></button>}
          <select
            className="inline-classifier"
            aria-label={`修改 ${item.title} 的分类`}
            value={manualClassificationValue(displayMeta)}
            onChange={(event) => {
              const selected = activityDisplayRegistry.find(
                (meta) => manualClassificationValue(meta) === event.target.value,
              );
              if (!selected) return;
              onChangeClassification(
                item.id,
                selected.category,
                selected.videoPurpose ?? "unknown",
              );
            }}
          >
            {activityDisplayRegistry.map((meta) => (
              <option key={meta.key} value={manualClassificationValue(meta)}>{meta.label}</option>
            ))}
          </select>
        </div>;
      })}
    </div>
  </section>;
});
