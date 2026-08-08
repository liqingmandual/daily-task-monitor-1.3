import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, CheckSquare2, LoaderCircle, RefreshCw, RotateCcw } from "lucide-react";
import {
  bulkRetryAiJobsWithCurrentMode,
  isDesktopRuntime,
  listPendingAiJobs,
  type AiJobStatus,
  type AiQueueRecord,
} from "../../lib/desktop";

export interface AiExecutionQueueActions {
  list: (request: { status?: AiJobStatus | null; limit?: number }) => Promise<AiQueueRecord[]>;
  retry: (jobIds: string[]) => Promise<AiQueueRecord[]>;
}

const defaultActions: AiExecutionQueueActions = {
  list: listPendingAiJobs,
  retry: bulkRetryAiJobsWithCurrentMode,
};

const kindLabels: Record<string, string> = {
  daily_analysis: "每日分析",
  trend_analysis: "趋势分析",
  trend_research_analysis: "趋势研究",
  classify_segment: "任务归类",
  classify_page: "页面归类",
  work_ledger_assignment: "工作流建议",
};

const statusLabels: Record<AiJobStatus, string> = {
  "awaiting-reassignment": "等待改派",
  pending: "待执行",
  running: "执行中",
  complete: "已完成",
};

function previewQueue(): AiQueueRecord[] {
  return [{
    id: "preview-legacy-job",
    generation: 0,
    kind: "classify_segment",
    status: "awaiting-reassignment",
    attempts: 3,
    nextAttemptAtMs: Date.now() + 60_000,
    lastError: "ModelScope returned HTTP 429",
    execution: {
      executionMode: "api-key",
      executorId: "legacy-provider-registry",
      model: "",
      evidenceHash: "preview-evidence",
      createdAtMs: Date.now() - 3_600_000,
    },
    startedAtMs: null,
    finishedAtMs: null,
    durationMs: null,
    executorId: null,
    model: null,
    exitCode: null,
    errorKind: "provider",
  }];
}

function diagnostic(value: string): string {
  if (!value) return "—";
  const lower = value.toLowerCase();
  if (/https?:\/\//i.test(value) || /authorization|bearer|api[_ -]?key|[a-z]:\\/i.test(value)) {
    return "诊断信息已脱敏";
  }
  return lower.length > 180 ? `${value.slice(0, 177)}...` : value;
}

function executionLabel(job: AiQueueRecord): string {
  const mode = job.execution.executionMode === "codex" ? "Codex" : "API";
  const executor = job.execution.executorId === "legacy-provider-registry"
    ? "旧 Provider 队列"
    : job.execution.executorId || "未配置";
  return `${mode} · ${executor}${job.execution.model ? ` / ${job.execution.model}` : ""}`;
}

