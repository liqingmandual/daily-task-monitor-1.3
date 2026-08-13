import { useEffect, useMemo, useRef, useState } from "react";
import { RefreshCcw } from "lucide-react";
import {
  exportTrendMarkdown,
  exportTrendReport,
  isTrendWorkbenchError,
  isDesktopRuntime,
  listenAnalysisChanged,
  loadTrendResearchAnalysis,
  loadTrendRange,
  loadTrendWorkbench,
  queueTrendResearchAnalysis,
  type TrendGranularity,
  type TrendMetric,
  type TrendMetricValues,
  type TrendPayload,
  type TrendResearchAnalysis,
  type ReportFormat,
  type TrendWorkbenchPayload,
  type TrendWorkbenchRequest,
} from "../../lib/desktop";
import {
  addTrendCalendarDays,
  defaultTrendGranularityForSelectedDates,
  defaultTrendGranularity,
  inclusiveTrendDayCount,
  normalizeTrendSelectedDates,
  resolveTrendRange,
  resolveTrendSelectedDates,
  shiftTrendSelectedDates,
  shiftTrendRange,
  type TrendCustomMode,
  type TrendPreset,
  type TrendRangeSelection,
} from "../../lib/trend-range";
import { buildLocalTrendAnalysis, buildTrendStatisticsDto } from "../../lib/trend-analysis";
import { TrendResearchAnalysis as TrendResearchAnalysisPanel } from "./TrendResearchAnalysis";
import { TrendTimelineChart } from "./TrendTimelineChart";
import { TrendRangeToolbar } from "./TrendRangeToolbar";
import { TrendStatisticsStrip } from "./TrendStatisticsStrip";
import { TrendComparisonChart } from "./TrendComparisonChart";
import { TrendDrilldown } from "./TrendDrilldown";
import { buildTrendChartPoints, TrendStackedTimeChart, TrendWeekdayChart } from "./TrendDashboardCharts";
import { DonutChart } from "../today/analysis-shared";
import {
  ACTIVITY_SCOPE_STORAGE_KEYS,
  activityDisplayRegistry,
  compositionToDonutItems,
  compositionForScope,
  parsePersistedActivityScope,
  type ActivityComposition,
  type ActivityDisplayKey,
  type ActivityScope,
} from "../../lib/activity-composition";

export type TrendLoadStatus = "loading" | "ready" | "error";

const focusMetricLabels: Record<TrendMetric, string> = {
  monitoredSeconds: "监控时长",
  activeSeconds: "活跃时长",
  learningSeconds: "学习时长",
  idleSeconds: "不活跃时长",
  switchCount: "切换次数",
  longestFocusSeconds: "最长专注块",
  classificationCoverage: "分类覆盖率",
  completedTaskCount: "任务完成数",
  linkedTaskSeconds: "已关联任务时长",
};

export interface TrendLoadState {
  rangeKey: string;
  status: TrendLoadStatus;
  payload: TrendPayload | null;
  error: string;
}

type TrendRangeLoader = (startDate: string, endDate: string) => Promise<TrendPayload>;
type TrendAnalysisLoader = (request: TrendWorkbenchRequest) => Promise<TrendResearchAnalysis>;
type TrendAnalysisQueuer = (request: TrendWorkbenchRequest, force: boolean) => Promise<string | null>;
type TrendMarkdownExporter = (request: TrendWorkbenchRequest) => Promise<string>;
type TrendReportExporter = (request: TrendWorkbenchRequest, format: ReportFormat) => Promise<string | null>;

export interface TrendAnalysisLoadState {
  status: TrendLoadStatus;
  evidenceHash: string;
  analysis: TrendResearchAnalysis | null;
  error: string;
}

export interface TrendActionState {
  evidenceHash: string;
  status: "idle" | "pending" | "success" | "error";
  message: string;
}

export interface TrendWorkbenchLoadState {
  requestKey: string;
  status: TrendLoadStatus;
  payload: TrendWorkbenchPayload | null;
  error: string;
}

type TrendWorkbenchLoader = (request: TrendWorkbenchRequest) => Promise<TrendWorkbenchPayload>;

export function trendWorkbenchRequestKey(request: TrendWorkbenchRequest): string {
  return JSON.stringify(request);
}

export function trendAnalysisEvidenceHash(payload: TrendWorkbenchPayload): string {
  return payload.analysisEvidenceHash ?? payload.evidenceHash;
}

export function visibleTrendWorkbenchLoadState(state: TrendWorkbenchLoadState, requestKey: string): TrendWorkbenchLoadState {
  return state.requestKey === requestKey
    ? state
    : { requestKey, status: "loading", payload: null, error: "" };
}

export function createTrendWorkbenchRequestCoordinator(loader: TrendWorkbenchLoader, onState: (state: TrendWorkbenchLoadState) => void) {
  let requestVersion = 0;
  return {
    cancel() { requestVersion += 1; },
    invalidate(state: TrendWorkbenchLoadState) { requestVersion += 1; onState(state); },
    async load(request: TrendWorkbenchRequest) {
      const version = ++requestVersion;
      const requestKey = trendWorkbenchRequestKey(request);
      onState({ requestKey, status: "loading", payload: null, error: "" });
      try {
        const result = await loader(request);
        if (version !== requestVersion) return;
        onState({ requestKey, status: "ready", payload: result, error: "" });
      } catch (error) {
        if (version !== requestVersion) return;
        const message = isTrendWorkbenchError(error)
          ? error.message
          : error instanceof Error ? error.message : String(error);
        onState({ requestKey, status: "error", payload: null, error: message });
      }
    },
  };
}

export function trendRangeKey(range: TrendRangeSelection): string {
  return `${range.startDate}\n${range.endDate}`;
}

function loadingTrendState(rangeKey: string): TrendLoadState {
  return { rangeKey, status: "loading", payload: null, error: "" };
}

export function visibleTrendLoadState(state: TrendLoadState, selectedRangeKey: string): TrendLoadState {
  return state.rangeKey === selectedRangeKey ? state : loadingTrendState(selectedRangeKey);
}

export function createTrendRangeRequestCoordinator(
  loader: TrendRangeLoader,
  onState: (state: TrendLoadState) => void,
) {
  let requestVersion = 0;
  return {
    cancel() {
      requestVersion += 1;
    },
    invalidate(state: TrendLoadState) {
      requestVersion += 1;
      onState(state);
    },
    async load(range: TrendRangeSelection): Promise<void> {
      const version = ++requestVersion;
      const rangeKey = trendRangeKey(range);
      onState(loadingTrendState(rangeKey));
      try {
        const payload = await loader(range.startDate, range.endDate);
        if (version !== requestVersion) return;
        onState({ rangeKey, status: "ready", payload, error: "" });
      } catch (loadError) {
        if (version !== requestVersion) return;
        onState({
          rangeKey,
          status: "error",
          payload: null,
          error: loadError instanceof Error ? loadError.message : String(loadError),
        });
      }
    },
  };
}

export function createTrendAnalysisRequestCoordinator(
  loader: TrendAnalysisLoader,
  onState: (state: TrendAnalysisLoadState) => void,
) {
  let requestVersion = 0;
  return {
    cancel() {
      requestVersion += 1;
    },
    async load(payload: TrendWorkbenchPayload, request: TrendWorkbenchRequest): Promise<void> {
      const version = ++requestVersion;
      const evidenceHash = trendAnalysisEvidenceHash(payload);
      onState({ status: "loading", evidenceHash, analysis: null, error: "" });
      try {
        const analysis = await loader(request);
        if (version !== requestVersion) return;
        const requestScope = request.activityScope ?? "all";
        const analysisScope = analysis.activityScope ?? "all";
        const matches = analysis.evidenceHash === evidenceHash && analysisScope === requestScope;
        onState({
          status: "ready",
          evidenceHash,
          analysis: matches ? analysis : null,
          error: matches ? "" : analysisScope !== requestScope
            ? "分析口径与当前选择不匹配，已保留本地评价"
            : "分析证据已更新，已保留本地评价",
        });
      } catch (loadError) {
        if (version !== requestVersion) return;
        onState({
          status: "error",
          evidenceHash,
          analysis: null,
          error: loadError instanceof Error ? loadError.message : String(loadError),
        });
      }
    },
  };
}

