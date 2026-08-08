import type { CSSProperties } from "react";
import type { TrendMetric, TrendWorkbenchSummary } from "../../lib/desktop";

const metricAccents: Record<"monitoredSeconds" | "activeSeconds" | "idleSeconds" | "learningSeconds" | "switchCount" | "longestFocusSeconds", string> = {
  monitoredSeconds: "#0f9f8f",
  activeSeconds: "#2563eb",
  idleSeconds: "#64748b",
  learningSeconds: "#e58a12",
  switchCount: "#8b5cf6",
  longestFocusSeconds: "#0f9f8f",
};

function compactDurationLabel(value: string) {
  return value.replace(/\s+/g, "").replace("分钟", "分");
}

function shareLabel(value: number, monitoredSeconds: number): string {
  return monitoredSeconds > 0 ? `${(value / monitoredSeconds * 100).toFixed(1)}%` : "—";
}

export function TrendStatisticsStrip({
  summary,
  selectedDayCount,
  metric,
  formatDuration,
  onMetricChange,
}: {
  summary: TrendWorkbenchSummary;
  selectedDayCount: number;
  metric: TrendMetric;
  formatDuration: (seconds: number) => string;
  onMetricChange: (metric: TrendMetric) => void;
}) {
  const monitoredSeconds = summary.totals.monitoredSeconds;
  const durationCards = [
    ["总监测", "monitoredSeconds"],
    ["活跃", "activeSeconds"],
    ["不活跃", "idleSeconds"],
    ["学习", "learningSeconds"],
  ] as const;
  const cards: Array<{
    label: string;
    value: string;
    detail: string;
    nextMetric: keyof typeof metricAccents;
    share: string;
  }> = durationCards.map(([label, nextMetric]) => {
    const total = summary.totals[nextMetric];
    return {
      label,
      value: compactDurationLabel(formatDuration(total)),
      detail: selectedDayCount > 0
        ? `日均 ${formatDuration(total / selectedDayCount)} · 占日均总监测 ${shareLabel(total, monitoredSeconds)}`
        : "日均 — · 占日均总监测 —",
      nextMetric,
      share: shareLabel(total, monitoredSeconds),
    };
  });
  cards.push({
    label: "切换频率",
    value: summary.switchesPerActiveHour === null ? "—" : `${summary.switchesPerActiveHour.toFixed(1)} 次/小时`,
    detail: `共 ${Math.round(summary.totals.switchCount)} 次`,
    nextMetric: "switchCount",
    share: "",
  });
  cards.push({
    label: "最长专注",
    value: compactDurationLabel(formatDuration(summary.totals.longestFocusSeconds)),
    detail: `占总监测 ${shareLabel(summary.totals.longestFocusSeconds, monitoredSeconds)}`,
    nextMetric: "longestFocusSeconds",
    share: shareLabel(summary.totals.longestFocusSeconds, monitoredSeconds),
  });

  return (
    <section className="trend-statistics-section" aria-label="趋势统计概览">
      <p className="trend-statistics-scope">日均按全部已选日期计算，无记录日按零计入</p>
      <div className="trend-overview-grid interval" data-responsive-columns="6 3 2">
        {cards.map((card) => (
          <button
            type="button"
            className="trend-overview-card"
            style={{ "--trend-accent": metricAccents[card.nextMetric] } as CSSProperties}
            data-trend-overview-card="interval"
            data-trend-overview-metric={card.nextMetric}
            data-trend-accent={metricAccents[card.nextMetric]}
            data-trend-share={card.share || undefined}
            aria-pressed={metric === card.nextMetric}
            key={card.nextMetric}
            onClick={() => onMetricChange(card.nextMetric)}
          >
            <span>{card.label}</span>
            <strong>{card.value}</strong>
            <small>{card.detail}</small>
          </button>
        ))}
      </div>
    </section>
  );
}
