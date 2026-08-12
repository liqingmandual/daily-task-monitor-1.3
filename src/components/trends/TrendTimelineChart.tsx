import { useEffect, useMemo, useRef } from "react";
import type { TrendBucket, TrendMetric } from "../../lib/desktop";
import { chartValue, chartYAxis } from "./metricChartUnits";

const metricLabels: Record<TrendMetric, string> = {
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

function valueFor(bucket: TrendBucket, metric: TrendMetric) { return bucket.values[metric]; }
function displayValue(value: number, metric: TrendMetric, formatDuration: (seconds: number) => string) {
  if (metric === "classificationCoverage") return `${Math.round(value * 100)}%`;
  if (metric === "switchCount" || metric === "completedTaskCount") return value.toFixed(value % 1 ? 1 : 0);
  return formatDuration(value);
}

interface Props {
  buckets: TrendBucket[];
  metric: TrendMetric;
  selectedBucketId: string | null;
  formatDuration: (seconds: number) => string;
  onSelectBucket: (bucketId: string) => void;
}

export function TrendTimelineChart({ buckets, metric, selectedBucketId, formatDuration, onSelectBucket }: Props) {
  const chartRef = useRef<HTMLDivElement | null>(null);
  const selectedIndex = useMemo(() => buckets.findIndex((bucket) => bucket.id === selectedBucketId), [buckets, selectedBucketId]);
  useEffect(() => {
    const element = chartRef.current;
    if (!element || typeof element.ownerDocument.defaultView?.getComputedStyle !== "function") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void import("echarts").then((echarts) => {
      if (disposed) return;
      const themeStyles = element.ownerDocument.defaultView!.getComputedStyle(element.closest(".app-frame") ?? element);
      const themeValue = (name: string, fallback: string) => (
        typeof themeStyles.getPropertyValue === "function"
          ? themeStyles.getPropertyValue(name).trim() || fallback
          : fallback
      );
      const primary = themeValue("--primary", "#365f52");
      const secondary = themeValue("--other-active", primary);
      const text = themeValue("--text", "#24251f");
      const muted = themeValue("--muted", "#77786e");
      const border = themeValue("--border", "rgba(75, 76, 64, .14)");
      const surface = themeValue("--surface", "#faf9f5");
      const yAxis = chartYAxis(metric);
      const chart = echarts.init(element);
      chart.setOption({
      animationDuration: 220,
      textStyle: { color: text },
      grid: { left: 54, right: 18, top: 24, bottom: 52 },
      tooltip: { trigger: "axis", backgroundColor: surface, borderColor: border, textStyle: { color: text }, formatter: (params: Array<{ dataIndex: number }>) => { const bucket = buckets[params[0].dataIndex]; return `<b>${bucket.startDate}${bucket.startDate === bucket.endDate ? "" : ` 至 ${bucket.endDate}`}</b><br/>${metricLabels[metric]}：${displayValue(valueFor(bucket, metric), metric, formatDuration)}<br/>有效 ${bucket.recordedDayCount} 天 · 缺失 ${bucket.missingDayCount} 天`; } },
      xAxis: { type: "category", data: buckets.map((bucket) => bucket.startDate === bucket.endDate ? bucket.startDate.slice(5) : `${bucket.startDate.slice(5)}~${bucket.endDate.slice(5)}`), axisLine: { lineStyle: { color: border } }, axisTick: { lineStyle: { color: border } }, axisLabel: { color: muted, hideOverlap: true } },
      yAxis: { ...yAxis, axisLabel: { ...yAxis.axisLabel, color: muted }, splitLine: { lineStyle: { color: border } } },
      series: [{ type: "bar", barMaxWidth: 34, data: buckets.map((bucket, index) => ({ value: chartValue(valueFor(bucket, metric), metric), itemStyle: { color: index === selectedIndex ? primary : secondary, borderColor: index === selectedIndex ? text : "transparent", borderWidth: index === selectedIndex ? 1 : 0, borderRadius: [4, 4, 1, 1] } })) }],
      });
      chart.on("click", (event: { dataIndex?: number }) => { if (typeof event.dataIndex === "number") onSelectBucket(buckets[event.dataIndex].id); });
      const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => chart.resize());
      resize?.observe(element);
      cleanup = () => { resize?.disconnect(); chart.dispose(); };
    });
    return () => { disposed = true; cleanup?.(); };
  }, [buckets, formatDuration, metric, onSelectBucket, selectedIndex]);
  return <section className="trend-timeline-panel" aria-labelledby="trend-timeline-heading">
    <header className="trend-section-heading"><div><span>SINGLE METRIC</span><h2 id="trend-timeline-heading">时间分桶</h2></div><strong>{metricLabels[metric]}</strong></header>
    <div ref={chartRef} className="trend-echart timeline" role="img" aria-label={`${metricLabels[metric]}时间分桶柱状图`} />
    <div className="trend-chart-alternative" role="group" aria-label="时间柱图可访问替代表">{buckets.map((bucket) => <button className={`trend-bucket-button ${selectedBucketId === bucket.id ? "selected" : ""}`} type="button" aria-pressed={selectedBucketId === bucket.id} key={bucket.id} onClick={() => onSelectBucket(bucket.id)} title={`${bucket.startDate} 至 ${bucket.endDate}，${metricLabels[metric]} ${displayValue(valueFor(bucket, metric), metric, formatDuration)}，有效 ${bucket.recordedDayCount} 天，缺失 ${bucket.missingDayCount} 天`}><span>{bucket.startDate === bucket.endDate ? bucket.startDate : `${bucket.startDate}~${bucket.endDate}`}</span><strong>{displayValue(valueFor(bucket, metric), metric, formatDuration)}</strong></button>)}</div>
  </section>;
}