function dateTime(value: number | null): string {
  if (!value) return "—";
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

export function AiExecutionQueue({ actions = defaultActions }: { actions?: AiExecutionQueueActions }) {
  const preview = actions === defaultActions && !isDesktopRuntime();
  const [status, setStatus] = useState<AiJobStatus>("awaiting-reassignment");
  const [records, setRecords] = useState<AiQueueRecord[]>(() => preview ? previewQueue() : []);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(!preview);
  const [retrying, setRetrying] = useState(false);
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState("");
  const requestIdRef = useRef(0);
  const mountedRef = useRef(true);
  const statusRef = useRef(status);

  const refresh = useCallback(async (requestedStatus?: AiJobStatus) => {
    if (preview) return;
    const statusToLoad = requestedStatus ?? statusRef.current;
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError("");
    try {
      const next = await actions.list({ status: statusToLoad, limit: 500 });
      if (!mountedRef.current || requestId !== requestIdRef.current) return;
      setRecords(next);
      const validIds = new Set(next.filter((record) => record.status === "awaiting-reassignment").map((record) => record.id));
      setSelectedIds((current) => new Set([...current].filter((id) => validIds.has(id))));
    } catch (reason) {
      if (!mountedRef.current || requestId !== requestIdRef.current) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (mountedRef.current && requestId === requestIdRef.current) setLoading(false);
    }
  }, [actions, preview]);

  useEffect(() => {
    mountedRef.current = true;
    void refresh();
    const interval = window.setInterval(() => void refresh(), 10_000);
    return () => {
      mountedRef.current = false;
      requestIdRef.current += 1;
      window.clearInterval(interval);
    };
  }, [refresh]);

  const selectable = useMemo(
    () => records.filter((record) => record.status === "awaiting-reassignment"),
    [records],
  );
  const allSelected = selectable.length > 0 && selectable.every((record) => selectedIds.has(record.id));

  async function reassignSelected() {
    const ids = records
      .filter((record) => selectedIds.has(record.id) && record.status === "awaiting-reassignment")
      .map((record) => record.id);
    if (!ids.length || ids.length > 500) return;
    if (!window.confirm(`将 ${ids.length} 个暂停任务按当前 AI 方式重新排队。继续吗？`)) return;
    setRetrying(true);
    setError("");
    setFeedback("");
    try {
      if (preview) {
        setRecords([]);
      } else {
        const created = await actions.retry(ids);
        if (!mountedRef.current) return;
        setFeedback(`已创建 ${created.length} 个新一代任务，旧代仅保留审计记录`);
        await refresh();
      }
      if (!mountedRef.current) return;
      setSelectedIds(new Set());
    } catch (reason) {
      if (!mountedRef.current) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (mountedRef.current) setRetrying(false);
    }
  }

  return <div className="ai-execution-queue">
    <div className="ai-execution-queue-toolbar">
      <label><span>任务状态</span><select value={status} disabled={retrying} onChange={(event) => {
        const next = event.target.value as AiJobStatus;
        statusRef.current = next;
        setStatus(next);
        void refresh(next);
      }}>
        <option value="awaiting-reassignment">等待改派</option>
        <option value="pending">待执行</option>
        <option value="running">执行中</option>
        <option value="complete">已完成</option>
      </select></label>
      <span className="ai-execution-queue-count">{records.length} 项</span>
      <button type="button" className="ai-review-refresh" onClick={() => void refresh()} disabled={loading || retrying}>
        <RefreshCw size={15} />刷新
      </button>
    </div>

    {(error || feedback) && <p className={`ai-review-feedback ${error ? "error" : ""}`} role={error ? "alert" : "status"}>{error || feedback}</p>}

    <div className="ai-execution-queue-batchbar">
      <span><CheckSquare2 size={15} />已选 {selectedIds.size} 项，最多 500 项</span>
      <button type="button" onClick={() => void reassignSelected()} disabled={!selectedIds.size || retrying}>
        {retrying ? <LoaderCircle size={15} className="spin" /> : <RotateCcw size={15} />}
        按当前方式重新排队
      </button>
    </div>

    <div className="ai-execution-queue-table-wrap">
      <table className="ai-execution-queue-table">
        <thead><tr>
          <th><input type="checkbox" aria-label="选择全部等待改派任务" checked={allSelected} disabled={!selectable.length || retrying} onChange={() => setSelectedIds(allSelected ? new Set() : new Set(selectable.map((record) => record.id)))} /></th>
          <th>任务类型</th><th>状态</th><th>执行来源</th><th>尝试</th><th>下次执行</th><th>诊断</th>
        </tr></thead>
        <tbody>
          {records.map((record) => <tr key={record.id}>
            <td><input type="checkbox" aria-label={`选择任务 ${record.id}`} checked={selectedIds.has(record.id)} disabled={record.status !== "awaiting-reassignment" || retrying} onChange={() => setSelectedIds((current) => { const next = new Set(current); next.has(record.id) ? next.delete(record.id) : next.add(record.id); return next; })} /></td>
            <td><b>{kindLabels[record.kind] ?? record.kind}</b><small>第 {record.generation + 1} 代</small></td>
            <td><span className={`ai-queue-status ${record.status}`}>{statusLabels[record.status]}</span></td>
            <td>{executionLabel(record)}</td>
            <td>{record.attempts}</td>
            <td>{dateTime(record.nextAttemptAtMs)}</td>
            <td className="ai-queue-diagnostic" title={diagnostic(record.lastError)}>{record.lastError ? <><AlertTriangle size={14} />{diagnostic(record.lastError)}</> : "—"}</td>
          </tr>)}
          {!records.length && !loading && <tr><td colSpan={7} className="ai-queue-empty">当前状态下没有任务</td></tr>}
          {loading && <tr><td colSpan={7} className="ai-queue-empty"><LoaderCircle size={18} className="spin" />正在读取执行队列</td></tr>}
        </tbody>
      </table>
    </div>
  </div>;
}