export function createTrendActionRequestCoordinator(onState: (state: TrendActionState) => void) {
  let requestVersion = 0;
  return {
    activate(evidenceHash: string) {
      requestVersion += 1;
      onState({ evidenceHash, status: "idle", message: "" });
    },
    cancel() {
      requestVersion += 1;
    },
    async run<T>(
      evidenceHash: string,
      pendingMessage: string,
      action: () => Promise<T>,
      successMessage: (result: T) => string,
      errorLabel = "操作失败",
    ): Promise<void> {
      const version = ++requestVersion;
      onState({ evidenceHash, status: "pending", message: pendingMessage });
      try {
        const result = await action();
        if (version !== requestVersion) return;
        onState({ evidenceHash, status: "success", message: successMessage(result) });
      } catch (actionError) {
        if (version !== requestVersion) return;
        const detail = actionError instanceof Error ? actionError.message : String(actionError);
        onState({ evidenceHash, status: "error", message: `${errorLabel}：${detail}` });
      }
    },
  };
}

interface TrendWorkbenchProps {
  preset: TrendPreset;
  range: TrendRangeSelection | null;
  customStart: string;
  customEnd: string;
  customMode?: TrendCustomMode;
  specificDraftDates?: string[];
  specificAppliedDates?: string[];
  specificSelectionError?: string;
  status: TrendLoadStatus;
  payload: TrendPayload | null;
  workbenchStatus?: TrendLoadStatus;
  workbenchPayload?: TrendWorkbenchPayload | null;
  workbenchError?: string;
  granularity?: TrendGranularity;
  metric?: TrendMetric;
  customBaselineEnabled?: boolean;
  customBaselineStart?: string;
  customBaselineEnd?: string;
  analysisStatus?: TrendLoadStatus;
  analysis?: TrendResearchAnalysis | null;
  analysisError?: string;
  activityScope?: ActivityScope;
  nativeActionsAvailable?: boolean;
  reanalysisFeedback?: TrendActionState;
  exportFeedback?: TrendActionState;
  error: string;
  onPresetChange: (preset: TrendPreset) => void;
  onCustomStartChange: (value: string) => void;
  onCustomEndChange: (value: string) => void;
  onCustomModeChange?: (value: TrendCustomMode) => void;
  onSpecificDraftDatesChange?: (dates: string[]) => void;
  onClearSpecificDates?: () => void;
  onApplySpecificDates?: () => void;
  onRemoveSpecificDate?: (date: string) => void;
  onShift: (direction: -1 | 1) => void;
  onRefresh: () => void;
  onReanalyze?: () => void;
  onActivityScopeChange?: (value: ActivityScope) => void;
  onExport?: (format: ReportFormat) => void;
  onGranularityChange?: (value: TrendGranularity) => void;
  onMetricChange?: (value: TrendMetric) => void;
  onCustomBaselineEnabledChange?: (value: boolean) => void;
  onCustomBaselineStartChange?: (value: string) => void;
  onCustomBaselineEndChange?: (value: string) => void;
  onOpenDate?: (date: string) => void;
  onOpenTask?: (taskId: string) => void;
  formatDuration?: (seconds: number) => string;
}

interface TrendsViewProps {
  anchorDate: string;
  formatDuration: (seconds: number) => string;
  autoAnalysisEnabled?: boolean;
  loadRange?: TrendRangeLoader;
  loadWorkbench?: TrendWorkbenchLoader;
  loadAnalysis?: TrendAnalysisLoader;
  queueAnalysis?: TrendAnalysisQueuer;
  exportMarkdown?: TrendMarkdownExporter;
  exportDocument?: TrendReportExporter;
  desktopRuntime?: () => boolean;
  onOpenDate?: (date: string) => void;
  onOpenTask?: (taskId: string) => void;
}

function fallbackDuration(seconds: number): string {
  const rounded = Math.max(0, Math.round(seconds));
  const hours = Math.floor(rounded / 3_600);
  const minutes = Math.floor((rounded % 3_600) / 60);
  if (hours) return `${hours} 小时 ${minutes} 分钟`;
  if (minutes) return `${minutes} 分钟`;
  return `${rounded} 秒`;
}

const previewMetrics: TrendMetric[] = [
  "monitoredSeconds", "activeSeconds", "learningSeconds", "idleSeconds", "switchCount",
  "longestFocusSeconds", "classificationCoverage", "completedTaskCount", "linkedTaskSeconds",
];

