import { forwardRef, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Clock3, Minus, Plus, Search, Sparkles, X } from "lucide-react";
import { AppIcon } from "../AppIcon";
import { identityFor, type AppIdentity } from "../../lib/app-identity";
import { activityDisplayRegistry, displayMetaForActivity, type ActivityDisplayMeta } from "../../lib/activity-composition";
import type { ActivityCategory, Segment, VideoPurpose } from "../../lib/metrics";
import { matchesTimelineFilter, timelineFilterLabel, type TimelineFilter } from "../../lib/timeline-filter";
import { PanelHeading } from "./analysis-shared";
import { ActivityExplanationLayers } from "./ActivityExplanationLayers";

const DAY_MS = 24 * 3_600_000;
const BUCKET_MS = 5 * 60_000;
const TIMELINE_COLORS = [
  "#2563eb",
  "#ea580c",
  "#059669",
  "#7c3aed",
  "#eab308",
  "#0891b2",
  "#dc2626",
  "#65a30d",
  "#db2777",
  "#4f46e5",
];
export const IDLE_TIMELINE_COLOR = "#94a3b8";
const emptyReviewSubjectIds: ReadonlySet<string> = new Set();

export type TimelineAppBand = { app: string; appPath?: string; milliseconds: number; share: number; color: string };
export type TimelineCluster = { id: string; startMs: number; endMs: number; apps: [TimelineAppBand]; segments: Segment[] };

export function anchoredTimelineScrollLeft(timeRatio: number, scrollWidth: number, pointerViewportX: number): number {
  return Math.max(0, Math.min(scrollWidth, timeRatio * scrollWidth - pointerViewportX));
}

function appKey(app: string, appPath?: string): string { return `${app}\u0000${appPath ?? ""}`; }
function colorForApp(app: string, appPath?: string): string {
  let hash = 0;
  for (const char of appKey(app, appPath)) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return TIMELINE_COLORS[Math.abs(hash) % TIMELINE_COLORS.length];
}

export function buildTimelineClusters(segments: readonly Segment[]): TimelineCluster[] {
  const sorted = [...segments]
    .filter((segment) => segment.endMs > segment.startMs)
    .sort((left, right) => left.startMs - right.startMs || left.endMs - right.endMs || left.id.localeCompare(right.id));
  const buckets: Array<{ startMs: number; endMs: number; winner: TimelineAppBand; totalMilliseconds: number }> = [];
  for (let startMs = 0; startMs < DAY_MS; startMs += BUCKET_MS) {
    const endMs = startMs + BUCKET_MS;
    const totals = new Map<string, { app: string; appPath?: string; milliseconds: number; idleMilliseconds: number }>();
    for (const segment of sorted) {
      const overlap = Math.max(0, Math.min(endMs, segment.endMs) - Math.max(startMs, segment.startMs));
      if (!overlap) continue;
      const key = appKey(segment.app, segment.appPath);
      const current = totals.get(key);
      totals.set(key, {
        app: segment.app,
        appPath: segment.appPath,
        milliseconds: (current?.milliseconds ?? 0) + overlap,
        idleMilliseconds: (current?.idleMilliseconds ?? 0) + (segment.category === "idle" ? overlap : 0),
      });
    }
    const total = [...totals.values()].reduce((sum, item) => sum + item.milliseconds, 0);
    const top = [...totals.values()].sort((left, right) => right.milliseconds - left.milliseconds || left.app.localeCompare(right.app))[0];
    if (top) buckets.push({
      startMs,
      endMs,
      totalMilliseconds: total,
      winner: {
        app: top.app,
        appPath: top.appPath,
        milliseconds: top.milliseconds,
        share: top.milliseconds / total,
        color: top.idleMilliseconds === top.milliseconds ? IDLE_TIMELINE_COLOR : colorForApp(top.app, top.appPath),
      },
    });
  }

  const merged: Array<{ startMs: number; endMs: number; winner: TimelineAppBand; totalMilliseconds: number }> = [];
  for (const bucket of buckets) {
    const previous = merged.at(-1);
    if (previous && previous.endMs === bucket.startMs && appKey(previous.winner.app, previous.winner.appPath) === appKey(bucket.winner.app, bucket.winner.appPath)) {
      previous.endMs = bucket.endMs;
      previous.winner.milliseconds += bucket.winner.milliseconds;
      previous.totalMilliseconds += bucket.totalMilliseconds;
      previous.winner.share = previous.winner.milliseconds / previous.totalMilliseconds;
    } else {
      merged.push({ ...bucket, winner: { ...bucket.winner } });
    }
  }

  return merged.map((item) => ({
    id: `${item.startMs}-${item.endMs}-${appKey(item.winner.app, item.winner.appPath)}`,
    startMs: item.startMs,
    endMs: item.endMs,
    apps: [item.winner],
    segments: sorted.filter((segment) => segment.startMs < item.endMs && segment.endMs > item.startMs),
  }));
}

