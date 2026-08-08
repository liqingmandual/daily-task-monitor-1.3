import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  CircleDot,
  GitBranch,
  RotateCcw,
  UserRoundPen,
} from "lucide-react";
import type { AiReviewRecord } from "../../lib/desktop";
import { reviewSourceLabel } from "../../lib/ai-review";

const stateLabels: Record<AiReviewRecord["state"], string> = {
  pending: "待审核",
  auto_applied: "已自动应用",
  manual_override: "人工改写",
  execution_error: "执行失败",
  dismissed: "已忽略",
  reverted: "已撤销",
};

function StateIcon({ state }: { state: AiReviewRecord["state"] }) {
  if (state === "auto_applied") return <CheckCircle2 size={14} />;
  if (state === "manual_override") return <UserRoundPen size={14} />;
  if (state === "execution_error") return <AlertTriangle size={14} />;
  if (state === "reverted") return <RotateCcw size={14} />;
  return <CircleDot size={14} />;
}

export function AiReviewTable({
  records,
  activeId,
  selectedIds,
  pendingIds,
  onOpen,
  onToggle,
}: {
  records: AiReviewRecord[];
  activeId: string | null;
  selectedIds: Set<string>;
  pendingIds: Set<string>;
  onOpen: (record: AiReviewRecord) => void;
  onToggle: (reviewId: string) => void;
}) {
  return <section className="ai-review-table-shell" aria-label="AI 审核记录">
    <div className="ai-review-table-head" aria-hidden="true">
      <span />
      <span>审核对象</span>
      <span>来源</span>
      <span>置信度</span>
      <span>状态</span>
      <span>时间</span>
    </div>
    <div className="ai-review-table-body">
      {records.map((record) => <div
        className={`ai-review-row ${activeId === record.id ? "active" : ""}`}
        data-pending={pendingIds.has(record.id) || undefined}
        key={record.id}
      >
        <label className="ai-review-select">
          <input
            type="checkbox"
            aria-label={`选择审核 ${record.id}`}
            checked={selectedIds.has(record.id)}
            disabled={pendingIds.has(record.id) || record.state !== "pending"}
            onChange={() => onToggle(record.id)}
          />
        </label>
        <button type="button" className="ai-review-open" aria-pressed={activeId === record.id} onClick={() => onOpen(record)}>
          <span className="ai-review-kind-icon" aria-hidden="true">{record.kind === "classification" ? <Bot size={15} /> : <GitBranch size={15} />}</span>
          <span><strong>{record.evidenceSummary || record.subjectId}</strong><small>{record.kind === "classification" ? "活动分类" : "工作流分配"} · {record.subjectId}</small></span>
        </button>
        <span className="ai-review-cell source">{record.execution.executionMode === null ? <UserRoundPen size={13} aria-hidden="true" /> : <Bot size={13} aria-hidden="true" />}<span>{reviewSourceLabel(record)}</span></span>
        <span className={`ai-review-cell confidence ${record.confidence !== null && record.confidence < .85 ? "low" : ""}`}>
          {record.confidence !== null && record.confidence < .85 ? <AlertTriangle size={13} aria-hidden="true" /> : <CheckCircle2 size={13} aria-hidden="true" />}
          <span>{record.confidence === null ? "无评分" : `${Math.round(record.confidence * 100)}%`}</span>
        </span>
        <span className={`ai-review-cell state state-${record.state}`}><StateIcon state={record.state} /><span>{stateLabels[record.state]}</span></span>
        <time className="ai-review-time" dateTime={new Date(record.createdAtMs).toISOString()}>{new Date(record.createdAtMs).toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false })}</time>
      </div>)}
      {!records.length && <div className="ai-review-empty"><CheckCircle2 size={22} /><strong>当前筛选下没有记录</strong><span>调整筛选条件或切换审核页签。</span></div>}
    </div>
  </section>;
}
