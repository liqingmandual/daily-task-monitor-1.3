import {
  Ban,
  BrainCircuit,
  CalendarDays,
  CheckCircle2,
  Clock3,
  FlaskConical,
  Gauge,
  SearchCheck,
  Sparkles,
  Target,
} from "lucide-react";

import type {
  ProjectTimeInsight,
  WorkLedgerProject,
  WorkLedgerTask,
} from "../../lib/desktop";
import { DonutChart } from "../today/analysis-shared";
import { ProjectDailyStackedChart, workflowChartPalette } from "./ProjectDailyStackedChart";

function compactDuration(seconds: number) {
  if (seconds < 60) return `${Math.max(0, Math.round(seconds))}秒`;
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor(seconds % 3_600 / 60);
  return hours ? `${hours}小时${minutes}分` : `${minutes}分钟`;
}

function projectEvidenceFinding(insight: ProjectTimeInsight) {
  const activePoints = insight.dailyPoints.filter((point) => point.investedSeconds > 0);
  const recent = insight.dailyPoints.slice(-7).reduce((sum, point) => sum + point.investedSeconds, 0);
  const previous = insight.dailyPoints.slice(-14, -7).reduce((sum, point) => sum + point.investedSeconds, 0);
  const topTask = insight.taskContributions.find((task) => task.investedSeconds > 0);
  const recentDirection = recent > previous ? "增加" : recent < previous ? "减少" : "持平";
  return {
    fact: `近 7 天投入 ${compactDuration(recent)}，较此前 7 天${recentDirection}；${topTask ? `主要任务为“${topTask.taskTitle}”` : "暂无主要任务"}。`,
    hypothesis: "若变化来自稳定的学习或开发节律，而不是一次性事件，下一完整周期的活跃天数和日均投入应出现同方向变化。",
    validation: `继续记录 7 天，对比活跃日、自然日均与实际产出；当前结论基于 ${activePoints.length} 个有投入日期。`,
    action: topTask
      ? `为“${topTask.taskTitle}”设定一个可核验产出，并优先减少跨任务切换。`
      : "先积累至少两个独立工作时段，再评估任务边界与节律。",
  };
}

