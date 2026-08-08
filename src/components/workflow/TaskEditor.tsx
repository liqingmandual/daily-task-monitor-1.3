import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, RefObject } from "react";
import { X } from "lucide-react";
import type { WorkLedgerProject, WorkLedgerTask, WorkLedgerTaskPriority, WorkLedgerTaskSaveRequest } from "../../lib/desktop";

function trapDialogFocus(event: KeyboardEvent<HTMLElement>, dialog: HTMLElement | null, onClose: () => void) {
  if (event.key === "Escape") {
    event.preventDefault();
    onClose();
    return;
  }
  if (event.key !== "Tab" || !dialog) return;
  const focusable = Array.from(dialog.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex="0"]'));
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

export function TaskEditor({ task, projectId, projects, pending, onSave, onClose, restoreFocusRef }: {
  task: WorkLedgerTask | null;
  projectId: string;
  projects: WorkLedgerProject[];
  pending: boolean;
  onSave: (request: WorkLedgerTaskSaveRequest) => void;
  onClose: () => void;
  restoreFocusRef?: RefObject<HTMLElement | null>;
}) {
  const [title, setTitle] = useState(task?.title ?? "");
  const [priority, setPriority] = useState<WorkLedgerTaskPriority>(task?.priority ?? "medium");
  const [expectedOutput, setExpectedOutput] = useState(task?.expectedOutput ?? "");
  const [dueDate, setDueDate] = useState(task?.dueDate ?? "");
  const [selectedProjectId, setSelectedProjectId] = useState(task?.projectId ?? projectId);
  const firstField = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  useEffect(() => firstField.current?.focus(), []);
  useEffect(() => () => restoreFocusRef?.current?.focus(), []);

  return <div className="workflow-dialog-layer" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
    <section ref={dialogRef} className="workflow-dialog" role="dialog" aria-modal="true" aria-label={task ? "编辑任务" : "新建任务"} onKeyDown={(event) => trapDialogFocus(event, dialogRef.current, onClose)}>
      <header><div><span>TASK</span><h2>{task ? "编辑任务" : "新建任务"}</h2></div><button type="button" className="workflow-icon-button" aria-label="关闭任务编辑器" title="关闭" onClick={onClose}><X size={17} /></button></header>
      <form onSubmit={(event) => {
        event.preventDefault();
        if (pending || !title.trim()) return;
        onSave({
          id: task?.id ?? `task-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`,
          projectId: selectedProjectId,
          title: title.trim(),
          priority,
          expectedOutput: expectedOutput.trim(),
          dueDate: dueDate || null,
        });
      }}>
        <label htmlFor="workflow-task-title">任务名称</label>
        <input ref={firstField} id="workflow-task-title" value={title} onInput={(event) => setTitle(event.currentTarget.value)} disabled={pending} required />
        <label htmlFor="workflow-task-project">所属项目</label>
        <select id="workflow-task-project" value={selectedProjectId} onChange={(event) => setSelectedProjectId(event.target.value)} disabled={pending}>
          {projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}
        </select>
        <label htmlFor="workflow-task-output">预期产出</label>
        <textarea id="workflow-task-output" value={expectedOutput} onInput={(event) => setExpectedOutput(event.currentTarget.value)} disabled={pending} />
        <div className="workflow-form-grid">
          <label>优先级<select value={priority} onChange={(event) => setPriority(event.target.value as WorkLedgerTaskPriority)} disabled={pending}><option value="low">低</option><option value="medium">中</option><option value="high">高</option><option value="urgent">紧急</option></select></label>
          <label>截止日期<input type="date" value={dueDate} onChange={(event) => setDueDate(event.target.value)} disabled={pending} /></label>
        </div>
        <footer><button type="button" className="workflow-secondary-button" onClick={onClose} disabled={pending}>取消</button><button type="submit" className="workflow-primary-button" disabled={pending}>保存任务</button></footer>
      </form>
    </section>
  </div>;
}