export function scrollIntoViewWithHeaderOffset(element: HTMLElement, header: HTMLElement, scrollContainer?: HTMLElement | null): void {
  const breathingRoom = 24;
  const headerHeight = header.getBoundingClientRect().height;
  if (scrollContainer) {
    const top = element.getBoundingClientRect().top - scrollContainer.getBoundingClientRect().top + scrollContainer.scrollTop - headerHeight - breathingRoom;
    scrollContainer.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
    return;
  }
  const top = element.getBoundingClientRect().top + window.scrollY - headerHeight - breathingRoom;
  window.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
}

function manualClassificationValue(meta: ActivityDisplayMeta): string { return meta.category === "video_input" ? `video_input:${meta.videoPurpose ?? "unknown"}` : meta.category; }
function formatDuration(seconds: number): string { const value = Math.max(0, Math.round(seconds)); const h = Math.floor(value / 3600); const m = Math.floor(value % 3600 / 60); return h ? `${h} 小时 ${m} 分钟` : m ? `${m} 分钟` : `${value} 秒`; }
function formatClock(ms: number): string { const minutes = Math.round(ms / 60_000); return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`; }
const inactivityReasonLabels = { input_idle: "无输入", continuity_gap: "监控断档", legacy_gap_repair: "历史推断" } as const;

export const TimelinePanel = forwardRef<HTMLDivElement, {
  segments: Segment[]; identities?: ReadonlyMap<string, AppIdentity>; filter: TimelineFilter;
  onFilterChange: (filter: TimelineFilter) => void;
  onChangeClassification: (segmentId: string, category: ActivityCategory, videoPurpose: VideoPurpose) => void;
  reviewSubjectIds?: ReadonlySet<string>; onOpenAiReview?: (subjectId: string) => void; pulse?: boolean; onPulseEnd?: () => void;
}>(function TimelinePanel({ segments, identities = new Map(), filter, onFilterChange, onChangeClassification, reviewSubjectIds = emptyReviewSubjectIds, onOpenAiReview, pulse = false, onPulseEnd }, ref) {
  const visibleSegments = useMemo(() => segments.filter((segment) => matchesTimelineFilter(segment, filter)), [segments, filter]);
  const clusters = useMemo(() => buildTimelineClusters(visibleSegments), [visibleSegments]);
  const [zoom, setZoom] = useState(1);
  const [dragging, setDragging] = useState(false);
  const timelineScrollRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ pointerId: number; clientX: number; scrollLeft: number } | null>(null);
  const zoomAnchorRef = useRef<{ timeRatio: number; pointerViewportX: number } | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(() => clusters[0]?.id ?? null);
  const selected = clusters.find((cluster) => cluster.id === selectedId) ?? null;
  const ticks = Array.from({ length: 25 }, (_, hour) => hour);

  useLayoutEffect(() => {
    const scroll = timelineScrollRef.current;
    const anchor = zoomAnchorRef.current;
    if (!scroll || !anchor) return;
    scroll.scrollLeft = anchoredTimelineScrollLeft(anchor.timeRatio, scroll.scrollWidth, anchor.pointerViewportX);
    zoomAnchorRef.current = null;
  }, [zoom]);

  return <section id="activity-timeline" className={`panel timeline-panel orbit-timeline-panel ${pulse ? "target-pulse" : ""}`} onAnimationEnd={onPulseEnd}>
    <div id="activity-timeline-heading" ref={ref} className="timeline-scroll-anchor" data-scroll-offset="header">
      <div className="timeline-head">
        <PanelHeading eyebrow="DAY ORBIT" title="活跃时间线" icon={<Clock3 size={18} />} />
        <div className="timeline-tools">
          <div className="timeline-zoom" aria-label="缩放时间线">
            <button type="button" aria-label="缩小时间线" disabled={zoom <= 1} onClick={() => setZoom((value) => Math.max(1, value - .5))}><Minus size={14} /></button>
            <input type="range" min="1" max="6" step=".5" value={zoom} aria-label="时间线缩放级别" onChange={(event) => setZoom(Number(event.target.value))} />
            <button type="button" aria-label="放大时间线" disabled={zoom >= 6} onClick={() => setZoom((value) => Math.min(6, value + .5))}><Plus size={14} /></button>
            <output>{zoom.toFixed(zoom % 1 ? 1 : 0)}×</output>
          </div>
          <div className="filter-row"><Search size={16} /><select aria-label="筛选活动分类" value={filter.mode === "category" ? displayMetaForActivity(filter.category, filter.videoPurpose ?? "unknown").key : "all"} onChange={(event) => {
            if (event.target.value === "all") return onFilterChange({ mode: "all" });
            const next = activityDisplayRegistry.find((meta) => meta.key === event.target.value);
            if (next) onFilterChange({ mode: "category", category: next.category, ...(next.videoPurpose ? { videoPurpose: next.videoPurpose } : {}) });
          }}><option value="all">全部分类</option>{activityDisplayRegistry.map((meta) => <option key={meta.key} value={meta.key}>{meta.label}</option>)}</select>
          {filter.mode !== "all" && <span className="active-filter">{timelineFilterLabel(filter)} <button aria-label="清除时间线筛选" onClick={() => onFilterChange({ mode: "all" })}><X size={13} /></button></span>}</div>
        </div>
      </div>
    </div>

    <div
      ref={timelineScrollRef}
      className={`orbit-timeline-scroll ${dragging ? "dragging" : ""}`}
      aria-label="可缩放的全天应用时间线，每格五分钟，仅显示占比最高的应用"
      onWheel={(event) => {
        if (!event.deltaY) return;
        const nextZoom = Math.max(1, Math.min(6, zoom + (event.deltaY < 0 ? .5 : -.5)));
        if (nextZoom === zoom) return;
        event.preventDefault();
        const scroll = timelineScrollRef.current;
        if (!scroll) return setZoom(nextZoom);
        const viewportX = event.clientX - scroll.getBoundingClientRect().left;
        const timeRatio = Math.max(0, Math.min(1, (scroll.scrollLeft + viewportX) / scroll.scrollWidth));
        zoomAnchorRef.current = { timeRatio, pointerViewportX: viewportX };
        setZoom(nextZoom);
      }}
      onPointerDown={(event) => {
        if (event.button !== 0 || (event.target as HTMLElement).closest("button, input, select")) return;
        dragRef.current = { pointerId: event.pointerId, clientX: event.clientX, scrollLeft: event.currentTarget.scrollLeft };
        event.currentTarget.setPointerCapture(event.pointerId);
        setDragging(true);
      }}
      onPointerMove={(event) => {
        if (!dragRef.current || dragRef.current.pointerId !== event.pointerId) return;
        event.currentTarget.scrollLeft = dragRef.current.scrollLeft - (event.clientX - dragRef.current.clientX);
      }}
      onPointerUp={(event) => {
        if (dragRef.current?.pointerId !== event.pointerId) return;
        dragRef.current = null;
        event.currentTarget.releasePointerCapture(event.pointerId);
        setDragging(false);
      }}
      onPointerCancel={() => { dragRef.current = null; setDragging(false); }}
    >
      <div className="orbit-timeline-canvas" style={{ width: `${zoom * 100}%` }}>
        <div className="orbit-time-ruler" aria-hidden="true">{ticks.map((hour) => <time key={hour} style={{ left: `${hour / 24 * 100}%` }}>{String(hour).padStart(2, "0")}:00</time>)}</div>
        <div className="orbit-color-track">
          {ticks.slice(0, -1).map((hour) => <i className="orbit-hour-grid" key={hour} style={{ left: `${hour / 24 * 100}%` }} />)}
          {clusters.map((cluster) => <button
            type="button" key={cluster.id} className={`orbit-cluster ${selectedId === cluster.id ? "selected" : ""}`}
            style={{ left: `${cluster.startMs / DAY_MS * 100}%`, width: `${Math.max((cluster.endMs - cluster.startMs) / DAY_MS * 100, .12)}%` }}
            aria-pressed={selectedId === cluster.id}
            aria-label={`${formatClock(cluster.startMs)} 至 ${formatClock(cluster.endMs)}，${cluster.apps.map((app) => `${app.app} ${Math.round(app.share * 100)}%`).join("，")}`}
            title={`${formatClock(cluster.startMs)}–${formatClock(cluster.endMs)}\n${cluster.apps.map((app) => `${app.app} · ${Math.round(app.share * 100)}%`).join("\n")}`}
            onClick={() => setSelectedId((value) => value === cluster.id ? null : cluster.id)}
          ><span style={{ background: cluster.apps[0].color }} /></button>)}
        </div>
        <div className="orbit-track-caption"><span>00:00</span><b>滚轮缩放 · 拖动平移 · 5 分钟 TOP 1</b><span>24:00</span></div>
      </div>
    </div>

    <div className="orbit-timeline-legend" aria-label="应用颜色图例">{[...new Map(clusters.flatMap((cluster) => cluster.apps).map((app) => [appKey(app.app, app.appPath), app])).values()].slice(0, 10).map((app) => { const identity = identityFor(identities, app.app, app.appPath); return <button type="button" key={appKey(app.app, app.appPath)} onClick={() => onFilterChange({ mode: "app", app: app.app, ...(app.appPath ? { appPath: app.appPath } : {}) })}><i style={{ background: app.color }} /><span>{identity.displayName}</span></button>; })}</div>

    {selected ? <div className="orbit-selection" aria-live="polite"><header><div><span>SELECTED RANGE</span><h3>{formatClock(selected.startMs)}–{formatClock(selected.endMs)}</h3></div><div className="orbit-selection-apps">{selected.apps.map((app) => { const identity = identityFor(identities, app.app, app.appPath); return <span key={appKey(app.app, app.appPath)}><AppIcon identity={identity} /><b>{identity.displayName}</b><small>{Math.round(app.share * 100)}%</small></span>; })}</div></header>
      <div className="hour-segment-list">{selected.segments.map((item) => { const identity = identityFor(identities, item.app, item.appPath); const meta = displayMetaForActivity(item.category, item.videoPurpose); return <div className="hour-segment" key={item.id} style={{ "--row-color": meta.color } as React.CSSProperties}><time>{formatClock(item.startMs)}–{formatClock(item.endMs)}</time><AppIcon identity={identity} /><div><strong>{item.title}</strong><span>{identity.displayName}</span></div><span className="category-tag"><i />{meta.label}{item.category === "idle" && item.inactivityReason ? <small> · {inactivityReasonLabels[item.inactivityReason]}</small> : null}</span><b>{formatDuration((item.endMs - item.startMs) / 1000)}</b>{reviewSubjectIds.has(item.id) && onOpenAiReview && <button type="button" className="ai-review-marker timeline-review-marker" aria-label={`在 AI 审核中心查看 ${item.title}`} data-ai-review-subject={item.id} onClick={() => onOpenAiReview(item.id)}><Sparkles size={14} /><span>审核</span></button>}<select className="inline-classifier" aria-label={`修改 ${item.title} 的分类`} value={manualClassificationValue(meta)} onChange={(event) => { const next = activityDisplayRegistry.find((candidate) => manualClassificationValue(candidate) === event.target.value); if (next) onChangeClassification(item.id, next.category, next.videoPurpose ?? "unknown"); }}>{activityDisplayRegistry.map((candidate) => <option key={candidate.key} value={manualClassificationValue(candidate)}>{candidate.label}</option>)}</select><ActivityExplanationLayers segment={item} /></div>; })}</div>
    </div> : <p className="orbit-timeline-hint">滚轮缩放、按住空白处拖动平移；每 5 分钟只显示占用最高的应用。</p>}
  </section>;
});
