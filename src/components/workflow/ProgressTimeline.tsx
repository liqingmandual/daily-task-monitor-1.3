import { useEffect, useState } from "react";
import { Plus } from "lucide-react";
import type { WorkLedgerProgressEntry } from "../../lib/desktop";

const provenanceLabels: Record<WorkLedgerProgressEntry["originKind"], string> = {
  manual: "Manual",
  focus_outcome: "Focus",
  daily_actual_output: "Daily output",
};

export function ProgressTimeline({ taskId, entries, pending, onAdd }: {
  taskId: string;
  entries: WorkLedgerProgressEntry[];
  pending: boolean;
  onAdd: (taskId: string, note: string) => Promise<boolean>;
}) {
  const [note, setNote] = useState("");
  useEffect(() => setNote(""), [taskId]);
  return <section className="workflow-inspector-section" aria-labelledby="workflow-progress-heading">
    <div className="workflow-section-heading"><h3 id="workflow-progress-heading">进展</h3><span>{entries.length}</span></div>
    <form className="workflow-progress-form" onSubmit={async (event) => {
      event.preventDefault();
      if (pending || !note.trim()) return;
      if (await onAdd(taskId, note.trim())) setNote("");
    }}>
      <label className="sr-only" htmlFor="workflow-progress-note">进展说明</label>
      <textarea id="workflow-progress-note" value={note} onInput={(event) => setNote(event.currentTarget.value)} placeholder="记录可验证的进展" disabled={pending} />
      <button type="submit" className="workflow-icon-button" aria-label="添加进展" title="添加进展" disabled={pending || !note.trim()}><Plus size={16} /></button>
    </form>
    <ol className="workflow-progress-list">
      {[...entries].sort((a, b) => b.createdAtMs - a.createdAtMs).map((entry) => <li key={entry.id}><time>{new Intl.DateTimeFormat("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }).format(entry.createdAtMs)}</time><span className={`workflow-progress-origin ${entry.originKind}`}>{provenanceLabels[entry.originKind]}</span><p>{entry.note}</p></li>)}
      {!entries.length && <li className="workflow-empty">暂无进展</li>}
    </ol>
  </section>;
}
