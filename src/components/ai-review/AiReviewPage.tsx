import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  Filter,
  GitBranch,
  Layers3,
  ListChecks,
  LoaderCircle,
  RotateCcw,
  UserRoundPen,
} from "lucide-react";
import {
  isDesktopRuntime,
  listenWorkflowChanged,
  listAiReviews,
  resolveAiReview,
  retryAiReview,
  revertAiAutoApply,
  type AiReviewFilter,
  type AiReviewRecord,
  type AiReviewResolution,
} from "../../lib/desktop";
import {
  applyAiReviewFilters,
  buildAiReviewFilter,
  buildAiReviewResolution,
  createLatestRequestGuard,
  defaultAiReviewFilters,
  reviewStatesByTab,
  validateBatchAccept,
  validateChangedJson,
  type AiReviewTab,
  type AiReviewUiFilters,
} from "../../lib/ai-review";
import { AiReviewInspector } from "./AiReviewInspector";
import { AiReviewTable } from "./AiReviewTable";
import { AiExecutionQueue } from "./AiExecutionQueue";

export interface AiReviewActions {
  list: (filter: AiReviewFilter) => Promise<AiReviewRecord[]>;
  resolve: (request: AiReviewResolution) => Promise<AiReviewRecord[]>;
  revert: (reviewId: string) => Promise<AiReviewRecord>;
  retry: (reviewId: string, useCurrentMode: boolean) => Promise<boolean>;
}

const defaultActions: AiReviewActions = {
  list: listAiReviews,
  resolve: resolveAiReview,
  revert: revertAiAutoApply,
  retry: retryAiReview,
};

type AiReviewWorkspaceTab = AiReviewTab | "execution-queue";

const tabs: Array<{ id: AiReviewWorkspaceTab; label: string; icon: typeof Bot }> = [
  { id: "pending", label: "待审核", icon: Bot },
  { id: "auto-applied", label: "已自动应用", icon: CheckCircle2 },
  { id: "manual-override", label: "人工改写", icon: UserRoundPen },
  { id: "execution-error", label: "执行失败", icon: AlertTriangle },
  { id: "execution-queue", label: "执行队列", icon: ListChecks },
];

function previewExecution(mode: "api-key" | "codex" | null, hash: string, diagnostic = ""): AiReviewRecord["execution"] {
  return {
    executionMode: mode,
    executorId: mode === "api-key" ? "openai" : mode === "codex" ? "codex-cli" : null,
    model: mode === "api-key" ? "gpt-4.1-mini" : mode === "codex" ? "gpt-5-codex" : null,
    evidenceHash: hash,
    generation: 1,
    createdAtMs: 1_784_080_800_000,
    startedAtMs: mode ? 1_784_080_800_100 : null,
    finishedAtMs: mode ? 1_784_080_801_280 : null,
    durationMs: mode ? 1_180 : null,
    exitCode: diagnostic ? 1 : mode ? 0 : null,
    errorKind: diagnostic ? "provider" : null,
    diagnostic,
  };
}

export function previewAiReviewRecords(): AiReviewRecord[] {
  const proposed = JSON.stringify({
    category: "research",
    videoPurpose: "unknown",
    confidence: .78,
    reason: "编辑器标题与研究任务匹配",
    modelVersion: "preview-model",
    candidates: [
      { label: "搜索/调研", score: .78, value: { category: "research", videoPurpose: "unknown", confidence: .78, reason: "编辑器标题与研究任务匹配", modelVersion: "preview-model" } },
      { label: "创作开发", score: .17, value: { category: "creation_development", videoPurpose: "unknown", confidence: .17, reason: "存在代码编辑信号", modelVersion: "preview-model" } },
    ],
  });
  const base: AiReviewRecord = {
    id: "preview-pending-classification",
    kind: "classification",
    state: "pending",
    subjectId: "2",
    beforeJson: JSON.stringify({ category: "pending", confidence: 0 }),
    proposedJson: proposed,
    appliedJson: null,
    confidence: .78,
    evidenceSummary: "Visual Studio Code · AiReviewPage.tsx · 42 分钟",
    evidenceHash: "sha256:preview-classification-7f4e",
    execution: previewExecution("api-key", "sha256:preview-classification-7f4e"),
    createdAtMs: 1_784_080_800_000,
    resolvedAtMs: null,
  };
  return [
    base,
    { ...base, id: "preview-pending-workflow", kind: "workflow_assignment", subjectId: "preview-browser", confidence: .69, evidenceSummary: "react.dev · React 状态管理文档", evidenceHash: "sha256:preview-workflow-a221", execution: previewExecution("codex", "sha256:preview-workflow-a221"), proposedJson: JSON.stringify({ evidenceKind: "browser", evidenceId: "preview-browser", taskId: "preview-task", confidence: .69, reason: "页面标题与任务内容匹配", createdAtMs: 1_784_080_800_000 }) },
    { ...base, id: "preview-auto", state: "auto_applied", confidence: .94, appliedJson: proposed, evidenceHash: "sha256:preview-auto-94aa", execution: previewExecution("codex", "sha256:preview-auto-94aa") },
    { ...base, id: "preview-manual", state: "manual_override", confidence: 1, appliedJson: JSON.stringify({ category: "creation_development" }), evidenceSummary: "用户将活动改为创作开发", evidenceHash: "sha256:preview-manual-18cd", execution: previewExecution(null, "sha256:preview-manual-18cd") },
    { ...base, id: "preview-dismissed", state: "dismissed", confidence: .52, evidenceSummary: "用户忽略了低置信度分类建议", evidenceHash: "sha256:preview-dismissed-52aa", execution: previewExecution("api-key", "sha256:preview-dismissed-52aa"), resolvedAtMs: 1_784_080_900_000 },
    { ...base, id: "preview-reverted", state: "reverted", confidence: .91, appliedJson: proposed, evidenceSummary: "用户撤销了自动应用结果", evidenceHash: "sha256:preview-reverted-91aa", execution: previewExecution("codex", "sha256:preview-reverted-91aa"), resolvedAtMs: 1_784_080_900_000 },
    { ...base, id: "preview-error", state: "execution_error", confidence: null, proposedJson: "", evidenceSummary: "浏览器证据的工作流建议", evidenceHash: "sha256:preview-error-3b29", execution: previewExecution("api-key", "sha256:preview-error-3b29", "Provider timed out; Authorization: Bearer hidden https://api.example.test stderr: private output") },
  ];
}

