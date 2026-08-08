import { ExternalLink, Link2, Link2Off, Monitor, Globe2 } from "lucide-react";
import type { LinkedWorkLedgerEvidence, WorkLedgerEvidence } from "../../lib/desktop";
import { formatLedgerDuration } from "./TaskLedger";

function EvidenceIcon({ kind }: { kind: WorkLedgerEvidence["kind"] }) {
  return kind === "activity" ? <Monitor size={15} /> : <Globe2 size={15} />;
}

function EvidenceMeta({ evidence }: { evidence: WorkLedgerEvidence }) {
  const classification = evidence.classificationConfidence === null
    ? "未分类"
    : `分类 ${Math.round(evidence.classificationConfidence * 100)}%`;
  return <span>{evidence.application}{evidence.domain ? ` · ${evidence.domain}` : ""} · {evidence.durationSeconds ? formatLedgerDuration(evidence.durationSeconds) : classification}</span>;
}

export function EvidenceAssignmentPanel({ taskId, linked, unassigned, pendingAssignments, pendingRemovals, onAssign, onRemove, onOpenTimeline }: {
  taskId: string;
  linked: LinkedWorkLedgerEvidence[];
  unassigned: WorkLedgerEvidence[];
  pendingAssignments: Set<string>;
  pendingRemovals: Set<string>;
  onAssign: (evidence: WorkLedgerEvidence) => void;
  onRemove: (evidence: WorkLedgerEvidence) => void;
  onOpenTimeline?: (evidence: WorkLedgerEvidence) => void;
}) {
  return <section className="workflow-inspector-section" aria-labelledby="workflow-evidence-heading">
    <div className="workflow-section-heading"><h3 id="workflow-evidence-heading">证据</h3><span>{linked.length} 条已关联证据</span></div>
    <div className="workflow-evidence-group">
      <h4>已关联</h4>
      {linked.map((item) => <article className="workflow-evidence-row" key={`${item.evidence.kind}-${item.evidence.id}`}>
        <EvidenceIcon kind={item.evidence.kind} />
        <div><strong>{item.evidence.title}</strong><EvidenceMeta evidence={item.evidence} /><small>{item.provenance === "manual" ? "手动" : item.provenance === "ai" ? "AI" : "规则"} · {item.assignmentReason}</small></div>
        <div className="workflow-row-actions">
          {item.evidence.kind === "activity" && onOpenTimeline && <button type="button" aria-label={`在今日时间线中查看：${item.evidence.title}`} title="在今日时间线中查看" onClick={() => onOpenTimeline(item.evidence)}><ExternalLink size={14} /></button>}
          <button type="button" aria-label={`移除证据：${item.evidence.title}`} title="移除证据" disabled={pendingRemovals.has(`${item.evidence.kind}:${item.evidence.id}`)} onClick={() => onRemove(item.evidence)}><Link2Off size={14} /></button>
        </div>
      </article>)}
      {!linked.length && <p className="workflow-empty">尚未关联证据</p>}
    </div>
    <div className="workflow-evidence-group">
      <h4>可分配</h4>
      {unassigned.map((evidence) => <article className="workflow-evidence-row" key={`${evidence.kind}-${evidence.id}`}>
        <EvidenceIcon kind={evidence.kind} />
        <div><strong>{evidence.title}</strong><EvidenceMeta evidence={evidence} /></div>
        <button type="button" className="workflow-icon-button" aria-label={`分配证据：${evidence.title}`} title="分配到当前任务" disabled={pendingAssignments.has(`${evidence.kind}:${evidence.id}`)} onClick={() => onAssign(evidence)}><Link2 size={14} /></button>
      </article>)}
      {!unassigned.length && <p className="workflow-empty">没有未分配证据</p>}
    </div>
    <span className="sr-only">当前任务 {taskId}</span>
  </section>;
}
