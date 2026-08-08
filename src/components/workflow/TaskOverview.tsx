import { Download, GitMerge } from "lucide-react";
import { useState } from "react";
import type { ReportFormat, TaskTimeInsight, WorkLedgerTask } from "../../lib/desktop";
import { formatLedgerDuration } from "./TaskLedger";

export function TaskOverview({
  insight,
  loading,
  error = "",
  mergeTargets,
  pending = false,
  onMerge,
  onExport,
}: {
  insight: TaskTimeInsight | null;
  loading: boolean;
  error?: string;
  mergeTargets: WorkLedgerTask[];
  pending?: boolean;
  onMerge: (targetTaskId: string) => void;
  onExport: (format: ReportFormat) => void;
}) {
  const [mergeTarget, setMergeTarget] = useState("");
  if (loading) return <p className="workflow-overview-state" role="status">正在汇总任务生命周期数据…</p>;
  if (error) return <p className="workflow-overview-state error" role="alert">{error}</p>;
  if (!insight) return <p className="workflow-overview-state">暂无可用的任务洞察。</p>;
  const { summary } = insight;
  const maximum = Math.max(1, ...insight.dailyPoints.map((point) => point.investedSeconds));
  return <div className="workflow-task-overview">
    <div className="workflow-overview-metrics">
      <div><small>生命周期总投入</small><strong>{formatLedgerDuration(summary.lifecycleTotalSeconds)}</strong></div>
      <div><small>活跃日均 · {summary.activeDayCount} 天</small><strong>{formatLedgerDuration(summary.activeDayAverageSeconds)}</strong></div>
      <div><small>自然日均 · {summary.naturalDayCount} 天</small><strong>{formatLedgerDuration(summary.naturalDayAverageSeconds)}</strong></div>
      <div><small>日投入中位数</small><strong>{formatLedgerDuration(insight.medianDailySeconds)}</strong></div>
      <div><small>最长连续片段</small><strong>{formatLedgerDuration(insight.longestContinuousSeconds)}</strong></div>
      <div><small>专注占比</small><strong>{Math.round(insight.focusShare * 100)}%</strong></div>
    </div>

    <section className="workflow-overview-section">
      <header><div><span>DAILY SERIES</span><h3>每日投入</h3></div><small>活动与专注区间并集</small></header>
      {insight.dailyPoints.length ? <>
        <div className="workflow-daily-bars" role="img" aria-label="任务每日投入柱状图">
          {insight.dailyPoints.map((point) => <div key={point.date} title={`${point.date} · ${formatLedgerDuration(point.investedSeconds)}`}>
            <i style={{ height: `${Math.max(4, point.investedSeconds / maximum * 100)}%` }} />
            <span>{point.date.slice(5)}</span>
          </div>)}
        </div>
        <details className="workflow-data-table">
          <summary>查看图表替代表</summary>
          <table>
            <thead><tr><th>日期</th><th>总投入</th><th>活动</th><th>专注</th><th>切换</th></tr></thead>
            <tbody>{insight.dailyPoints.map((point) => <tr key={point.date}>
              <td>{point.date}</td>
              <td>{formatLedgerDuration(point.investedSeconds)}</td>
              <td>{formatLedgerDuration(point.activitySeconds)}</td>
              <td>{formatLedgerDuration(point.focusSeconds)}</td>
              <td>{point.switchCount}</td>
            </tr>)}</tbody>
          </table>
        </details>
      </> : <p className="workflow-empty">所选截止日期前暂无计时片段。</p>}
    </section>

    <section className="workflow-overview-section">
      <header><div><span>QUALITY</span><h3>任务质量 · 透明维度</h3></div><small>不合成黑箱总分</small></header>
      <div className="workflow-quality-dimensions">
        {insight.assessment.dimensions.map((dimension) => <article key={dimension.key}>
          <strong>{dimension.label}</strong>
          <p>{dimension.conclusion}</p>
        </article>)}
      </div>
      {insight.assessment.dataLimitations.length > 0 && <div className="workflow-data-limitations">
        <strong>数据限制</strong>
        <ul>{insight.assessment.dataLimitations.map((limitation) => <li key={limitation}>{limitation}</li>)}</ul>
      </div>}
    </section>

    <section className="workflow-overview-section workflow-overview-actions">
      <header><div><span>ACTIONS</span><h3>导出与整理</h3></div></header>
      <div className="workflow-export-buttons">
        <button type="button" disabled={pending} onClick={() => onExport("markdown")}><Download size={14} />Markdown</button>
        <button type="button" disabled={pending} onClick={() => onExport("docx")}><Download size={14} />Word</button>
      </div>
      {mergeTargets.length > 0 && <div className="workflow-merge-control">
        <label htmlFor="workflow-merge-target">合并到任务</label>
        <select id="workflow-merge-target" value={mergeTarget} onChange={(event) => setMergeTarget(event.target.value)} disabled={pending}>
          <option value="">选择目标任务</option>
          {mergeTargets.map((task) => <option key={task.id} value={task.id}>{task.title}</option>)}
        </select>
        <button type="button" disabled={!mergeTarget || pending} onClick={() => onMerge(mergeTarget)}><GitMerge size={14} />合并</button>
      </div>}
    </section>
  </div>;
}