function nextTab(current: AiReviewWorkspaceTab, key: string): AiReviewWorkspaceTab | null {
  const index = tabs.findIndex((tab) => tab.id === current);
  if (key === "ArrowRight") return tabs[(index + 1) % tabs.length].id;
  if (key === "ArrowLeft") return tabs[(index - 1 + tabs.length) % tabs.length].id;
  if (key === "Home") return tabs[0].id;
  if (key === "End") return tabs.at(-1)?.id ?? null;
  return null;
}

export function AiReviewPage({
  subjectId,
  actions = defaultActions,
  onResolved,
}: {
  subjectId?: string | null;
  actions?: AiReviewActions;
  onResolved?: (record: AiReviewRecord) => void;
}) {
  const localPreview = actions === defaultActions && !isDesktopRuntime();
  const [tab, setTab] = useState<AiReviewWorkspaceTab>("pending");
  const [filters, setFilters] = useState<AiReviewUiFilters>(defaultAiReviewFilters);
  const [records, setRecords] = useState<AiReviewRecord[]>(() => localPreview ? previewAiReviewRecords() : []);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [pendingIds, setPendingIds] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(!localPreview);
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState("");
  const requestGuardRef = useRef(createLatestRequestGuard());
  const mountedRef = useRef(false);

  useEffect(() => {
    mountedRef.current = true;
    const guard = createLatestRequestGuard();
    requestGuardRef.current = guard;
    return () => {
      mountedRef.current = false;
      guard.dispose();
    };
  }, []);

  const refresh = useCallback(async () => {
    if (tab === "execution-queue") {
      setLoading(false);
      return;
    }
    if (localPreview || !mountedRef.current) return;
    const guard = requestGuardRef.current;
    const requestId = guard.next();
    if (guard.isCurrent(requestId)) {
      setLoading(true);
      setError("");
    }
    try {
      const nextRecords = await actions.list(buildAiReviewFilter(tab, filters, subjectId));
      if (guard.isCurrent(requestId)) setRecords(nextRecords);
    } catch (reason) {
      if (guard.isCurrent(requestId)) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (guard.isCurrent(requestId)) setLoading(false);
    }
  }, [actions, filters, localPreview, subjectId, tab]);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (localPreview) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenWorkflowChanged(() => {
      if (!disposed) void refresh();
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [localPreview, refresh]);

  const tabRecords = useMemo(
    () => tab === "execution-queue"
      ? []
      : records.filter((record) => reviewStatesByTab[tab].includes(record.state)),
    [records, tab],
  );
  const visibleRecords = useMemo(
    () => applyAiReviewFilters(tabRecords, filters, subjectId),
    [filters, subjectId, tabRecords],
  );
  const activeRecord = visibleRecords.find((record) => record.id === activeId) ?? visibleRecords[0] ?? null;

  useEffect(() => {
    if (activeRecord && activeRecord.id !== activeId) setActiveId(activeRecord.id);
    if (!activeRecord && activeId !== null) setActiveId(null);
  }, [activeId, activeRecord]);

  useEffect(() => {
    const visibleIds = new Set(visibleRecords.map((record) => record.id));
    setSelectedIds((current) => {
      const next = new Set([...current].filter((reviewId) => visibleIds.has(reviewId)));
      return next.size === current.size ? current : next;
    });
  }, [visibleRecords]);

  function patchFilter<Key extends keyof AiReviewUiFilters>(key: Key, value: AiReviewUiFilters[Key]) {
    setFilters((current) => ({ ...current, [key]: value }));
  }

  function markPending(ids: string[], pending: boolean) {
    if (!mountedRef.current) return;
    setPendingIds((current) => {
      const next = new Set(current);
      ids.forEach((id) => pending ? next.add(id) : next.delete(id));
      return next;
    });
  }

  async function resolve(recordsToResolve: AiReviewRecord[], action: "accept" | "change" | "ignore", changedJson?: string) {
    if (action === "change") {
      if (recordsToResolve.length !== 1) {
        setFeedback("");
        setError("替代值只能应用到单条审核记录");
        return;
      }
      const validation = validateChangedJson(recordsToResolve[0], changedJson ?? "");
      if (!validation.ok) {
        setFeedback("");
        setError(validation.reason);
        return;
      }
      changedJson = validation.changedJson;
    }
    const ids = recordsToResolve.map((record) => record.id);
    markPending(ids, true);
    setError("");
    setFeedback("");
    try {
      let resolved: AiReviewRecord[];
      if (localPreview) {
        const resolvedAtMs = Date.now();
        resolved = recordsToResolve.map((record) => ({
          ...record,
          state: action === "ignore" ? "dismissed" as const : "manual_override" as const,
          appliedJson: action === "change" ? changedJson ?? null : action === "accept" ? record.proposedJson : record.appliedJson,
          resolvedAtMs,
        }));
        setRecords((current) => current.map((record) => resolved.find((item) => item.id === record.id) ?? record));
      } else {
        resolved = await actions.resolve(buildAiReviewResolution(recordsToResolve, action, changedJson));
        if (!mountedRef.current) return;
        await refresh();
      }
      if (!mountedRef.current) return;
      setSelectedIds(new Set());
      resolved.forEach((record) => onResolved?.(record));
      setFeedback(action === "ignore" ? "审核记录已忽略" : `已处理 ${recordsToResolve.length} 条审核记录`);
    } catch (reason) {
      if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (mountedRef.current) markPending(ids, false);
    }
  }

  async function revert(record: AiReviewRecord) {
    if (!window.confirm("撤销后将恢复自动应用前的值。确认继续吗？")) return;
    markPending([record.id], true);
    setError("");
    try {
      const reverted = localPreview ? { ...record, state: "reverted" as const, resolvedAtMs: Date.now() } : await actions.revert(record.id);
      if (!mountedRef.current) return;
      if (localPreview) setRecords((current) => current.map((item) => item.id === record.id ? reverted : item));
      else await refresh();
      if (!mountedRef.current) return;
      onResolved?.(reverted);
      setFeedback("自动应用已撤销");
    } catch (reason) {
      if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (mountedRef.current) markPending([record.id], false);
    }
  }

  async function retry(record: AiReviewRecord, useCurrentMode: boolean) {
    markPending([record.id], true);
    setError("");
    setFeedback("");
    try {
      let created = true;
      if (localPreview) {
        setRecords((current) => current.map((item) => item.id === record.id ? { ...item, state: "pending", execution: { ...item.execution, diagnostic: "", errorKind: null, exitCode: null } } : item));
      } else {
        created = await actions.retry(record.id, useCurrentMode);
        if (!mountedRef.current) return;
        if (!created) {
          setFeedback("未创建新任务/无需重复入队");
          return;
        }
        await refresh();
      }
      if (!mountedRef.current) return;
      onResolved?.(record);
      setFeedback(useCurrentMode ? "已按当前模式重新入队" : "已按原模式重新入队");
    } catch (reason) {
      if (mountedRef.current) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (mountedRef.current) markPending([record.id], false);
    }
  }

  function batchAccept() {
    const selected = visibleRecords.filter((record) => selectedIds.has(record.id));
    const validation = validateBatchAccept(selected);
    if (!validation.ok) {
      setFeedback("");
      setError(validation.reason);
      return;
    }
    void resolve(selected, "accept");
  }

  return <section className="ai-review-page">
    <header className="ai-review-heading">
      <div><span>AI REVIEW</span><h1>AI 审核中心</h1></div>
      <p>{subjectId ? `聚焦对象 ${subjectId}` : "集中处理分类、工作流建议与执行异常"}</p>
    </header>

    <div className="ai-review-toolbar">
      <div className="ai-review-tabs" role="tablist" aria-label="AI 审核状态">
        {tabs.map((item) => {
          const Icon = item.icon;
          return <button
            type="button"
            role="tab"
            id={`ai-review-${item.id}-tab`}
            aria-controls="ai-review-workbench"
            aria-selected={tab === item.id}
            tabIndex={tab === item.id ? 0 : -1}
            data-review-tab={item.id}
            key={item.id}
            onClick={() => { setTab(item.id); setSelectedIds(new Set()); }}
            onKeyDown={(event) => {
              const next = nextTab(item.id, event.key);
              if (!next) return;
              event.preventDefault();
              setTab(next);
              setSelectedIds(new Set());
              requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(`[data-review-tab="${next}"]`)?.focus());
            }}
          ><Icon size={15} />{item.label}</button>;
        })}
      </div>
      {tab !== "execution-queue" && <button type="button" className="ai-review-refresh" aria-label="刷新审核记录" disabled={loading} onClick={() => void refresh()}><RotateCcw size={15} />刷新</button>}
    </div>

    {tab !== "execution-queue" && <div className="ai-review-filters" aria-label="组合筛选">
      <Filter size={16} aria-hidden="true" />
      <label><span>类型</span><select aria-label="审核类型" value={filters.kind} onChange={(event) => patchFilter("kind", event.target.value as AiReviewUiFilters["kind"])}><option value="all">全部</option><option value="classification">活动分类</option><option value="workflow_assignment">工作流分配</option><option value="project_draft">项目草稿</option></select></label>
      <label><span>执行方式</span><select aria-label="执行方式" value={filters.executionMode} onChange={(event) => patchFilter("executionMode", event.target.value as AiReviewUiFilters["executionMode"])}><option value="all">全部</option><option value="api-key">API Key</option><option value="codex">本机 Codex</option><option value="manual">人工修改</option></select></label>
      <label><span>执行者</span><input aria-label="执行者" value={filters.executorId} onChange={(event) => patchFilter("executorId", event.target.value)} placeholder="provider" /></label>
      <label><span>模型</span><input aria-label="模型" value={filters.model} onChange={(event) => patchFilter("model", event.target.value)} placeholder="model" /></label>
      <label><span>开始</span><input type="date" aria-label="开始日期" value={filters.dateFrom} onChange={(event) => patchFilter("dateFrom", event.target.value)} /></label>
      <label><span>结束</span><input type="date" aria-label="结束日期" value={filters.dateTo} onChange={(event) => patchFilter("dateTo", event.target.value)} /></label>
      <label className="confidence-filter"><span>置信度</span><span><input type="number" min="0" max="100" aria-label="最低置信度" value={filters.confidenceMin} onChange={(event) => patchFilter("confidenceMin", Number(event.target.value))} /><i>-</i><input type="number" min="0" max="100" aria-label="最高置信度" value={filters.confidenceMax} onChange={(event) => patchFilter("confidenceMax", Number(event.target.value))} /><i>%</i></span></label>
    </div>}

    {(error || feedback) && <p className={`ai-review-feedback ${error ? "error" : ""}`} role={error ? "alert" : "status"}>{error || feedback}</p>}

    {tab !== "execution-queue" && <div className="ai-review-batchbar">
      <span><Layers3 size={14} />已选择 {selectedIds.size} 项</span>
      <button type="button" aria-label="批量接受同类" disabled={!selectedIds.size || pendingIds.size > 0} onClick={batchAccept}><CheckCircle2 size={14} />批量接受同类</button>
    </div>}

    <div id="ai-review-workbench" className="ai-review-workbench" role="tabpanel" aria-labelledby={`ai-review-${tab}-tab`}>
      {tab === "execution-queue" ? <AiExecutionQueue /> : loading && !records.length ? <div className="ai-review-loading" role="status"><LoaderCircle size={22} className="spin" />正在读取审核记录</div> : <>
        <AiReviewTable
          records={visibleRecords}
          activeId={activeRecord?.id ?? null}
          selectedIds={selectedIds}
          pendingIds={pendingIds}
          onOpen={(record) => setActiveId(record.id)}
          onToggle={(reviewId) => setSelectedIds((current) => { const next = new Set(current); if (next.has(reviewId)) next.delete(reviewId); else next.add(reviewId); return next; })}
        />
        <AiReviewInspector
          record={activeRecord}
          pending={activeRecord ? pendingIds.has(activeRecord.id) : false}
          onAccept={(record) => void resolve([record], "accept")}
          onChange={(record, changedJson) => void resolve([record], "change", changedJson)}
          onIgnore={(record) => void resolve([record], "ignore")}
          onRevert={(record) => void revert(record)}
          onRetry={(record, useCurrentMode) => void retry(record, useCurrentMode)}
        />
      </>}
    </div>
  </section>;
}
