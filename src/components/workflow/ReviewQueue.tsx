import { CheckCheck, Sparkles } from "lucide-react";
import type { WorkLedgerEvidence, WorkLedgerSuggestion, WorkLedgerTask } from "../../lib/desktop";

export function ReviewQueue({ suggestions, evidence, tasks, selected, pending, onToggle, onConfirm }: {
  suggestions: WorkLedgerSuggestion[];
  evidence: WorkLedgerEvidence[];
  tasks: WorkLedgerTask[];
  selected: Set<string>;
  pending: boolean;
  onToggle: (key: string) => void;
  onConfirm: () => void;
}) {
  return <section className="workflow-inspector-section" aria-labelledby="workflow-review-heading">
    <div className="workflow-section-heading"><h3 id="workflow-review-heading">待归档</h3><span>{suggestions.length}</span></div>
    <p className="workflow-review-note"><Sparkles size={14} />建议仅供确认，不会自动覆盖手动分配。</p>
    <div className="workflow-review-list">
      {suggestions.map((suggestion) => {
        const key = `${suggestion.source}-${suggestion.evidenceKind}-${suggestion.evidenceId}-${suggestion.taskId}`;
        const item = evidence.find((candidate) => candidate.kind === suggestion.evidenceKind && candidate.id === suggestion.evidenceId);
        const task = tasks.find((candidate) => candidate.id === suggestion.taskId);
        return <label className="workflow-review-row" key={key}>
          <input type="checkbox" checked={selected.has(key)} disabled={pending} onChange={() => onToggle(key)} />
          <span><strong>{item?.title ?? suggestion.evidenceId}</strong><small>建议归入：{task?.title ?? "未知任务"}</small><em>{suggestion.source === "local" ? "本地建议" : "AI 建议"} · {Math.round(suggestion.confidence * 100)}%</em><small>{suggestion.reason}</small></span>
        </label>;
      })}
      {!suggestions.length && <p className="workflow-empty">审核队列已清空</p>}
    </div>
    <button type="button" className="workflow-primary-button workflow-confirm-button" aria-label="批量确认建议" disabled={pending || !selected.size} onClick={onConfirm}><CheckCheck size={15} />确认 {selected.size || "所选"} 项</button>
  </section>;
}
