import { Check, ListTodo, Pencil, Plus } from "lucide-react";
import type { LinkedWorkLedgerEvidence, TaskTimeSummary, WorkLedgerTask } from "../../lib/desktop";

const statusLabels: Record<WorkLedgerTask["status"], string> = {
  todo: "待开始",
  in_progress: "进行中",
  blocked: "受阻",
  completed: "已完成",
  cancelled: "已取消",
};

export function formatLedgerDuration(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return `${Math.max(0, Math.round(seconds))} 秒`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (!hours) return `${minutes} 分钟`;
  return rest ? `${hours} 小时 ${rest} 分钟` : `${hours} 小时`;
}

function formatActivityTime(ms: number | null): string {
  if (!ms) return "暂无";
  return new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false }).format(ms);
}

export function TaskLedger({
  projectName,
  tasks,
  taskSummaries = [],
  linkedEvidence,
  selectedTaskId,
  pendingTaskId,
  onSelect,
  onCreate,
  onEdit,
  onComplete,
}: {
  projectName: string;
  tasks: WorkLedgerTask[];
  taskSummaries?: TaskTimeSummary[];
  linkedEvidence: LinkedWorkLedgerEvidence[];
  selectedTaskId: string | null;
  pendingTaskId?: string;
  onSelect: (taskId: string) => void;
  onCreate: () => void;
  onEdit: (task: WorkLedgerTask) => void;
  onComplete: (task: WorkLedgerTask) => void;
}) {
  return <section className="workflow-task-ledger" aria-label="任务台账">
    <header className="workflow-ledger-header">
      <div><span>任务台账</span><h2>{projectName || "未选择项目"}</h2></div>
      <button type="button" className="workflow-primary-button" aria-label="新建任务" onClick={onCreate} disabled={!projectName}><Plus size={15} />新建任务</button>
    </header>
    <div className="workflow-ledger-columns" aria-hidden="true">
      <span>状态</span><span>任务与预期产出</span><span>生命周期投入</span><span>活跃日均</span><span>最近活动</span><span>归属可信度</span>
    </div>
    <div className="workflow-task-list">
      {tasks.map((task) => {
        const evidence = linkedEvidence.filter((item) => item.taskId === task.id);
        const summary = taskSummaries.find((item) => item.taskId === task.id);
        const invested = summary?.lifecycleTotalSeconds
          ?? evidence.reduce((sum, item) => sum + item.evidence.durationSeconds, 0);
        const activeAverage = summary?.activeDayAverageSeconds ?? invested;
        const latest = summary?.latestActivityAtMs
          ?? evidence.reduce<number | null>((value, item) => Math.max(value ?? 0, item.evidence.occurredAtMs), null);
        const fallbackConfidence = evidence.length
          ? evidence.reduce((sum, item) => sum + item.assignmentConfidence, 0) / evidence.length
          : null;
        const assignmentConfidence = summary?.assignmentConfidence ?? fallbackConfidence;
        const confidence = assignmentConfidence === null ? null : Math.round(assignmentConfidence * 100);
        return <article className={`workflow-task-row ${selectedTaskId === task.id ? "selected" : ""}`} key={task.id}>
          <button type="button" className="workflow-task-select" onClick={() => onSelect(task.id)} aria-pressed={selectedTaskId === task.id}>
            <span className={`workflow-status status-${task.status}`}><i />{statusLabels[task.status]}</span>
            <span className="workflow-task-copy"><strong>{task.title}{task.reviewState === "provisional" && <em>AI 暂定</em>}</strong><small>{task.expectedOutput || "未填写预期产出"}</small></span>
            <span className="workflow-task-stat"><small>生命周期</small><b>累计 {formatLedgerDuration(invested)}</b></span>
            <span className="workflow-task-stat"><small>{summary ? `${summary.activeDayCount} 个活跃日` : "当前范围"}</small><b>日均 {formatLedgerDuration(activeAverage)}</b></span>
            <span className="workflow-task-stat"><small>最近活动</small><b>{formatActivityTime(latest)}</b></span>
            <span className="workflow-task-stat"><small>{evidence.length} 条证据</small><b>{confidence === null ? "待补充" : `归属 ${confidence}%`}</b></span>
          </button>
          <div className="workflow-row-actions workflow-task-actions">
            <button type="button" aria-label={`编辑任务：${task.title}`} title="编辑任务" disabled={pendingTaskId === task.id} onClick={() => onEdit(task)}><Pencil size={14} /></button>
            {task.status !== "completed" && <button type="button" aria-label={`完成任务：${task.title}`} title="完成任务" disabled={pendingTaskId === task.id} onClick={() => onComplete(task)}><Check size={15} /></button>}
          </div>
        </article>;
      })}
      {!tasks.length && <div className="workflow-ledger-empty"><ListTodo size={22} /><strong>暂无任务</strong><span>新建任务后即可关联活动证据。</span></div>}
    </div>
  </section>;
}
