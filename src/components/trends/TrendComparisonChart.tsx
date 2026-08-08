import { useEffect, useMemo, useRef } from "react";
import type { TrendBaselineKind, TrendBaselineSeries, TrendMetric } from "../../lib/desktop";
import { chartValue, chartYAxis } from "./metricChartUnits";

const baselineMeta: Record<TrendBaselineKind, { label: string; color: string; pattern: string }> = {
  current: { label: "当前区间", color: "#3164c8", pattern: "实色" },
  previousEqualLength: { label: "上一等长区间", color: "#17a2a4", pattern: "斜线" },
  previousMonthSamePeriod: { label: "上月同期", color: "#d08a27", pattern: "点状" },
  custom: { label: "自定义基准", color: "#7b61a8", pattern: "网格" },
};

const metricLabels: Record<TrendMetric, string> = {
  monitoredSeconds: "监控时长", activeSeconds: "活跃时长", learningSeconds: "学习时长", idleSeconds: "不活跃时长",
  switchCount: "切换次数", longestFocusSeconds: "最长专注块", classificationCoverage: "分类覆盖率",
  completedTaskCount: "任务完成数", linkedTaskSeconds: "已关联任务时长",
};

function displayValue(value: number, metric: TrendMetric, formatDuration: (seconds: number) => string) {
  if (metric === "classificationCoverage") return `${Math.round(value * 100)}%`;
  if (metric === "switchCount" || metric === "completedTaskCount") return value.toFixed(value % 1 ? 1 : 0);
  return formatDuration(value);
}

function deltaText(series: TrendBaselineSeries, metric: TrendMetric, formatDuration: (seconds: number) => string) {
  if (!series.isValid || series.value === null || series.absoluteDelta === null) return "无有效采样";
  const absolute = `${series.absoluteDelta > 0 ? "+" : series.absoluteDelta < 0 ? "-" : ""}${displayValue(Math.abs(series.absoluteDelta), metric, formatDuration)}`;
  if (series.kind === "current") return "比较基准";
  if (series.percentDelta === null) return `${absolute} · 基准为 0，百分比不可用`;
  return `${absolute} · ${series.percentDelta > 0 ? "+" : ""}${series.percentDelta.toFixed(1)}%`;
}

function tooltipText(baselines: TrendBaselineSeries[], metric: TrendMetric, formatDuration: (seconds: number) => string) {
  return baselines.map((series) => {
    const value = !series.isValid || series.value === null ? "无有效采样" : displayValue(series.value, metric, formatDuration);
    return `<b>${baselineMeta[series.kind].label}</b><br/>${series.range.startDate} 至 ${series.range.endDate}<br/>${value}<br/>${deltaText(series, metric, formatDuration)}`;
  }).join("<br/><br/>");
}

export function TrendComparisonChart({ baselines, metric, formatDuration }: { baselines: TrendBaselineSeries[]; metric: TrendMetric; formatDuration: (seconds: number) => string }) {
  const chartRef = useRef<HTMLDivElement | null>(null);
  const validValues = useMemo(() => baselines.map((item) => item.value), [baselines]);
  useEffect(() => {
    const element = chartRef.current;
    if (!element || typeof element.ownerDocument.defaultView?.getComputedStyle !== "function") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void import("echarts").then((echarts) => {
      if (disposed) return;
      const chart = echarts.init(element);
      chart.setOption({
      animationDuration: 220,
      grid: { left: 48, right: 18, top: 42, bottom: 48 },
      legend: { top: 4, data: baselines.map((item) => baselineMeta[item.kind].label) },
      tooltip: { trigger: "axis", formatter: () => tooltipText(baselines, metric, formatDuration) },
      xAxis: { type: "category", data: [metricLabels[metric]], axisLabel: { interval: 0, overflow: "truncate", width: 110 } },
      yAxis: chartYAxis(metric),
      series: baselines.map((item) => ({
        name: baselineMeta[item.kind].label,
        type: "bar",
        barMaxWidth: 58,
        itemStyle: { color: baselineMeta[item.kind].color, decal: item.kind === "current" ? undefined : { symbol: item.kind === "previousMonthSamePeriod" ? "circle" : "rect", dashArrayX: item.kind === "custom" ? [2, 2] : [1, 0], dashArrayY: [3, 3] } },
        data: [item.isValid && item.value !== null ? chartValue(item.value, metric) : "-"],
      })),
      });
      const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => chart.resize());
      resize?.observe(element);
      cleanup = () => { resize?.disconnect(); chart.dispose(); };
    });
    return () => { disposed = true; cleanup?.(); };
  }, [baselines, formatDuration, metric]);
  return <section className="trend-comparison-chart" aria-labelledby="trend-comparison-chart-heading">
    <header className="trend-section-heading"><div><span>MULTI-BASELINE</span><h2 id="trend-comparison-chart-heading">基准比较</h2></div></header>
    <div ref={chartRef} className="trend-echart comparison" role="img" aria-label="当前区间与多个基准的柱状比较" />
    <div className="trend-baseline-grid" role="list" aria-label="基准比较替代表">
      {baselines.map((series, index) => <div className="trend-baseline-item" role="listitem" key={series.kind} data-baseline-kind={series.kind} title={`${series.range.startDate} 至 ${series.range.endDate}`}>
        <span className="trend-baseline-swatch" style={{ backgroundColor: baselineMeta[series.kind].color }} aria-hidden="true" />
        <span><b>{baselineMeta[series.kind].label}</b><small>{baselineMeta[series.kind].pattern} · {series.recordedDayCount} 有效 / {series.missingDayCount} 缺失</small></span>
        <strong>{!series.isValid || validValues[index] === null ? "无有效采样" : displayValue(validValues[index]!, metric, formatDuration)}</strong>
        <em>{deltaText(series, metric, formatDuration)}</em>
      </div>)}
    </div>
  </section>;
}
