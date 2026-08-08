import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Bot,
  Check,
  CheckCircle2,
  FileJson2,
  Fingerprint,
  GitBranch,
  RotateCcw,
  ShieldAlert,
  UserRoundPen,
  X,
} from "lucide-react";
import type { AiReviewRecord, ProjectDraftProposal } from "../../lib/desktop";
import {
  parseReviewJson,
  reviewCandidates,
  reviewSourceLabel,
  sanitizeReviewDiagnostic,
  validateChangedJson,
} from "../../lib/ai-review";

const stateLabels: Record<AiReviewRecord["state"], string> = {
  pending: "待审核",
  auto_applied: "已自动应用",
  manual_override: "人工改写",
  execution_error: "执行失败",
  dismissed: "已忽略",
  reverted: "已撤销",
};

function JsonView({ label, value }: { label: string; value: string | null }) {
  if (!value) return null;
  return <div className="ai-review-json-block">
    <span><FileJson2 size={13} />{label}</span>
    <pre>{JSON.stringify(parseReviewJson(value), null, 2)}</pre>
  </div>;
}

function ProjectDraftEditor({ value, disabled, onChange }: {
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  let draft: ProjectDraftProposal;
  try {
    draft = JSON.parse(value) as ProjectDraftProposal;
  } catch {
    return null;
  }
  if (!Array.isArray(draft.tasks)) return null;
  const commit = (next: ProjectDraftProposal) => onChange(JSON.stringify(next));
  const replaceTask = (index: number, patch: Partial<ProjectDraftProposal["tasks"][number]>) => {
    commit({ ...draft, tasks: draft.tasks.map((task, taskIndex) => taskIndex === index ? { ...task, ...patch } : task) });
  };
  const moveCluster = (clusterId: string, targetIndex: number) => {
    commit({
      ...draft,
      tasks: draft.tasks.map((task, index) => ({
        ...task,
        clusterIds: index === targetIndex
          ? [...task.clusterIds.filter((id) => id !== clusterId), clusterId]
          : task.clusterIds.filter((id) => id !== clusterId),
      })),
    });
  };
  const removeTask = (index: number) => {
    if (draft.tasks.length <= 1) return;
    const destination = index === 0 ? 1 : 0;
    const moved = draft.tasks[index].clusterIds;
    const tasks = draft.tasks
      .map((task, taskIndex) => taskIndex === destination ? { ...task, clusterIds: [...task.clusterIds, ...moved] } : task)
      .filter((_, taskIndex) => taskIndex !== index);
    commit({ ...draft, tasks });
  };
  return <div className="ai-review-project-draft-editor">
    <label><span>项目名称</span><input value={draft.name} disabled={disabled} onInput={(event) => commit({ ...draft, name: event.currentTarget.value })} /></label>
    <label><span>项目说明</span><textarea value={draft.description} disabled={disabled} onInput={(event) => commit({ ...draft, description: event.currentTarget.value })} /></label>
    <div className="ai-review-project-draft-tasks">
      {draft.tasks.map((task, index) => <fieldset key={task.key}>
        <legend>任务 {index + 1}</legend>
        <label><span>任务标题</span><input value={task.title} disabled={disabled} onInput={(event) => replaceTask(index, { title: event.currentTarget.value })} /></label>
        <label><span>预期产出</span><textarea value={task.expectedOutput} disabled={disabled} onInput={(event) => replaceTask(index, { expectedOutput: event.currentTarget.value })} /></label>
        <div className="ai-review-project-draft-clusters">
          {task.clusterIds.map((clusterId) => <label key={clusterId}>
            <span>{clusterId.slice(0, 18)}…</span>
            <select value={index} disabled={disabled} onChange={(event) => moveCluster(clusterId, Number(event.currentTarget.value))}>
              {draft.tasks.map((candidate, taskIndex) => <option value={taskIndex} key={candidate.key}>{candidate.title || `任务 ${taskIndex + 1}`}</option>)}
            </select>
          </label>)}
        </div>
        <button type="button" disabled={disabled || draft.tasks.length <= 1} onClick={() => removeTask(index)}>合并到其他任务</button>
      </fieldset>)}
    </div>
  </div>;
}

export function AiReviewInspector({
  record,
  pending,
  onAccept,
  onChange,
  onIgnore,
  onRevert,
  onRetry,
}: {
  record: AiReviewRecord | null;
  pending: boolean;
  onAccept: (record: AiReviewRecord) => void;
  onChange: (record: AiReviewRecord, changedJson: string) => void;
  onIgnore: (record: AiReviewRecord) => void;
  onRevert: (record: AiReviewRecord) => void;
  onRetry: (record: AiReviewRecord, useCurrentMode: boolean) => void;
}) {
  const candidates = record ? reviewCandidates(record) : [];
  const [selectedCandidate, setSelectedCandidate] = useState(0);
  const [changedJson, setChangedJson] = useState("");

  useEffect(() => {
    setSelectedCandidate(0);
    setChangedJson(candidates[0]?.changedJson ?? record?.proposedJson ?? "");
  }, [record?.id]);

  if (!record) return <aside className="ai-review-inspector empty" aria-label="AI 审核详情">
    <Bot size={24} />
    <strong>选择一条审核记录</strong>
    <span>证据、候选值和可执行操作将在这里显示。</span>
  </aside>;

  const diagnostic = sanitizeReviewDiagnostic(record.execution.diagnostic);
  const changedJsonValidation = validateChangedJson(record, changedJson);
  const StateIcon = record.state === "execution_error"
    ? AlertTriangle
    : record.state === "manual_override"
      ? UserRoundPen
      : record.state === "dismissed"
        ? X
        : record.state === "reverted"
          ? RotateCcw
          : CheckCircle2;
  return <aside className="ai-review-inspector" aria-label="AI 审核详情">
    <header className="ai-review-inspector-header">
      <div className="ai-review-inspector-title">
        <span>{record.kind === "classification" ? <Bot size={16} /> : <GitBranch size={16} />}{record.kind === "classification" ? "活动分类" : "工作流分配"}</span>
        <h2>{record.evidenceSummary || record.subjectId}</h2>
      </div>
      <span className={`ai-review-state-badge state-${record.state}`}><StateIcon size={14} />{stateLabels[record.state]}</span>
    </header>

    <div className="ai-review-inspector-scroll">
      <section className="ai-review-detail-section">
        <h3>证据</h3>
        <p>{record.evidenceSummary || "没有可显示的证据摘要"}</p>
        <dl className="ai-review-meta-grid">
          <div><dt><Fingerprint size={13} />证据哈希</dt><dd>{record.evidenceHash}</dd></div>
          <div><dt>{record.execution.executionMode === null ? <UserRoundPen size={13} /> : <Bot size={13} />}来源</dt><dd>{reviewSourceLabel(record)}</dd></div>
          <div><dt><CheckCircle2 size={13} />置信度</dt><dd>{record.confidence === null ? "无评分" : `${Math.round(record.confidence * 100)}%`}</dd></div>
          <div><dt>生成批次</dt><dd>#{record.execution.generation}</dd></div>
        </dl>
      </section>

      {record.state === "execution_error" && <section className="ai-review-detail-section ai-review-diagnostic" role="status">
        <h3><ShieldAlert size={15} />清洗后的失败诊断</h3>
        <p>{diagnostic || "执行失败，后端未提供诊断。"}</p>
        <dl className="ai-review-error-meta">
          <div><dt>错误类型</dt><dd>{record.execution.errorKind ?? "unknown"}</dd></div>
          <div><dt>退出码</dt><dd>{record.execution.exitCode ?? "-"}</dd></div>
        </dl>
      </section>}

      {candidates.length > 0 && <section className="ai-review-detail-section">
        <h3>候选值与评分</h3>
        <div className="ai-review-candidates" role="radiogroup" aria-label="选择替代候选">
          {candidates.map((candidate, index) => <label key={`${candidate.label}-${index}`}>
            <input
              type="radio"
              name={`candidate-${record.id}`}
              checked={selectedCandidate === index}
              onChange={() => {
                setSelectedCandidate(index);
                setChangedJson(candidate.changedJson);
              }}
            />
            <span><strong>{candidate.label}</strong><small>{candidate.score === null ? "无评分" : `${Math.round(candidate.score * 100)}%`}</small></span>
          </label>)}
        </div>
      </section>}

      {record.state === "pending" && <section className="ai-review-detail-section">
        {record.kind === "project_draft" && <ProjectDraftEditor value={changedJson} disabled={pending} onChange={setChangedJson} />}
        {record.kind !== "project_draft" && <label className="ai-review-json-editor">
          <span>替代 JSON</span>
          <textarea aria-label="替代 JSON" value={changedJson} onInput={(event) => setChangedJson(event.currentTarget.value)} spellCheck={false} />
        </label>}
        {!changedJsonValidation.ok && <p className="ai-review-json-error" role="alert">{changedJsonValidation.reason}</p>}
      </section>}

      <section className="ai-review-detail-section ai-review-json-comparison">
        <h3>值对比</h3>
        <JsonView label="修改前" value={record.beforeJson} />
        <JsonView label="AI 建议" value={record.proposedJson} />
        <JsonView label="已应用" value={record.appliedJson} />
      </section>
    </div>

    <footer className="ai-review-actions">
      {record.state === "pending" && <>
        <button type="button" className="primary" aria-label="接受建议" disabled={pending} onClick={() => onAccept(record)}><Check size={15} />接受</button>
        <button
          type="button"
          aria-label="应用替代值"
          disabled={pending || !changedJsonValidation.ok}
          onClick={() => {
            if (changedJsonValidation.ok) onChange(record, changedJsonValidation.changedJson);
          }}
        ><FileJson2 size={15} />应用替代值</button>
        <button type="button" aria-label="忽略建议" disabled={pending} onClick={() => onIgnore(record)}><X size={15} />忽略</button>
      </>}
      {record.state === "auto_applied" && <button type="button" className="danger" aria-label="撤销自动应用" disabled={pending} onClick={() => onRevert(record)}><RotateCcw size={15} />撤销自动应用</button>}
      {record.state === "execution_error" && <>
        <button type="button" aria-label="按原模式重试" disabled={pending} onClick={() => onRetry(record, false)}><RotateCcw size={15} />按原模式重试</button>
        <button type="button" className="primary" aria-label="按当前模式重试" disabled={pending} onClick={() => onRetry(record, true)}><RotateCcw size={15} />按当前模式重试</button>
      </>}
    </footer>
  </aside>;
}
