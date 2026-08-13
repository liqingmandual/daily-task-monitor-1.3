import type { TrendBucket, TrendMetric, TrendRawRow } from "../../lib/desktop";
import {
  displayMetaForActivity,
  displayMetaForKey,
  isActivityCategory,
  isVideoPurpose,
  type ActivityDisplayKey,
  type ActivityScope,
} from "../../lib/activity-composition";

function Empty({ children }: { children: string }) { return <p className="trend-empty-copy">{children}</p>; }
function Distribution({ title, items, formatDuration }: { title: string; items: TrendBucket["drilldown"]["applicationDistribution"]; formatDuration: (seconds: number) => string }) {
  const max = Math.max(1, ...items.map((item) => item.seconds));
  return <section><h3>{title}</h3>{items.length === 0 ? <Empty>{title === "应用分布" ? "该分桶没有可用应用记录" : "该分桶没有已分类活动"}</Empty> : <ul className="trend-distribution-list">{items.map((item) => <li key={item.key} title={item.label}><span>{item.label}</span><i><b style={{ width: `${item.seconds / max * 100}%` }} /></i><strong>{formatDuration(item.seconds)}</strong></li>)}</ul>}</section>;
}

const durationMetrics = new Set<TrendMetric>([
  "monitoredSeconds",
  "activeSeconds",
  "learningSeconds",
  "idleSeconds",
  "longestFocusSeconds",
  "linkedTaskSeconds",
]);

const metricLabels: Record<TrendMetric, string> = {
  monitoredSeconds: "监控时长",
  activeSeconds: "活跃时长",
  learningSeconds: "学习时长",
  idleSeconds: "不活跃时长",
  switchCount: "切换次数",
  longestFocusSeconds: "最长专注块",
  classificationCoverage: "分类覆盖率",
  completedTaskCount: "完成任务",
  linkedTaskSeconds: "任务关联时长",
};

function metricValue(metric: TrendMetric, value: number, formatDuration: (seconds: number) => string): string {
  if (durationMetrics.has(metric)) return formatDuration(value);
  if (metric === "classificationCoverage") return `${Math.round(value * 100)}%`;
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 1 });
}

function deltaLabel(current: number, baseline: number | null): string {
  if (baseline === null || baseline === 0) return "无可比基准";
  const percent = (current - baseline) / Math.abs(baseline) * 100;
  const prefix = percent > 0 ? "+" : "";
  return `${prefix}${percent.toFixed(1)}%`;
}

function activityMetaForRow(row: TrendRawRow) {
  if (row.evidenceKind !== "activity" || !isActivityCategory(row.category)) return null;
  const purpose = isVideoPurpose(row.videoPurpose) ? row.videoPurpose : "unknown";
  return displayMetaForActivity(row.category, purpose);
}

function uniqueActivityRows(rows: TrendRawRow[]): TrendRawRow[] {
  const unique = new Map<string, TrendRawRow>();
  for (const row of rows) {
    if (!activityMetaForRow(row)) continue;
    const key = [row.evidenceId, row.date, row.startTime, row.endTime].join("|");
    if (!unique.has(key)) unique.set(key, row);
  }
  return [...unique.values()];
}

function distributionFromRows(
  rows: TrendRawRow[],
  select: (row: TrendRawRow) => { key: string; label: string } | null,
): TrendBucket["drilldown"]["applicationDistribution"] {
  const values = new Map<string, { key: string; label: string; seconds: number }>();
  for (const row of rows) {
    const selected = select(row);
    if (!selected) continue;
    const current = values.get(selected.key) ?? { ...selected, seconds: 0 };
    current.seconds += row.clippedDurationSeconds;
    values.set(selected.key, current);
  }
  return [...values.values()].sort((left, right) => right.seconds - left.seconds || left.label.localeCompare(right.label));
}

interface TrendDrilldownProps {
  bucket: TrendBucket | null;
  previousBucket?: TrendBucket | null;
  rangeMean?: number | null;
  metric?: TrendMetric;
  activityScope?: ActivityScope;
  activityFilter?: ActivityDisplayKey | null;
  formatDuration: (seconds: number) => string;
  onClearActivityFilter?: () => void;
  onOpenDate?: (date: string) => void;
  onOpenTask?: (taskId: string) => void;
}