export function ProjectDashboard({
  project,
  tasks,
  insight,
  loading,
  error,
  rangeDays,
  onRangeChange,
  onSelectTask,
  selectedTaskId,
  onOpenDate,
  onCancelTask,
  pendingCancelTaskId,
}: {
  project: WorkLedgerProject;
  tasks: WorkLedgerTask[];
  insight: ProjectTimeInsight | null;
  loading: boolean;
  error: string;
  rangeDays: number;
  onRangeChange: (days: number) => void;
  onSelectTask: (taskId: string) => void;
  selectedTaskId?: string | null;
  onOpenDate?: (date: string) => void;
  onCancelTask?: (task: WorkLedgerTask) => void;
  pendingCancelTaskId?: string;
}) {
  const summary = insight?.summary;
  const taskById = new Map(tasks.map((task) => [task.id, task]));
  const donutItems = (insight?.taskContributions ?? [])
    .filter((item) => item.investedSeconds > 0)
    .map((item, index) => ({
      key: item.taskId,
      name: item.taskTitle,
      value: item.investedSeconds,
      color: workflowChartPalette[index % workflowChartPalette.length],
    }));
  if ((insight?.sharedSeconds ?? 0) > 0) {
    donutItems.push({
      key: "__shared__",
      name: "共享证据",
      value: insight?.sharedSeconds ?? 0,
      color: "#94a3b8",
    });
  }
  const finding = insight ? projectEvidenceFinding(insight) : null;
  const taskContributionById = new Map((insight?.taskContributions ?? []).map((item) => [item.taskId, item]));
  const totalSeconds = summary?.lifecycleTotalSeconds ?? 0;

  return (
    <section className="workflow-project-dashboard" aria-label={`${project.name} 工作流统计`}>
      {loading && <p className="workflow-dashboard-state" role="status">正在计算工作流统计...</p>}
      {error && <p className="workflow-dashboard-state error" role="alert">{error}</p>}
      {!loading && !error && summary && (
        <div className="workflow-project-kpis">
          <article><Clock3 size={17} /><span>总投入</span><strong>{compactDuration(summary.lifecycleTotalSeconds)}</strong></article>
          <article><Gauge size={17} /><span>活跃日均</span><strong>{compactDuration(summary.activeDayAverageSeconds)}</strong></article>
          <article><CalendarDays size={17} /><span>自然日均</span><strong>{compactDuration(summary.naturalDayAverageSeconds)}</strong></article>
          <article><Target size={17} /><span>活跃天数</span><strong>{summary.activeDayCount}天</strong></article>
          <article><Sparkles size={17} /><span>最长专注</span><strong>{compactDuration(insight?.longestContinuousSeconds ?? 0)}</strong></article>
          <article><BrainCircuit size={17} /><span>切换负荷</span><strong>{insight?.switchCount ?? 0}次</strong></article>
        </div>
      )}

      {!error && (
        <>
          <div className="workflow-project-visuals">
            <article className="workflow-project-chart-card workflow-composition-card">
              <header>
                <div><strong>任务构成</strong><span>按投入时间</span></div>
              </header>
              <div className="workflow-composition-body">
                {donutItems.length ? (
                  <DonutChart
                    items={donutItems}
                    ariaLabel="工作流任务时间构成"
                    centerLabel="总投入"
                    centerValue={compactDuration(totalSeconds)}
                    onSelect={(item) => item.key !== "__shared__" && taskById.has(item.key) && onSelectTask(item.key)}
                  />
                ) : <p className="workflow-dashboard-empty">自动识别到任务后显示构成。</p>}
                <ul className="workflow-project-legend">
                  {donutItems.map((item) => (
                    <li key={item.key}>
                      <button
                        type="button"
                        disabled={item.key === "__shared__"}
                        onClick={() => item.key !== "__shared__" && onSelectTask(item.key)}
                      >
                        <i style={{ backgroundColor: item.color }} />
                        <span>{item.name}</span>
                        <strong>{totalSeconds ? `${Math.round(item.value / totalSeconds * 100)}%` : "0%"}</strong>
                        <small>{compactDuration(item.value)}</small>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            </article>

            <article className="workflow-project-chart-card workflow-daily-chart-card">
              <header>
                <div><strong>每日投入</strong><span>{rangeDays ? `过去 ${rangeDays} 天` : "全部日期"}</span></div>
                <div className="workflow-range-switch" role="group" aria-label="每日投入图范围">
                  {[7, 30, 90, 0].map((days) => (
                    <button
                      type="button"
                      key={days}
                      aria-pressed={rangeDays === days}
                      onClick={() => onRangeChange(days)}
                    >{days || "全部"}</button>
                  ))}
                </div>
              </header>
              <ProjectDailyStackedChart
                points={insight?.dailyPoints ?? []}
                tasks={insight?.taskContributions ?? []}
                formatDuration={compactDuration}
                onOpenDate={onOpenDate}
              />
            </article>
          </div>

          <div className="workflow-project-lower-grid">
            <article className="workflow-task-summary-card">
              <header>
                <div><strong>任务列表</strong><span>按投入排序</span></div>
              </header>
              <div className="workflow-task-summary-head" aria-hidden="true">
                <span>任务</span><span>总投入</span><span>占比</span><span />
              </div>
              <ol>
                {tasks.map((task) => {
                  const contribution = taskContributionById.get(task.id);
                  const invested = contribution?.investedSeconds ?? 0;
                  return (
                    <li key={task.id}>
                      <button
                        type="button"
                        className="workflow-task-summary-select workflow-task-select"
                        aria-pressed={selectedTaskId === task.id}
                        aria-label={`查看任务：${task.title}`}
                        onClick={() => onSelectTask(task.id)}
                      >
                        <b>{task.title}</b>
                        <span>{compactDuration(invested)}</span>
                        <span>{totalSeconds ? `${Math.round(invested / totalSeconds * 100)}%` : "0%"}</span>
                      </button>
                      {task.originKind === "ai" && onCancelTask && (
                        <button
                          type="button"
                          className="workflow-cancel-recognition"
                          disabled={pendingCancelTaskId === task.id}
                          aria-label={`取消错误识别：${task.title}`}
                          title="取消错误识别并释放自动证据"
                          onClick={() => onCancelTask(task)}
                        ><Ban size={14} /></button>
                      )}
                    </li>
                  );
                })}
              </ol>
              {!tasks.length && <p className="workflow-dashboard-empty">正在等待自动识别有监控意义的任务。</p>}
            </article>

            <article className="workflow-project-analysis">
              <header>
                <div><strong>AI 洞察</strong><span>基于证据</span></div>
              </header>
              {finding ? (
                <dl className="workflow-insight-rows">
                  <div><dt><SearchCheck size={15} /><span>事实</span></dt><dd>{finding.fact}</dd></div>
                  <div><dt><BrainCircuit size={15} /><span>假设</span></dt><dd>{finding.hypothesis}</dd></div>
                  <div><dt><FlaskConical size={15} /><span>验证</span></dt><dd>{finding.validation}</dd></div>
                  <div><dt><CheckCircle2 size={15} /><span>建议</span></dt><dd>{finding.action}</dd></div>
                </dl>
              ) : <p className="workflow-dashboard-empty">积累更多证据后生成洞察。</p>}
            </article>
          </div>
        </>
      )}
    </section>
  );
}