function addPreviewDays(value: string, days: number): string {
  const date = new Date(`${value}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

const inactivityReasonLabels = {
  input_idle: "无输入",
  continuity_gap: "监控断档",
  legacy_gap_repair: "历史推断",
} as const;

function previewActivityComposition(activeSeconds: number, idleSeconds: number) {
  const active = Math.max(0, Math.round(activeSeconds));
  const idle = Math.max(0, Math.round(idleSeconds));
  return {
    all: {
      totalSeconds: active + idle,
      items: [
        ...(active ? [{ key: "creation_development" as const, category: "creation_development" as const, videoPurpose: null, seconds: active, share: active / (active + idle), meaningfulReason: "core" as const }] : []),
        ...(idle ? [{ key: "idle" as const, category: "idle" as const, videoPurpose: null, seconds: idle, share: idle / (active + idle), meaningfulReason: "excluded" as const }] : []),
      ],
    },
    meaningful: {
      totalSeconds: active,
      items: active ? [{ key: "creation_development" as const, category: "creation_development" as const, videoPurpose: null, seconds: active, share: 1, meaningfulReason: "core" as const }] : [],
    },
  };
}

function emptyActivityComposition(): ActivityComposition {
  return { totalSeconds: 0, items: [] };
}

function legacyAllActivityComposition(
  payload: TrendPayload | null,
  workbenchPayload: TrendWorkbenchPayload,
): ActivityComposition {
  const source = payload?.summary.categoryBreakdown?.length
    ? payload.summary.categoryBreakdown.map((item) => ({ key: item.name, seconds: item.seconds }))
    : [...workbenchPayload.buckets.reduce((map, bucket) => {
      for (const item of bucket.drilldown.categoryDistribution) {
        map.set(item.key, (map.get(item.key) ?? 0) + item.seconds);
      }
      return map;
    }, new Map<string, number>())].map(([key, seconds]) => ({ key, seconds }));
  const secondsByKey = new Map<ActivityDisplayKey, number>();
  for (const item of source) {
    const meta = activityDisplayRegistry.find((candidate) => candidate.key === item.key)
      ?? activityDisplayRegistry.find((candidate) => candidate.category === item.key
        && (candidate.category !== "video_input" || candidate.key === "unknown_video"));
    if (!meta || item.seconds <= 0) continue;
    secondsByKey.set(meta.key, (secondsByKey.get(meta.key) ?? 0) + item.seconds);
  }
  const totalSeconds = [...secondsByKey.values()].reduce((sum, seconds) => sum + seconds, 0);
  return {
    totalSeconds,
    items: activityDisplayRegistry.flatMap((meta) => {
      const seconds = secondsByKey.get(meta.key) ?? 0;
      return seconds ? [{
        key: meta.key,
        category: meta.category,
        videoPurpose: meta.videoPurpose,
        seconds,
        share: totalSeconds ? seconds / totalSeconds : 0,
        meaningfulReason: meta.meaningfulReason,
      }] : [];
    }).sort((left, right) => right.seconds - left.seconds),
  };
}

function TrendCompositionChart({
  payload,
  workbenchPayload,
  formatDuration,
  activityScope,
  onActivityScopeChange,
  onSelectCategory,
}: {
  payload: TrendPayload | null;
  workbenchPayload: TrendWorkbenchPayload;
  formatDuration: (seconds: number) => string;
  activityScope: ActivityScope;
  onActivityScopeChange: (value: ActivityScope) => void;
  onSelectCategory: (value: ActivityDisplayKey) => void;
}) {
  const composition = workbenchPayload.activityComposition
    ? compositionForScope(workbenchPayload.activityComposition, activityScope)
    : activityScope === "all" || activityScope === "active"
      ? compositionForScope({
        all: legacyAllActivityComposition(payload, workbenchPayload),
        meaningful: emptyActivityComposition(),
      }, activityScope)
      : emptyActivityComposition();
  const items = compositionToDonutItems(composition);
  return <article className="trend-composition-card">
    <header>
      <div><span>活动构成</span><strong>{activityScope === "all" ? "完整时间口径" : activityScope === "active" ? "活跃" : "学习"}</strong></div>
      <div className="trend-composition-toggle" role="group" aria-label="趋势活动构成口径">
        <button type="button" aria-pressed={activityScope === "all"} onClick={() => onActivityScopeChange("all")}>全部</button>
        <button type="button" aria-pressed={activityScope === "active"} onClick={() => onActivityScopeChange("active")}>活跃</button>
        <button type="button" aria-pressed={activityScope === "meaningful"} onClick={() => onActivityScopeChange("meaningful")}>学习</button>
      </div>
    </header>
    {items.length ? <DonutChart
      items={items}
      ariaLabel="趋势活动构成"
      centerLabel="合计"
      centerValue={formatDuration(composition.totalSeconds)}
      onSelect={(item) => onSelectCategory(item.key as ActivityDisplayKey)}
    /> : <p>当前口径暂无可展示数据。</p>}
    <ul>
      {items.slice(0, 6).map((item) => <li key={item.key}>
        <i style={{ background: item.color }} />
        <span>{item.name}</span>
        <strong>
          总计 {formatDuration(item.value)} · 日均 {formatDuration(item.value / Math.max(1, workbenchPayload.range.selectedDateCount ?? workbenchPayload.range.dayCount))}
          {" · "}占总监测 {workbenchPayload.summary.totals.monitoredSeconds > 0 ? `${(item.value / workbenchPayload.summary.totals.monitoredSeconds * 100).toFixed(1)}%` : "—"}
          {" · "}构成 {(item.share * 100).toFixed(0)}%
        </strong>
      </li>)}
    </ul>
    {activityScope === "all" && workbenchPayload.inactivityReasonDistribution?.length ? (
      <details className="trend-inactivity-reasons">
        <summary>不活跃原因分布</summary>
        <dl>
          {workbenchPayload.inactivityReasonDistribution.map((item) => (
            <div key={item.reason}>
              <dt>{inactivityReasonLabels[item.reason]}</dt>
              <dd>{formatDuration(item.seconds)}</dd>
            </div>
          ))}
        </dl>
      </details>
    ) : null}
  </article>;
}

function datesInTrendRange(range: TrendRangeSelection): string[] {
  if (!range.startDate || !range.endDate) return [];
  const dates: string[] = [];
  for (let date = range.startDate; date <= range.endDate; date = addTrendCalendarDays(date, 1)) {
    if (dates.length === 366) throw new RangeError("日期范围最多支持 366 天");
    dates.push(date);
  }
  return dates;
}

function previewMetricValues(index: number): TrendMetricValues {
  const normalizedIndex = ((index % 7) + 7) % 7;
  const activeSeconds = [12_600, 15_300, 10_800, 16_200, 14_400, 8_100, 13_500][normalizedIndex];
  const monitoredSeconds = activeSeconds + 3_600;
  return {
    monitoredSeconds,
    activeSeconds,
    learningSeconds: Math.round(activeSeconds * [0.42, 0.55, 0.48, 0.62, 0.51, 0.36, 0.58][normalizedIndex]),
    idleSeconds: monitoredSeconds - activeSeconds,
    switchCount: [16, 11, 19, 9, 14, 22, 10][normalizedIndex],
    longestFocusSeconds: [2_400, 3_300, 1_800, 4_200, 2_700, 1_500, 3_600][normalizedIndex],
    classificationCoverage: [0.92, 0.96, 0.84, 0.97, 0.89, 0.78, 0.94][normalizedIndex],
    completedTaskCount: [2, 3, 1, 4, 2, 0, 3][normalizedIndex],
    linkedTaskSeconds: Math.round(activeSeconds * [0.58, 0.72, 0.46, 0.81, 0.64, 0.32, 0.76][normalizedIndex]),
  };
}

function sumPreviewValues(values: TrendMetricValues[]): TrendMetricValues {
  const totals = Object.fromEntries(previewMetrics.map((metric) => [metric, values.reduce((sum, item) => sum + item[metric], 0)])) as unknown as TrendMetricValues;
  totals.classificationCoverage = values.reduce((sum, item) => sum + item.classificationCoverage, 0) / Math.max(values.length, 1);
  totals.longestFocusSeconds = Math.max(0, ...values.map((item) => item.longestFocusSeconds));
  return totals;
}

export function buildPreviewTrendPayload(selection: TrendRangeSelection | TrendWorkbenchRequest): TrendPayload {
  return buildDynamicPreviewTrendPayload(selection);
}

export function buildPreviewWorkbenchPayload(request: TrendWorkbenchRequest): TrendWorkbenchPayload {
  return buildDynamicPreviewWorkbenchPayload(request);
}

type PreviewFact = { date: string; values: TrendMetricValues; recorded: boolean };

function previewDateFacts(range: TrendRangeSelection, offset = 0, selectedDates?: string[]): PreviewFact[] {
  const facts: PreviewFact[] = [];
  const selected = selectedDates ? new Set(selectedDates) : null;
  let envelopeIndex = 0;
  for (let date = range.startDate; date <= range.endDate; date = addPreviewDays(date, 1), envelopeIndex += 1) {
    if (envelopeIndex === 366) throw new RangeError("预览范围最多支持 366 天");
    if (selected && !selected.has(date)) continue;
    const index = envelopeIndex + offset;
    facts.push({ date, values: previewMetricValues(index), recorded: index % 7 !== 5 });
  }
  return facts;
}

function previewRangeFromFacts(range: TrendRangeSelection, facts: PreviewFact[], selectedDates?: string[]) {
  return {
    startMs: Date.parse(`${range.startDate}T00:00:00Z`),
    endMs: Date.parse(`${addPreviewDays(range.endDate, 1)}T00:00:00Z`),
    startDate: range.startDate,
    endDate: range.endDate,
    dayCount: facts.length,
    ...(selectedDates ? {
      selectionMode: "selectedDates" as const,
      selectedDates,
      selectedDateCount: selectedDates.length,
      envelopeDayCount: inclusiveTrendDayCount(range),
    } : { selectionMode: "continuous" as const }),
  };
}

function previewAggregate(facts: PreviewFact[]) {
  const recorded = facts.filter((fact) => fact.recorded);
  return { recorded, missingDayCount: facts.length - recorded.length, totals: sumPreviewValues(recorded.map((fact) => fact.values)) };
}

function previewDailyStatistic(facts: PreviewFact[], statistic: (values: number[]) => number): TrendMetricValues {
  const recorded = facts.filter((fact) => fact.recorded);
  return Object.fromEntries(previewMetrics.map((metric) => [metric, statistic(recorded.map((fact) => fact.values[metric]))])) as unknown as TrendMetricValues;
}

function previousMonthDate(date: string): string {
  const [year, month, day] = date.split("-").map(Number);
  const target = new Date(Date.UTC(year, month - 2, 1));
  target.setUTCDate(Math.min(day, new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate()));
  return target.toISOString().slice(0, 10);
}

function previewBaselineSeries(kind: "current" | "previousEqualLength" | "previousMonthSamePeriod" | "custom", range: TrendRangeSelection, currentValue: number | null, metric: TrendMetric, offset: number, selectedDates?: string[]) {
  const aggregate = previewAggregate(previewDateFacts(range, offset, selectedDates));
  const value = aggregate.recorded.length ? aggregate.totals[metric] : null;
  const absoluteDelta = value === null || currentValue === null ? null : currentValue - value;
  return { kind, range, isValid: value !== null, value, absoluteDelta, percentDelta: absoluteDelta === null || value === null || value === 0 ? null : absoluteDelta / value * 100, recordedDayCount: aggregate.recorded.length, missingDayCount: aggregate.missingDayCount, evidenceIds: aggregate.recorded.map((fact) => `preview-${kind}-${fact.date}`) };
}

function buildDynamicPreviewTrendPayload(selection: TrendRangeSelection | TrendWorkbenchRequest): TrendPayload {
  const range = { startDate: selection.startDate, endDate: selection.endDate };
  const selectedDates = "selectedDates" in selection && selection.selectedDates?.length
    ? normalizeTrendSelectedDates(selection.selectedDates)
    : undefined;
  const facts = previewDateFacts(range, 0, selectedDates);
  const aggregate = previewAggregate(facts);
  const envelopeDayCount = inclusiveTrendDayCount(range);
  const previousRange = { startDate: addPreviewDays(range.startDate, -envelopeDayCount), endDate: addPreviewDays(range.endDate, -envelopeDayCount) };
  const previousDates = selectedDates?.map((date) => addPreviewDays(date, -envelopeDayCount));
  const previousFacts = previewDateFacts(previousRange, -1, previousDates);
  const previous = previewAggregate(previousFacts);
  const ratio = (numerator: number, denominator: number) => denominator ? numerator / denominator : 0;
  const delta = (current: number, prior: number) => prior ? (current - prior) / prior * 100 : null;
  return { range: previewRangeFromFacts(range, facts, selectedDates), days: facts.map((fact) => ({ date: fact.date, label: ["周日", "周一", "周二", "周三", "周四", "周五", "周六"][new Date(`${fact.date}T00:00:00Z`).getUTCDay()], monitoredSeconds: fact.recorded ? fact.values.monitoredSeconds : 0, activeSeconds: fact.recorded ? fact.values.activeSeconds : 0, idleSeconds: fact.recorded ? fact.values.idleSeconds : 0, learningSeconds: fact.recorded ? fact.values.learningSeconds : 0, switchCount: fact.recorded ? fact.values.switchCount : 0, longestFocusSeconds: fact.recorded ? fact.values.longestFocusSeconds : 0, completedTaskCount: fact.recorded ? fact.values.completedTaskCount : 0, classificationCoverage: fact.recorded ? fact.values.classificationCoverage : 0, topCategory: fact.recorded ? { name: "creation_development", seconds: fact.values.learningSeconds, share: .52 } : null, topApp: fact.recorded ? { name: "Codex", seconds: fact.values.activeSeconds, share: .64 } : null })), summary: { monitoredSeconds: aggregate.totals.monitoredSeconds, activeSeconds: aggregate.totals.activeSeconds, idleSeconds: aggregate.totals.idleSeconds, learningSeconds: aggregate.totals.learningSeconds, switchCount: aggregate.totals.switchCount, longestFocusSeconds: aggregate.totals.longestFocusSeconds, completedTaskCount: aggregate.totals.completedTaskCount, averageMonitoredSeconds: ratio(aggregate.totals.monitoredSeconds, aggregate.recorded.length), averageActiveSeconds: ratio(aggregate.totals.activeSeconds, aggregate.recorded.length), averageIdleSeconds: ratio(aggregate.totals.idleSeconds, aggregate.recorded.length), averageLearningSeconds: ratio(aggregate.totals.learningSeconds, aggregate.recorded.length), averageSwitchCount: ratio(aggregate.totals.switchCount, aggregate.recorded.length), learningRatio: ratio(aggregate.totals.learningSeconds, aggregate.totals.activeSeconds), switchesPerActiveHour: ratio(aggregate.totals.switchCount, aggregate.totals.activeSeconds / 3600), productiveDayCount: aggregate.recorded.length, focusDayCount: aggregate.recorded.filter((fact) => fact.values.longestFocusSeconds >= 1800).length, categoryBreakdown: [{ name: "creation_development", seconds: aggregate.totals.learningSeconds, share: .52 }], appBreakdown: [{ name: "Codex", seconds: aggregate.totals.activeSeconds, share: .64 }] }, comparison: { previousRange: previewRangeFromFacts(previousRange, previousFacts, previousDates), dayCount: facts.length, previousMonitoredSeconds: previous.totals.monitoredSeconds, previousActiveSeconds: previous.totals.activeSeconds, previousIdleSeconds: previous.totals.idleSeconds, previousLearningSeconds: previous.totals.learningSeconds, previousSwitchCount: previous.totals.switchCount, previousLongestFocusSeconds: previous.totals.longestFocusSeconds, previousCompletedTaskCount: previous.totals.completedTaskCount, previousLearningRatio: ratio(previous.totals.learningSeconds, previous.totals.activeSeconds), previousSwitchesPerActiveHour: ratio(previous.totals.switchCount, previous.totals.activeSeconds / 3600), previousClassificationCoverage: previous.totals.classificationCoverage, previousCategoryBreakdown: [], previousAppBreakdown: [], monitoredSecondsDeltaPercent: delta(aggregate.totals.monitoredSeconds, previous.totals.monitoredSeconds), activeSecondsDeltaPercent: delta(aggregate.totals.activeSeconds, previous.totals.activeSeconds), idleSecondsDeltaPercent: delta(aggregate.totals.idleSeconds, previous.totals.idleSeconds), learningSecondsDeltaPercent: delta(aggregate.totals.learningSeconds, previous.totals.learningSeconds), switchCountDeltaPercent: delta(aggregate.totals.switchCount, previous.totals.switchCount), longestFocusSecondsDeltaPercent: delta(aggregate.totals.longestFocusSeconds, previous.totals.longestFocusSeconds), completedTaskCountDeltaPercent: delta(aggregate.totals.completedTaskCount, previous.totals.completedTaskCount), learningRatioDeltaPercent: delta(ratio(aggregate.totals.learningSeconds, aggregate.totals.activeSeconds), ratio(previous.totals.learningSeconds, previous.totals.activeSeconds)), switchesPerActiveHourDeltaPercent: delta(ratio(aggregate.totals.switchCount, aggregate.totals.activeSeconds), ratio(previous.totals.switchCount, previous.totals.activeSeconds)), classificationCoverageDeltaPercent: delta(aggregate.totals.classificationCoverage, previous.totals.classificationCoverage) }, quality: { recordedDayCount: aggregate.recorded.length, missingDayCount: aggregate.missingDayCount, classifiedSeconds: aggregate.recorded.reduce((sum, fact) => sum + Math.round(fact.values.monitoredSeconds * fact.values.classificationCoverage), 0), pendingSeconds: aggregate.missingDayCount * 240, lowConfidenceSeconds: aggregate.recorded.filter((fact) => fact.values.classificationCoverage < .85).length * 900, classificationCoverage: aggregate.totals.classificationCoverage }, workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] }, evidenceHash: `preview-trend-${range.startDate}-${range.endDate}-${selectedDates?.join(",") ?? "continuous"}` };
}

function buildDynamicPreviewWorkbenchPayload(request: TrendWorkbenchRequest): TrendWorkbenchPayload {
  const range = { startDate: request.startDate, endDate: request.endDate };
  const selectedDates = request.selectedDates?.length ? normalizeTrendSelectedDates(request.selectedDates) : undefined;
  const facts = previewDateFacts(range, 0, selectedDates);
  const aggregate = previewAggregate(facts);
  const granularity = request.granularity ?? "day";
  const groups = new Map<string, PreviewFact[]>();
  for (const fact of facts) { const weekday = new Date(`${fact.date}T00:00:00Z`).getUTCDay(); const key = granularity === "day" ? fact.date : granularity === "week" ? addPreviewDays(fact.date, -((weekday + 6) % 7)) : fact.date.slice(0, 7); groups.set(key, [...(groups.get(key) ?? []), fact]); }
  const buckets = [...groups.values()].map((group) => { const startDate = group[0].date; const endDate = group[group.length - 1].date; const id = `${startDate}_${endDate}`; const bucket = previewAggregate(group); return { id, startDate, endDate, values: bucket.totals, recordedDayCount: bucket.recorded.length, missingDayCount: bucket.missingDayCount, evidenceIds: bucket.recorded.map((fact) => `preview-${fact.date}`), activityComposition: previewActivityComposition(bucket.totals.activeSeconds, bucket.totals.idleSeconds), drilldown: { bucketId: id, rawRows: bucket.recorded.map((fact) => ({ rowId: `preview-row-${fact.date}`, bucketId: id, evidenceKind: "activity" as const, evidenceId: `activity-${fact.date}`, date: fact.date, startTime: "09:00", endTime: "12:30", app: "Codex", titleSummary: "预览活动记录", category: "creation_development", videoPurpose: null, meaningful: true, meaningfulReason: "core" as const, taskId: "preview-task", taskTitle: "完善趋势工作台", projectId: "preview-project", projectName: "Orbit", clippedDurationSeconds: fact.values.activeSeconds, confidence: fact.values.classificationCoverage, reviewState: "confirmed" as const, shared: false })), applicationDistribution: [{ key: "codex", label: "Codex", seconds: bucket.totals.activeSeconds }], categoryDistribution: [{ key: "creation_development", label: "创作开发", seconds: bucket.totals.learningSeconds }], completedTasks: [], linkedTaskRollups: [], linkedProjectRollups: [], workflowOwnership: [], dataQuality: { recordedDayCount: bucket.recorded.length, missingDayCount: bucket.missingDayCount, classifiedSeconds: bucket.recorded.reduce((sum, fact) => sum + Math.round(fact.values.monitoredSeconds * fact.values.classificationCoverage), 0), classificationCoverage: bucket.totals.classificationCoverage, lowConfidenceSeconds: bucket.recorded.filter((fact) => fact.values.classificationCoverage < .85).length * 900, pendingSeconds: bucket.missingDayCount * 240 } } }; });
  const currentValue = aggregate.recorded.length ? aggregate.totals[request.metric] : null;
  const envelopeDayCount = inclusiveTrendDayCount(range);
  const equalRange = { startDate: addPreviewDays(range.startDate, -envelopeDayCount), endDate: addPreviewDays(range.endDate, -envelopeDayCount) };
  const equalDates = selectedDates?.map((date) => addPreviewDays(date, -envelopeDayCount));
  const monthRange = { startDate: previousMonthDate(range.startDate), endDate: previousMonthDate(range.endDate) };
  const monthDates = selectedDates?.map(previousMonthDate);
  const baselines = [previewBaselineSeries("current", range, currentValue, request.metric, 0, selectedDates), previewBaselineSeries("previousEqualLength", equalRange, currentValue, request.metric, -1, equalDates), previewBaselineSeries("previousMonthSamePeriod", monthRange, currentValue, request.metric, -2, monthDates), ...(request.customBaseline ? [previewBaselineSeries("custom", request.customBaseline, currentValue, request.metric, 2)] : [])];
  const meanPerBucket = Object.fromEntries(previewMetrics.map((metric) => [metric, buckets.length ? buckets.reduce((sum, bucket) => sum + bucket.values[metric], 0) / buckets.length : 0])) as unknown as TrendMetricValues;
  const dailyMedian = previewDailyStatistic(facts, (values) => {
    if (!values.length) return 0;
    const sorted = [...values].sort((left, right) => left - right);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
  });
  const dailyMax = previewDailyStatistic(facts, (values) => values.length ? Math.max(...values) : 0);
  const averageSampleDayCount = aggregate.recorded.length;
  const dailyAverage = averageSampleDayCount
    ? {
      monitoredSeconds: aggregate.totals.monitoredSeconds / averageSampleDayCount,
      activeSeconds: aggregate.totals.activeSeconds / averageSampleDayCount,
      idleSeconds: aggregate.totals.idleSeconds / averageSampleDayCount,
      learningSeconds: aggregate.totals.learningSeconds / averageSampleDayCount,
    }
    : { monitoredSeconds: null, activeSeconds: null, idleSeconds: null, learningSeconds: null };
  return { range: previewRangeFromFacts(range, facts, selectedDates), granularity, metric: request.metric, metricAvailability: previewMetrics.map((metric) => ({ metric, status: "available" as const, reasonCode: null })), buckets, summary: { totals: aggregate.totals, dailyAverage, averageSampleDayCount, switchesPerActiveHour: aggregate.totals.activeSeconds > 0 ? aggregate.totals.switchCount * 3600 / aggregate.totals.activeSeconds : null, meanPerBucket, dailyMedian, dailyMax, dailySampleStddev: Object.fromEntries(previewMetrics.map((metric) => [metric, 0])) as unknown as TrendMetricValues, dailyCoefficientOfVariation: Object.fromEntries(previewMetrics.map((metric) => [metric, 0])) as unknown as TrendMetricValues, recordedDayCount: aggregate.recorded.length, effectiveActivityDayCount: aggregate.recorded.filter((fact) => fact.values.activeSeconds > 0).length, missingDayCount: aggregate.missingDayCount, classifiedSeconds: aggregate.recorded.reduce((sum, fact) => sum + Math.round(fact.values.monitoredSeconds * fact.values.classificationCoverage), 0), classificationCoverage: aggregate.totals.classificationCoverage, lowConfidenceSeconds: aggregate.recorded.filter((fact) => fact.values.classificationCoverage < .85).length * 900, pendingSeconds: aggregate.missingDayCount * 240, evidenceIds: aggregate.recorded.map((fact) => `preview-${fact.date}`) }, activityComposition: previewActivityComposition(aggregate.totals.activeSeconds, aggregate.totals.idleSeconds), baselines, evidence: [], evidenceHash: `preview-${trendWorkbenchRequestKey(request)}` };
}

export function TrendWorkbench({
  preset,
  range,
  customStart,
  customEnd,
  customMode = "continuous",
  specificDraftDates = [],
  specificAppliedDates = [],
  specificSelectionError = "",
  status,
  payload,
  workbenchStatus = "loading",
  workbenchPayload = null,
  workbenchError = "",
  granularity = "day",
  metric = "activeSeconds",
  customBaselineEnabled = false,
  customBaselineStart = "",
  customBaselineEnd = "",
  analysisStatus = "loading",
  analysis = null,
  analysisError = "",
  activityScope = "all",
  nativeActionsAvailable = true,
  reanalysisFeedback = { evidenceHash: "", status: "idle", message: "" },
  exportFeedback = { evidenceHash: "", status: "idle", message: "" },
  error,
  onPresetChange,
  onCustomStartChange,
  onCustomEndChange,
  onCustomModeChange = () => undefined,
  onSpecificDraftDatesChange = () => undefined,
  onClearSpecificDates = () => undefined,
  onApplySpecificDates = () => undefined,
  onRemoveSpecificDate = () => undefined,
  onShift,
  onRefresh,
  onReanalyze = () => undefined,
  onActivityScopeChange = () => undefined,
  onExport = () => undefined,
  onGranularityChange = () => undefined,
  onMetricChange = () => undefined,
  onCustomBaselineEnabledChange = () => undefined,
  onCustomBaselineStartChange = () => undefined,
  onCustomBaselineEndChange = () => undefined,
  onOpenDate,
  onOpenTask,
  formatDuration = fallbackDuration,
}: TrendWorkbenchProps) {
  const sparseSelection = preset === "custom" && customMode === "specific";
  const dataStatus = sparseSelection ? workbenchStatus : status;
  const dataError = sparseSelection ? workbenchError : error;
  const empty = dataStatus === "ready" && (sparseSelection
    ? (!workbenchPayload || workbenchPayload.buckets.length === 0 || workbenchPayload.summary.totals.monitoredSeconds === 0)
    : (!payload || payload.days.length === 0 || payload.summary.monitoredSeconds === 0));
  const exportEvidenceHash = sparseSelection ? workbenchPayload?.evidenceHash : payload?.evidenceHash;
  const exportReady = sparseSelection
    ? workbenchStatus === "ready" && Boolean(workbenchPayload)
    : status === "ready" && Boolean(payload);
  const selectedAnalysisEvidenceHash = workbenchPayload
    ? trendAnalysisEvidenceHash(workbenchPayload)
    : "";
  const localAnalysis = workbenchPayload
    ? buildLocalTrendAnalysis(buildTrendStatisticsDto(workbenchPayload, activityScope))
    : null;
  const trendChartPoints = useMemo(() => workbenchPayload
    ? buildTrendChartPoints(
      workbenchPayload.buckets,
      workbenchPayload.range.startDate,
      workbenchPayload.range.endDate,
      workbenchPayload.granularity,
    )
    : [], [workbenchPayload]);
  const [selectedBucketId, setSelectedBucketId] = useState<string | null>(null);
  const [selectedActivityKey, setSelectedActivityKey] = useState<ActivityDisplayKey | null>(null);
  const timelineFocusRef = useRef<HTMLDivElement>(null);
  const drilldownDetailsRef = useRef<HTMLDetailsElement>(null);
  const selectedBucket = workbenchPayload?.buckets.find((bucket) => bucket.id === selectedBucketId) ?? null;
  const selectedBucketIndex = workbenchPayload?.buckets.findIndex((bucket) => bucket.id === selectedBucketId) ?? -1;
  const previousBucket = selectedBucketIndex > 0 ? workbenchPayload?.buckets[selectedBucketIndex - 1] ?? null : null;
  useEffect(() => {
    const buckets = workbenchPayload?.buckets ?? [];
    const latestNonEmpty = [...buckets].reverse().find((bucket) => (
      bucket.recordedDayCount > 0
      || bucket.values.monitoredSeconds > 0
      || bucket.values.activeSeconds > 0
    ));
    setSelectedBucketId(latestNonEmpty?.id ?? buckets.at(-1)?.id ?? null);
  }, [workbenchPayload?.evidenceHash]);
  useEffect(() => {
    if (selectedActivityKey && workbenchPayload?.activityComposition
      && !compositionForScope(workbenchPayload.activityComposition, activityScope).items.some((item) => item.key === selectedActivityKey)) {
      setSelectedActivityKey(null);
    }
  }, [activityScope, selectedActivityKey, workbenchPayload?.activityComposition]);
  const selectCompositionCategory = (key: ActivityDisplayKey) => {
    setSelectedActivityKey(key);
    const matchingBucket = [...(workbenchPayload?.buckets ?? [])].reverse().find((bucket) => (
      bucket.activityComposition
        ? compositionForScope(bucket.activityComposition, activityScope).items.some((item) => item.key === key)
        : false
    ));
    if (matchingBucket) setSelectedBucketId(matchingBucket.id);
    queueMicrotask(() => {
      if (!drilldownDetailsRef.current) return;
      drilldownDetailsRef.current.open = true;
      drilldownDetailsRef.current.scrollIntoView?.({ behavior: "smooth", block: "nearest" });
    });
  };

  return (
    <section className="secondary-view trends-workbench">
      <TrendRangeToolbar
        preset={preset} range={range} customStart={customStart} customEnd={customEnd}
        customMode={customMode} specificDraftDates={specificDraftDates}
        specificAppliedDates={specificAppliedDates} specificSelectionError={specificSelectionError}
        granularity={granularity} metric={metric}
        metricAvailability={workbenchPayload?.metricAvailability ?? []}
        customBaselineEnabled={customBaselineEnabled}
        customBaselineStart={customBaselineStart} customBaselineEnd={customBaselineEnd}
        loading={dataStatus === "loading" || workbenchStatus === "loading"}
        exportDisabled={!range || !exportReady || !nativeActionsAvailable || exportFeedback.status === "pending"}
        onPresetChange={onPresetChange} onCustomStartChange={onCustomStartChange} onCustomEndChange={onCustomEndChange}
        onCustomModeChange={onCustomModeChange} onSpecificDraftDatesChange={onSpecificDraftDatesChange}
        onClearSpecificDates={onClearSpecificDates} onApplySpecificDates={onApplySpecificDates}
        onRemoveSpecificDate={onRemoveSpecificDate}
        onGranularityChange={onGranularityChange} onMetricChange={onMetricChange}
        onCustomBaselineEnabledChange={onCustomBaselineEnabledChange}
        onCustomBaselineStartChange={onCustomBaselineStartChange} onCustomBaselineEndChange={onCustomBaselineEndChange}
        onShift={onShift} onRefresh={onRefresh} onExport={onExport}
      />
      {!nativeActionsAvailable && <p className="trend-action-feedback" role="status">仅桌面版支持重新分析与 Markdown 导出</p>}
      {exportEvidenceHash && exportFeedback.evidenceHash === exportEvidenceHash && exportFeedback.message && (
        <p className={`trend-action-feedback ${exportFeedback.status}`} role={exportFeedback.status === "error" ? "alert" : "status"}>{exportFeedback.message}</p>
      )}

      {dataStatus === "loading" && <div className="trend-state" role="status">正在读取趋势数据...</div>}
      {dataStatus === "error" && <div className="trend-state error" role="alert"><strong>趋势数据读取失败</strong><span>{dataError}</span></div>}
      {dataStatus === "ready" && empty && <div className="trend-state" role="status">当前范围暂无活动记录</div>}

      {dataStatus === "ready" && (sparseSelection || payload) && !empty && <>
        {workbenchStatus === "loading" && <div className="trend-inline-state" role="status">正在读取数据工作台...</div>}
        {workbenchStatus === "error" && <div className="trend-inline-state error" role="alert">{workbenchError}</div>}
        {workbenchStatus === "ready" && workbenchPayload?.buckets.length === 0 && <div className="trend-inline-state" role="status">当前范围没有可用的时间分桶</div>}
        {workbenchStatus === "ready" && workbenchPayload && workbenchPayload.buckets.length > 0 && <>
          <TrendStatisticsStrip summary={workbenchPayload.summary} selectedDayCount={workbenchPayload.range.selectedDateCount ?? workbenchPayload.range.dayCount} metric={metric} formatDuration={formatDuration} onMetricChange={(nextMetric) => {
            const focusTarget = timelineFocusRef.current;
            if (focusTarget) {
              focusTarget.setAttribute("aria-label", `已聚焦：${focusMetricLabels[nextMetric]}时间分桶图表`);
              focusTarget.focus();
            }
            if (nextMetric !== metric) onMetricChange(nextMetric);
          }} />
          <div className="trend-dashboard-primary">
            <TrendCompositionChart
              payload={payload}
              workbenchPayload={workbenchPayload}
              formatDuration={formatDuration}
              activityScope={activityScope}
              onActivityScopeChange={onActivityScopeChange}
              onSelectCategory={selectCompositionCategory}
            />
            <article ref={timelineFocusRef} className="trend-dashboard-card trend-dashboard-daily" tabIndex={-1} role="region" aria-label={`已聚焦：${focusMetricLabels[metric]}每日时间图表`} data-trend-chart-focus>
              <header><h2>每日时间 <small>（{workbenchPayload.range.dayCount === 30 ? "过去 30 天" : `${workbenchPayload.range.dayCount} 天`}）</small></h2></header>
              <TrendStackedTimeChart
                points={trendChartPoints}
                activityScope={activityScope}
                selectedBucketId={selectedBucketId}
                formatDuration={formatDuration}
                onSelectBucket={setSelectedBucketId}
              />
            </article>
          </div>
        </>}
        {localAnalysis && <>
          <div className="trend-dashboard-lower">
            <article className="trend-dashboard-card trend-dashboard-weekday">
              <header><h2>按星期分布 <small>（活跃时间）</small></h2></header>
              <TrendWeekdayChart points={trendChartPoints} formatDuration={formatDuration} />
            </article>
            <div className="trend-dashboard-analysis">
              <p className="trend-analysis-scope">分析口径：{activityScope === "all" ? "全部活动" : activityScope === "active" ? "活跃" : "学习"}</p>
              <div className="trend-evaluation-actions">
                <button
                  type="button"
                  className="icon-button"
                  aria-label="重新分析"
                  title="重新分析"
                  disabled={!nativeActionsAvailable || reanalysisFeedback.status === "pending"}
                  onClick={onReanalyze}
                ><RefreshCcw size={17} /></button>
              </div>
              {reanalysisFeedback.message && <p
                className={`trend-action-feedback ${reanalysisFeedback.status}`}
                role={reanalysisFeedback.status === "error" ? "alert" : "status"}
              >{reanalysisFeedback.message}</p>}
              <TrendResearchAnalysisPanel
                localAnalysis={localAnalysis}
                analysis={analysisStatus === "ready" ? analysis : null}
                analysisStatus={analysisStatus}
                analysisError={analysisError}
                evidence={activityScope !== "all"
                  ? (workbenchPayload?.analysisEvidence ?? [])
                  : (workbenchPayload?.evidence ?? [])}
                evidenceHash={selectedAnalysisEvidenceHash}
                activityScope={activityScope}
                formatDuration={formatDuration}
                onEvidenceActivate={(evidenceId) => {
                  const matchingBucket = workbenchPayload?.buckets.find((bucket) => (
                    bucket.evidenceIds.includes(evidenceId)
                    || bucket.drilldown.rawRows.some((row) => row.evidenceId === evidenceId)
                  ));
                  if (matchingBucket) setSelectedBucketId(matchingBucket.id);
                }}
              />
            </div>
          </div>
        </>}
        {workbenchStatus === "ready" && workbenchPayload && workbenchPayload.buckets.length > 0 && (
          <details ref={drilldownDetailsRef} className="trend-dashboard-details">
            <summary>查看基准比较与明细下钻</summary>
            <div className="trend-primary-timeline" tabIndex={-1} role="region" aria-label={`已聚焦：${focusMetricLabels[metric]}时间分桶图表`}>
              <TrendTimelineChart buckets={workbenchPayload.buckets} metric={metric} selectedBucketId={selectedBucketId} formatDuration={formatDuration} onSelectBucket={setSelectedBucketId} />
            </div>
            <TrendComparisonChart baselines={workbenchPayload.baselines} metric={metric} formatDuration={formatDuration} />
            <TrendDrilldown
              bucket={selectedBucket}
              previousBucket={previousBucket}
              rangeMean={workbenchPayload.summary.meanPerBucket[metric]}
              metric={metric}
              activityScope={activityScope}
              activityFilter={selectedActivityKey}
              formatDuration={formatDuration}
              onClearActivityFilter={() => setSelectedActivityKey(null)}
              onOpenDate={onOpenDate}
              onOpenTask={onOpenTask}
            />
          </details>
        )}
      </>}
    </section>
  );
}

export function TrendsView({
  anchorDate,
  formatDuration,
  autoAnalysisEnabled = false,
  loadRange = loadTrendRange,
  loadWorkbench = loadTrendWorkbench,
  loadAnalysis = loadTrendResearchAnalysis,
  queueAnalysis = queueTrendResearchAnalysis,
  exportMarkdown = exportTrendMarkdown,
  exportDocument = exportTrendReport,
  desktopRuntime = isDesktopRuntime,
  onOpenDate,
  onOpenTask,
}: TrendsViewProps) {
  const [preset, setPreset] = useState<TrendPreset>("week");
  const [activityScope, setActivityScope] = useState<ActivityScope>(() => {
    try {
      return parsePersistedActivityScope(globalThis.localStorage?.getItem(ACTIVITY_SCOPE_STORAGE_KEYS.trends));
    } catch {
      return "all";
    }
  });
  const [rangeAnchorState, setRangeAnchorState] = useState({ propAnchor: anchorDate, rangeAnchor: anchorDate });
  const anchorPropChanged = rangeAnchorState.propAnchor !== anchorDate;
  if (anchorPropChanged) {
    setRangeAnchorState({ propAnchor: anchorDate, rangeAnchor: anchorDate });
  }
  const rangeAnchor = anchorPropChanged ? anchorDate : rangeAnchorState.rangeAnchor;
  const initialRange = (() => {
    try {
      return resolveTrendRange("week", anchorDate, "", "");
    } catch {
      return { startDate: "", endDate: "" };
    }
  })();
  const [customStart, setCustomStart] = useState(initialRange.startDate);
  const [customEnd, setCustomEnd] = useState(initialRange.endDate);
  const [customMode, setCustomMode] = useState<TrendCustomMode>("continuous");
  const [specificDraftDates, setSpecificDraftDates] = useState<string[]>(() => datesInTrendRange(initialRange));
  const [specificAppliedDates, setSpecificAppliedDates] = useState<string[]>(() => datesInTrendRange(initialRange));
  const [specificSelectionError, setSpecificSelectionError] = useState("");
  const [loadState, setLoadState] = useState<TrendLoadState>(() => loadingTrendState(""));
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [granularityOverride, setGranularityOverride] = useState<TrendGranularity | null>(null);
  const [metric, setMetric] = useState<TrendMetric>("activeSeconds");
  const [customBaselineEnabled, setCustomBaselineEnabled] = useState(false);
  const [customBaselineStart, setCustomBaselineStart] = useState("");
  const [customBaselineEnd, setCustomBaselineEnd] = useState("");
  const [workbenchState, setWorkbenchState] = useState<TrendWorkbenchLoadState>({ requestKey: "", status: "loading", payload: null, error: "" });
  const [analysisState, setAnalysisState] = useState<TrendAnalysisLoadState>({
    status: "loading",
    evidenceHash: "",
    analysis: null,
    error: "",
  });
  const [reanalysisFeedback, setReanalysisFeedback] = useState<TrendActionState>({ evidenceHash: "", status: "idle", message: "" });
  const [exportFeedback, setExportFeedback] = useState<TrendActionState>({ evidenceHash: "", status: "idle", message: "" });
  const coordinatorRef = useRef<ReturnType<typeof createTrendRangeRequestCoordinator> | null>(null);
  const workbenchCoordinatorRef = useRef<ReturnType<typeof createTrendWorkbenchRequestCoordinator> | null>(null);
  const analysisCoordinatorRef = useRef<ReturnType<typeof createTrendAnalysisRequestCoordinator> | null>(null);
  const reanalysisCoordinatorRef = useRef<ReturnType<typeof createTrendActionRequestCoordinator> | null>(null);
  const exportCoordinatorRef = useRef<ReturnType<typeof createTrendActionRequestCoordinator> | null>(null);
  const autoQueuedEvidenceRef = useRef(new Set<string>());
  const customInitializedRef = useRef(false);
  useEffect(() => {
    try {
      globalThis.localStorage?.setItem(ACTIVITY_SCOPE_STORAGE_KEYS.trends, activityScope);
    } catch {
      // Storage can be unavailable in web previews and hardened desktop environments.
    }
  }, [activityScope]);
  if (!coordinatorRef.current) {
    coordinatorRef.current = createTrendRangeRequestCoordinator(loadRange, setLoadState);
  }
  if (!workbenchCoordinatorRef.current) {
    workbenchCoordinatorRef.current = createTrendWorkbenchRequestCoordinator(loadWorkbench, setWorkbenchState);
  }
  if (!analysisCoordinatorRef.current) {
    analysisCoordinatorRef.current = createTrendAnalysisRequestCoordinator(loadAnalysis, setAnalysisState);
  }
  if (!reanalysisCoordinatorRef.current) {
    reanalysisCoordinatorRef.current = createTrendActionRequestCoordinator(setReanalysisFeedback);
  }
  if (!exportCoordinatorRef.current) {
    exportCoordinatorRef.current = createTrendActionRequestCoordinator(setExportFeedback);
  }

  const resolution = useMemo(() => {
    try {
      if (preset === "custom" && customMode === "specific") {
        const selected = resolveTrendSelectedDates(specificAppliedDates);
        return { range: selected.range, error: "" };
      }
      return { range: resolveTrendRange(preset, rangeAnchor, customStart, customEnd), error: "" };
    } catch (rangeError) {
      return { range: null, error: rangeError instanceof Error ? rangeError.message : String(rangeError) };
    }
  }, [customEnd, customMode, customStart, preset, rangeAnchor, specificAppliedDates]);
  useEffect(() => {
    if (!desktopRuntime()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listenAnalysisChanged((event) => {
      const currentRange = resolution.range;
      const matchingRange = !currentRange
        || ((!event.rangeStart || event.rangeStart === currentRange.startDate)
          && (!event.rangeEnd || event.rangeEnd === currentRange.endDate));
      if (!active || event.page !== "trends" || event.scope !== activityScope || !matchingRange) return;
      setRefreshVersion((value) => value + 1);
    }).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    }).catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [activityScope, desktopRuntime, resolution.range?.endDate, resolution.range?.startDate]);

  const selectedRangeKey = resolution.range
    ? trendRangeKey(resolution.range)
    : `invalid:${resolution.error}`;
  const visibleLoadState = visibleTrendLoadState(loadState, selectedRangeKey);
  const usingSpecificDates = preset === "custom" && customMode === "specific";
  const granularity = resolution.range
    ? (granularityOverride ?? (usingSpecificDates
      ? defaultTrendGranularityForSelectedDates(specificAppliedDates)
      : defaultTrendGranularity(resolution.range)))
    : (granularityOverride ?? "day");
  const workbenchRequest = resolution.range ? {
    startDate: resolution.range.startDate,
    endDate: resolution.range.endDate,
    ...(usingSpecificDates ? { selectedDates: specificAppliedDates } : {}),
    timezoneOffsetMinutes: new Date().getTimezoneOffset(),
    granularity,
    metric,
    activityScope,
    customBaseline: customBaselineEnabled && customBaselineStart && customBaselineEnd
      ? { startDate: customBaselineStart, endDate: customBaselineEnd }
      : null,
  } satisfies TrendWorkbenchRequest : null;
  const selectedWorkbenchKey = workbenchRequest ? trendWorkbenchRequestKey(workbenchRequest) : selectedRangeKey;
  const visibleWorkbenchState = visibleTrendWorkbenchLoadState(workbenchState, selectedWorkbenchKey);
  const visibleAnalysisEvidenceHash = visibleWorkbenchState.payload
    ? trendAnalysisEvidenceHash(visibleWorkbenchState.payload)
    : "";

  useEffect(() => {
    if (!workbenchRequest) {
      workbenchCoordinatorRef.current?.invalidate({ requestKey: selectedRangeKey, status: "error", payload: null, error: resolution.error });
      return;
    }
    if (!desktopRuntime()) {
      workbenchCoordinatorRef.current?.invalidate({ requestKey: selectedWorkbenchKey, status: "ready", payload: buildPreviewWorkbenchPayload(workbenchRequest), error: "" });
      return;
    }
    let cancelled = false;
    queueMicrotask(() => { if (!cancelled) void workbenchCoordinatorRef.current?.load(workbenchRequest); });
    return () => { cancelled = true; workbenchCoordinatorRef.current?.cancel(); };
  }, [desktopRuntime, refreshVersion, resolution.error, selectedRangeKey, selectedWorkbenchKey]);

  useEffect(() => {
    if (usingSpecificDates) {
      coordinatorRef.current?.cancel();
      return;
    }
    if (!resolution.range) {
      coordinatorRef.current?.invalidate({
        rangeKey: selectedRangeKey,
        status: "error",
        payload: null,
        error: resolution.error,
      });
      return;
    }
    if (!desktopRuntime()) {
      coordinatorRef.current?.invalidate({
        rangeKey: selectedRangeKey,
        status: "ready",
        payload: buildPreviewTrendPayload(resolution.range),
        error: "",
      });
      return;
    }
    const range = resolution.range;
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      void coordinatorRef.current?.load(range);
    });
    return () => {
      cancelled = true;
      coordinatorRef.current?.cancel();
    };
  }, [desktopRuntime, refreshVersion, resolution.error, resolution.range?.endDate, resolution.range?.startDate, selectedRangeKey, usingSpecificDates]);

  useEffect(() => {
    const currentPayload = visibleWorkbenchState.status === "ready" ? visibleWorkbenchState.payload : null;
    if (!currentPayload || !workbenchRequest) {
      analysisCoordinatorRef.current?.cancel();
      return;
    }
    if (!desktopRuntime()) {
      analysisCoordinatorRef.current?.cancel();
      const evidenceHash = trendAnalysisEvidenceHash(currentPayload);
      setAnalysisState({
        status: "ready",
        evidenceHash,
        analysis: {
          activityScope,
          status: "limitations_only",
          findings: [],
          limitations: ["桌面 AI 执行不可用，仅显示本地事实统计"],
          source: "local",
          model: "policy-v1",
          evidenceHash,
        },
        error: "",
      });
      return;
    }
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      void analysisCoordinatorRef.current?.load(currentPayload, workbenchRequest);
    });
    return () => {
      cancelled = true;
      analysisCoordinatorRef.current?.cancel();
    };
  }, [desktopRuntime, selectedWorkbenchKey, visibleAnalysisEvidenceHash, visibleWorkbenchState.status]);

  useEffect(() => {
    const currentPayload = visibleWorkbenchState.status === "ready" ? visibleWorkbenchState.payload : null;
    const analysis = analysisState.status === "ready" ? analysisState.analysis : null;
    if (
      !autoAnalysisEnabled
      || activityScope !== "meaningful"
      || !desktopRuntime()
      || !currentPayload
      || !workbenchRequest
      || !analysis
      || analysis.activityScope !== "meaningful"
      || analysis.evidenceHash !== visibleAnalysisEvidenceHash
      || analysis.source !== "local"
      || autoQueuedEvidenceRef.current.has(visibleAnalysisEvidenceHash)
    ) return;
    autoQueuedEvidenceRef.current.add(visibleAnalysisEvidenceHash);
    void queueAnalysis(workbenchRequest, false).catch(() => {
      autoQueuedEvidenceRef.current.delete(visibleAnalysisEvidenceHash);
    });
  }, [
    activityScope,
    analysisState.analysis,
    analysisState.status,
    autoAnalysisEnabled,
    desktopRuntime,
    queueAnalysis,
    selectedWorkbenchKey,
    visibleAnalysisEvidenceHash,
    visibleWorkbenchState.status,
  ]);

  useEffect(() => {
    reanalysisCoordinatorRef.current?.activate(visibleAnalysisEvidenceHash);
    exportCoordinatorRef.current?.activate(usingSpecificDates
      ? (visibleWorkbenchState.payload?.evidenceHash ?? "")
      : (visibleLoadState.payload?.evidenceHash ?? ""));
    return () => {
      reanalysisCoordinatorRef.current?.cancel();
      exportCoordinatorRef.current?.cancel();
    };
  }, [usingSpecificDates, visibleAnalysisEvidenceHash, visibleLoadState.payload?.evidenceHash, visibleWorkbenchState.payload?.evidenceHash]);

  const changePreset = (nextPreset: TrendPreset) => {
    if (nextPreset === "custom" && resolution.range && !customInitializedRef.current) {
      setCustomStart(resolution.range.startDate);
      setCustomEnd(resolution.range.endDate);
      const initialDates = datesInTrendRange(resolution.range);
      setSpecificDraftDates(initialDates);
      setSpecificAppliedDates(initialDates);
      customInitializedRef.current = true;
    }
    setPreset(nextPreset);
  };

  const changeCustomMode = (nextMode: TrendCustomMode) => {
    setSpecificSelectionError("");
    setCustomMode(nextMode);
  };

  const applySpecificDates = () => {
    try {
      const selected = resolveTrendSelectedDates(specificDraftDates);
      setSpecificDraftDates(selected.selectedDates);
      setSpecificAppliedDates(selected.selectedDates);
      setSpecificSelectionError("");
    } catch (selectionError) {
      setSpecificSelectionError(selectionError instanceof Error ? selectionError.message : String(selectionError));
    }
  };

  const removeSpecificDate = (date: string) => {
    const nextAppliedDates = specificAppliedDates.filter((item) => item !== date);
    setSpecificAppliedDates(nextAppliedDates);
    setSpecificDraftDates((currentDraft) => currentDraft.filter((item) => item !== date));
    setSpecificSelectionError(nextAppliedDates.length ? "" : "请至少选择 1 天");
  };

  const changeCustomBaseline = (enabled: boolean) => {
    if (enabled && resolution.range && (!customBaselineStart || !customBaselineEnd)) {
      const previous = shiftTrendRange(resolution.range, -1);
      setCustomBaselineStart(previous.startDate);
      setCustomBaselineEnd(previous.endDate);
    }
    setCustomBaselineEnabled(enabled);
  };

  const shift = (direction: -1 | 1) => {
    if (!resolution.range) return;
    if (usingSpecificDates) {
      const shiftedDates = shiftTrendSelectedDates(specificAppliedDates, direction);
      setSpecificDraftDates(shiftedDates);
      setSpecificAppliedDates(shiftedDates);
      setSpecificSelectionError("");
      return;
    }
    const shifted = shiftTrendRange(resolution.range, direction);
    if (preset === "custom") {
      setCustomStart(shifted.startDate);
      setCustomEnd(shifted.endDate);
    } else {
      setRangeAnchorState({ propAnchor: anchorDate, rangeAnchor: shifted.endDate });
    }
  };

  const reanalyze = () => {
    const current = visibleWorkbenchState.payload;
    if (!current || !workbenchRequest || !desktopRuntime()) return;
    void reanalysisCoordinatorRef.current?.run(
      trendAnalysisEvidenceHash(current),
      "正在加入分析队列...",
      () => queueAnalysis(workbenchRequest, true),
      (jobId) => jobId ? "已加入分析队列" : "当前离线或未配置可用 AI，继续使用本地统计",
      "重新分析失败",
    );
  };

  const exportCurrentRange = (format: ReportFormat) => {
    const evidenceHash = usingSpecificDates
      ? visibleWorkbenchState.payload?.evidenceHash
      : visibleLoadState.payload?.evidenceHash;
    if (!evidenceHash || !workbenchRequest || !desktopRuntime()) return;
    void exportCoordinatorRef.current?.run(
      evidenceHash,
      `正在导出 ${format === "docx" ? "Word" : "Markdown"}...`,
      () => format === "docx" ? exportDocument(workbenchRequest, format) : exportMarkdown(workbenchRequest),
      (path) => path ? `已导出：${path}` : "已取消导出",
      "导出失败",
    );
  };

  return <TrendWorkbench
    preset={preset}
    range={resolution.range}
    customStart={customStart}
    customEnd={customEnd}
    customMode={customMode}
    specificDraftDates={specificDraftDates}
    specificAppliedDates={specificAppliedDates}
    specificSelectionError={specificSelectionError}
    status={visibleLoadState.status}
    payload={visibleLoadState.payload}
    workbenchStatus={visibleWorkbenchState.status}
    workbenchPayload={visibleWorkbenchState.payload}
    workbenchError={visibleWorkbenchState.error}
    granularity={granularity}
    metric={metric}
    customBaselineEnabled={customBaselineEnabled}
    customBaselineStart={customBaselineStart}
    customBaselineEnd={customBaselineEnd}
    analysisStatus={analysisState.evidenceHash === visibleAnalysisEvidenceHash ? analysisState.status : "loading"}
    analysis={analysisState.evidenceHash === visibleAnalysisEvidenceHash ? analysisState.analysis : null}
    analysisError={analysisState.evidenceHash === visibleAnalysisEvidenceHash ? analysisState.error : ""}
    activityScope={activityScope}
    nativeActionsAvailable={desktopRuntime()}
    reanalysisFeedback={reanalysisFeedback.evidenceHash === visibleAnalysisEvidenceHash
      ? reanalysisFeedback
      : { evidenceHash: visibleAnalysisEvidenceHash, status: "idle", message: "" }}
    exportFeedback={exportFeedback}
    error={visibleLoadState.error || resolution.error}
    onPresetChange={changePreset}
    onCustomStartChange={setCustomStart}
    onCustomEndChange={setCustomEnd}
    onCustomModeChange={changeCustomMode}
    onSpecificDraftDatesChange={(dates) => {
      setSpecificDraftDates(dates);
      setSpecificSelectionError("");
    }}
    onClearSpecificDates={() => {
      setSpecificDraftDates([]);
      setSpecificSelectionError("");
    }}
    onApplySpecificDates={applySpecificDates}
    onRemoveSpecificDate={removeSpecificDate}
    onShift={shift}
    onRefresh={() => setRefreshVersion((value) => value + 1)}
    onReanalyze={reanalyze}
    onActivityScopeChange={setActivityScope}
    onExport={exportCurrentRange}
    onGranularityChange={setGranularityOverride}
    onMetricChange={setMetric}
    onCustomBaselineEnabledChange={changeCustomBaseline}
    onCustomBaselineStartChange={setCustomBaselineStart}
    onCustomBaselineEndChange={setCustomBaselineEnd}
    onOpenDate={onOpenDate}
    onOpenTask={onOpenTask}
    formatDuration={formatDuration}
  />;
}