export function TrendDrilldown({
  bucket,
  previousBucket = null,
  rangeMean = null,
  metric = "activeSeconds",
  activityScope = "all",
  activityFilter = null,
  formatDuration,
  onClearActivityFilter,
  onOpenDate,
  onOpenTask,
}: TrendDrilldownProps) {
  if (!bucket) return <section className="trend-drilldown" aria-labelledby="trend-drilldown-heading"><header className="trend-section-heading"><div><span>PERIOD DETAIL</span><h2 id="trend-drilldown-heading">所选周期详情</h2></div></header><Empty>当前范围没有可查看的有效周期</Empty></section>;
  const data = bucket.drilldown;
  const currentValue = bucket.values[metric];
  const scopedRows = uniqueActivityRows(data.rawRows).filter((row) => (
    activityScope === "all"
      || activityScope === "active" && row.category !== "idle"
      || activityScope === "meaningful" && row.meaningful === true
  ));
  const filteredRows = activityFilter
    ? scopedRows.filter((row) => activityMetaForRow(row)?.key === activityFilter)
    : scopedRows;
  const legacyDistribution = activityScope === "all" && activityFilter === null && scopedRows.length === 0;
  const applicationDistribution = legacyDistribution
    ? data.applicationDistribution
    : distributionFromRows(filteredRows, (row) => ({ key: row.app, label: row.app }));
  const categoryDistribution = legacyDistribution
    ? data.categoryDistribution
    : distributionFromRows(filteredRows, (row) => {
      const meta = activityMetaForRow(row);
      return meta ? { key: meta.key, label: meta.label } : null;
    });
  const filteredTaskIds = new Set(filteredRows.flatMap((row) => row.taskId ? [row.taskId] : []));
  const topTasks = data.linkedTaskRollups
    .filter((item) => activityScope === "all" && !activityFilter || filteredTaskIds.has(item.taskId))
    .sort((left, right) => right.linkedSeconds - left.linkedSeconds)
    .slice(0, 5);
  const filterLabel = activityFilter ? displayMetaForKey(activityFilter).label : null;
  return <section className="trend-drilldown" aria-labelledby="trend-drilldown-heading">
    <header className="trend-section-heading"><div><span>PERIOD DETAIL</span><h2 id="trend-drilldown-heading">所选周期详情</h2></div><strong>{bucket.startDate} 至 {bucket.endDate}</strong></header>
    {filterLabel && <div className="trend-drilldown-filter" role="status"><span>分类筛选：<strong>{filterLabel}</strong></span><button type="button" className="trend-compact-button" onClick={onClearActivityFilter}>清除筛选</button></div>}
    <div className="trend-period-summary" aria-label="所选周期指标比较">
      <div><span>{metricLabels[metric]}</span><strong>{metricValue(metric, currentValue, formatDuration)}</strong></div>
      <div><span>较上一周期</span><strong>{deltaLabel(currentValue, previousBucket?.values[metric] ?? null)}</strong></div>
      <div><span>较区间均值</span><strong>{deltaLabel(currentValue, rangeMean)}</strong></div>
      <div><span>专注与切换</span><strong>{formatDuration(bucket.values.longestFocusSeconds)} · {bucket.values.switchCount} 次</strong></div>
    </div>
    <div className="trend-drilldown-grid">
      <Distribution title="应用分布" items={applicationDistribution} formatDuration={formatDuration} />
      <Distribution title="分类分布" items={categoryDistribution} formatDuration={formatDuration} />
      <section><h3>完成任务</h3>{data.completedTasks.length === 0 ? <Empty>该分桶没有已完成任务</Empty> : <ul className="trend-compact-list">{data.completedTasks.map((item) => <li key={`${item.taskId}-${item.completedAtMs}`}><span title={item.taskTitle}>{item.taskTitle}</span><small title={item.projectName}>{item.projectName}</small></li>)}</ul>}</section>
      <section><h3>数据质量</h3><dl className="trend-quality-list"><div><dt>有效 / 缺失天</dt><dd>{data.dataQuality.recordedDayCount} / {data.dataQuality.missingDayCount}</dd></div><div><dt>分类覆盖</dt><dd>{Math.round(data.dataQuality.classificationCoverage * 100)}%</dd></div><div><dt>低置信</dt><dd>{formatDuration(data.dataQuality.lowConfidenceSeconds)}</dd></div><div><dt>待处理</dt><dd>{formatDuration(data.dataQuality.pendingSeconds)}</dd></div></dl></section>
    </div>
    <section className="trend-activity-evidence" aria-label="活动证据明细">
      <h3>{filterLabel ? `${filterLabel}证据` : activityScope === "meaningful" ? "学习活动证据" : activityScope === "active" ? "活跃活动证据" : "活动证据"}</h3>
      {filteredRows.length === 0 ? <Empty>当前筛选下没有活动证据</Empty> : <div className="trend-table-scroll"><table className="trend-data-table"><thead><tr><th>日期 / 时间</th><th>应用</th><th>分类</th><th>任务</th><th>时长</th></tr></thead><tbody>{filteredRows.slice(0, 50).map((row) => {
        const meta = activityMetaForRow(row);
        return <tr key={`${row.evidenceId}-${row.date}-${row.startTime}-${row.endTime}`}><th>{row.date}<small>{row.startTime}–{row.endTime}</small></th><td>{row.app}</td><td>{meta?.label ?? row.category}</td><td>{row.taskId && row.taskTitle && onOpenTask ? <button type="button" className="trend-link-button" onClick={() => onOpenTask(row.taskId!)}>{row.taskTitle}</button> : (row.taskTitle ?? "—")}</td><td>{formatDuration(row.clippedDurationSeconds)}</td></tr>;
      })}</tbody></table></div>}
    </section>
    <div className="trend-ownership-section"><header><h3>主要任务贡献</h3><p>唯一关联时长按时间并集计一次；共享证据会保留在多个任务明细中，因此任务明细合计可高于唯一总量。</p></header>
      {data.linkedTaskRollups.length === 0 ? <Empty>该分桶没有任务关联证据</Empty> : <div className="trend-table-scroll"><table className="trend-data-table"><thead><tr><th>任务 / 项目</th><th>唯一关联</th><th>活动</th><th>专注</th><th>证据</th><th>共享</th></tr></thead><tbody>{data.linkedTaskRollups.map((item) => <tr key={`${item.projectId}-${item.taskId}`}><th title={`${item.projectName} / ${item.taskTitle}`}>{item.taskTitle}<small>{item.projectName}</small></th><td>{formatDuration(item.linkedSeconds)}</td><td>{formatDuration(item.activitySeconds)}</td><td>{formatDuration(item.focusSeconds)}</td><td>{item.evidenceCount}</td><td>{item.sharedEvidenceCount}</td></tr>)}</tbody></table></div>}
      <div className="trend-ownership-grid">
        <section><h3>项目归集</h3>{data.linkedProjectRollups.length === 0 ? <Empty>该分桶没有项目归属</Empty> : <ul className="trend-compact-list">{data.linkedProjectRollups.map((item) => <li key={item.projectId}><span title={item.projectName}>{item.projectName}</span><small>{formatDuration(item.linkedSeconds)} · 活动 {formatDuration(item.activitySeconds)} · 专注 {formatDuration(item.focusSeconds)}</small></li>)}</ul>}</section>
        <section><h3>工作流归属证据</h3>{data.workflowOwnership.length === 0 ? <Empty>该分桶没有工作流归属记录</Empty> : <ul className="trend-compact-list">{data.workflowOwnership.map((item) => <li key={item.ownershipId}><span title={`${item.projectName} / ${item.taskTitle}`}>{item.taskTitle}</span><small>{item.projectName} · {item.evidenceKind === "activity" ? "活动" : "专注"}{item.shared ? " · 共享" : ""}</small></li>)}</ul>}</section>
      </div>
    </div>
    {(onOpenDate || onOpenTask) && <div className="trend-period-actions">
      {onOpenDate && <button type="button" className="trend-compact-button" onClick={() => onOpenDate(bucket.endDate)}>查看今日时间线</button>}
      {onOpenTask && topTasks.map((item) => <button type="button" className="trend-compact-button" key={item.taskId} onClick={() => onOpenTask(item.taskId)}>查看任务：{item.taskTitle}</button>)}
    </div>}
  </section>;
}
