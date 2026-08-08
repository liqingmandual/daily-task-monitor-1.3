import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, RefObject } from "react";
import { Archive, Check, ClipboardList, Download, Pencil, Plus, Sparkles, X } from "lucide-react";
import {
  addWorkLedgerProgress,
  applyWorkLedgerSuggestion,
  archiveWorkLedgerProject,
  assignWorkLedgerEvidence,
  cancelWorkLedgerAiTask,
  dayBounds,
  getAppSettings,
  isDesktopRuntime,
  listenWorkflowChanged,
  loadWorkLedger,
  loadWorkLedgerProjectInsight,
  loadWorkLedgerTaskInsight,
  mergeWorkLedgerTasks,
  removeWorkLedgerEvidence,
  runWorkflowAiSuggestions,
  saveWorkLedgerProject,
  saveWorkLedgerTask,
  exportProjectReport,
  exportTaskReport,
  updateWorkLedgerTaskStatus,
  type WorkLedgerEvidence,
  type AiTaskCancellation,
  type AiExecutionMode,
  type WorkLedgerEvidenceAssignmentRequest,
  type WorkLedgerProgressEntry,
  type WorkLedgerProject,
  type WorkLedgerProjectSaveRequest,
  type WorkLedgerSnapshot,
  type WorkLedgerSuggestedAssignmentRequest,
  type WorkLedgerTask,
  type WorkLedgerTaskSaveRequest,
  type WorkLedgerTaskStatus,
  type WorkflowAnalysisQueueResult,
  type ReportFormat,
  type ProjectTimeInsight,
  type TaskTimeInsight,
} from "../../lib/desktop";
import { EvidenceAssignmentPanel } from "./EvidenceAssignmentPanel";
import { ProgressTimeline } from "./ProgressTimeline";
import { ProjectDashboard } from "./ProjectDashboard";
import { TaskEditor } from "./TaskEditor";
import { TaskOverview } from "./TaskOverview";

export interface WorkflowActions {
  load: typeof loadWorkLedger;
  saveProject: (request: WorkLedgerProjectSaveRequest) => Promise<WorkLedgerProject>;
  archiveProject: (projectId: string) => Promise<boolean>;
  saveTask: (request: WorkLedgerTaskSaveRequest) => Promise<WorkLedgerTask>;
  updateTaskStatus: (taskId: string, status: WorkLedgerTaskStatus) => Promise<WorkLedgerTask>;
  cancelAiTask?: (taskId: string) => Promise<AiTaskCancellation>;
  addProgress: (taskId: string, note: string) => Promise<WorkLedgerProgressEntry>;
  assignEvidence: (request: WorkLedgerEvidenceAssignmentRequest) => Promise<boolean>;
  removeEvidence: (request: WorkLedgerEvidenceAssignmentRequest) => Promise<boolean>;
  applySuggestion: (request: WorkLedgerSuggestedAssignmentRequest) => Promise<boolean>;
  runAiSuggestions?: (startMs: number, endMs: number) => Promise<WorkflowAnalysisQueueResult>;
  getAiAutomationState?: () => Promise<{ enabled: boolean; executionMode: AiExecutionMode }>;
  loadInsight?: (taskId: string, endMs: number) => Promise<TaskTimeInsight>;
  loadProjectInsight?: (projectId: string, endMs: number, rangeDays: number) => Promise<ProjectTimeInsight>;
  mergeTasks?: (sourceTaskId: string, targetTaskId: string) => Promise<boolean>;
  exportTask?: (taskId: string, endMs: number, format: ReportFormat) => Promise<string | null>;
  exportProject?: (projectId: string, endMs: number, format: ReportFormat) => Promise<string | null>;
}

const defaultActions: WorkflowActions = {
  load: loadWorkLedger,
  saveProject: saveWorkLedgerProject,
  archiveProject: archiveWorkLedgerProject,
  saveTask: saveWorkLedgerTask,
  updateTaskStatus: updateWorkLedgerTaskStatus,
  cancelAiTask: cancelWorkLedgerAiTask,
  addProgress: addWorkLedgerProgress,
  assignEvidence: assignWorkLedgerEvidence,
  removeEvidence: removeWorkLedgerEvidence,
  applySuggestion: applyWorkLedgerSuggestion,
  runAiSuggestions: runWorkflowAiSuggestions,
  loadInsight: loadWorkLedgerTaskInsight,
  loadProjectInsight: loadWorkLedgerProjectInsight,
  mergeTasks: mergeWorkLedgerTasks,
  exportTask: exportTaskReport,
  exportProject: exportProjectReport,
  getAiAutomationState: async () => {
    const settings = await getAppSettings();
    return {
      enabled: settings.aiAutoWorkflowAssignmentEnabled,
      executionMode: settings.aiExecutionMode,
    };
  },
};

const emptyReviewSubjectIds: ReadonlySet<string> = new Set();

type InspectorTab = "overview" | "progress" | "evidence";
const inspectorTabs: InspectorTab[] = ["overview", "progress", "evidence"];

function nextInspectorTab(current: InspectorTab, key: string): InspectorTab | null {
  const index = inspectorTabs.indexOf(current);
  if (key === "ArrowRight") return inspectorTabs[(index + 1) % inspectorTabs.length];
  if (key === "ArrowLeft") return inspectorTabs[(index - 1 + inspectorTabs.length) % inspectorTabs.length];
  if (key === "Home") return inspectorTabs[0];
  if (key === "End") return inspectorTabs.at(-1) ?? null;
  return null;
}

function trapDialogFocus(event: ReactKeyboardEvent<HTMLElement>, dialog: HTMLElement | null, onClose: () => void) {
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

function previewSnapshot(selectedDate: string): WorkLedgerSnapshot {
  const { startMs } = dayBounds(selectedDate);
  const project: WorkLedgerProject = { id: "preview-project", name: "学习雅思", color: "#2563eb", status: "active", description: "准备雅思考试并积累核心词汇", createdAtMs: startMs - 29 * 86_400_000, updatedAtMs: startMs + 43_200_000, archivedAtMs: null };
  const task: WorkLedgerTask = { id: "preview-research", projectId: project.id, title: "资料检索", status: "in_progress", priority: "high", expectedOutput: "整理考试要求与备考资料", dueDate: selectedDate, createdAtMs: project.createdAtMs, updatedAtMs: startMs + 43_200_000, completedAtMs: null, originKind: "ai", originKey: "preview-research", originConfidence: .94, reviewState: "confirmed" };
  const vocabularyTask: WorkLedgerTask = { ...task, id: "preview-vocabulary", title: "词汇学习", expectedOutput: "完成核心词汇复习", priority: "medium", originKey: "preview-vocabulary", originConfidence: .91 };
  const activity: WorkLedgerEvidence = { kind: "activity", id: "preview-search", occurredAtMs: startMs + 9 * 3_600_000, durationSeconds: 3_600, application: "Microsoft Edge", title: "雅思备考资料检索", domain: "", classificationSource: "rule", classificationReason: "学习型搜索", classificationConfidence: .94, evidenceHash: "preview-search" };
  const vocabulary: WorkLedgerEvidence = { kind: "activity", id: "preview-pdf", occurredAtMs: startMs + 11 * 3_600_000, durationSeconds: 2_400, application: "PDF Reader", title: "IELTS Vocabulary.pdf", domain: "", classificationSource: "rule", classificationReason: "学习型 PDF", classificationConfidence: .91, evidenceHash: "preview-pdf" };
  const browser: WorkLedgerEvidence = { kind: "browser", id: "preview-browser", occurredAtMs: startMs + 9 * 3_600_000, durationSeconds: 0, application: "Microsoft Edge", title: "IELTS preparation guide", domain: "ielts.org", classificationSource: "rule", classificationReason: "学习型搜索", classificationConfidence: .92, evidenceHash: "preview-browser" };
  const taskSummary = {
    taskId: task.id,
    lifecycleTotalSeconds: 5_400,
    activeDayAverageSeconds: 5_400,
    naturalDayAverageSeconds: 5_400,
    activeDayCount: 1,
    naturalDayCount: 1,
    latestActivityAtMs: activity.occurredAtMs,
    assignmentConfidence: 1,
    reviewState: "confirmed" as const,
  };
  return {
    projects: [project], tasks: [task, vocabularyTask],
    taskSummaries: [taskSummary],
    progress: [{ id: "preview-progress", taskId: task.id, note: "完成数据契约核对", createdAtMs: startMs + 10 * 3_600_000, originKind: "manual", sourceId: null, sourceDate: null }],
    linkedEvidence: [
      { taskId: task.id, provenance: "ai", assignmentConfidence: .94, assignmentReason: "共同目标：学习雅思", assignedAtMs: startMs + 10 * 3_600_000, evidence: activity },
      { taskId: task.id, provenance: "ai", assignmentConfidence: .92, assignmentReason: "共同目标：学习雅思", assignedAtMs: startMs + 10 * 3_600_000, evidence: browser },
      { taskId: vocabularyTask.id, provenance: "ai", assignmentConfidence: .91, assignmentReason: "共同目标：学习雅思", assignedAtMs: startMs + 12 * 3_600_000, evidence: vocabulary },
    ],
    unassignedEvidence: [],
    suggestions: [],
    ambiguousEvidenceHashes: [],
    summary: { projectCount: 1, taskCount: 2, progressCount: 1, linkedEvidenceCount: 3, unassignedEvidenceCount: 0, localSuggestionCount: 0, ambiguousEvidenceCount: 0 },
  };
}

function previewTaskInsight(selectedDate: string, taskId: string): TaskTimeInsight {
  const { startMs } = dayBounds(selectedDate);
  return {
    summary: {
      taskId,
      lifecycleTotalSeconds: 5_400,
      activeDayAverageSeconds: 5_400,
      naturalDayAverageSeconds: 5_400,
      activeDayCount: 1,
      naturalDayCount: 1,
      latestActivityAtMs: startMs + 9 * 3_600_000,
      assignmentConfidence: 1,
      reviewState: "confirmed",
    },
    dailyPoints: [{
      date: selectedDate,
      investedSeconds: 5_400,
      activitySeconds: 5_400,
      focusSeconds: 0,
      switchCount: 1,
    }],
    medianDailySeconds: 5_400,
    longestContinuousSeconds: 5_400,
    focusSeconds: 0,
    focusShare: 0,
    switchesPerHour: 2 / 3,
    regularity: 1,
    manualCorrectionRate: 0,
    pendingReviewSeconds: 0,
    browserEvidenceCount: 1,
    progressCount: 1,
    expectedOutput: "可验证的项目、任务与证据工作流",
    assessment: {
      dimensions: [
        { key: "time_investment", label: "时间投入", conclusion: "生命周期已投入 1 小时 30 分钟。", value: 5_400, unit: "秒" },
        { key: "continuity", label: "连续性", conclusion: "最长连续片段为 1 小时 30 分钟。", value: 5_400, unit: "秒" },
        { key: "switching_cost", label: "切换成本", conclusion: "当前约每小时切换 0.7 次。", value: 2 / 3, unit: "次/小时" },
        { key: "regularity", label: "规律性", conclusion: "当前仅有 1 个活跃日，规律性仍需更多样本。", value: 1, unit: "比例" },
        { key: "evidence_confidence", label: "证据可信度", conclusion: "现有归属均经人工确认。", value: 1, unit: "比例" },
      ],
      dataLimitations: ["本地预览仅包含一个活动日；浏览记录只作证据，不增加时长。"],
    },
  };
}

function previewProjectInsight(selectedDate: string, projectId: string, tasks: WorkLedgerTask[]): ProjectTimeInsight {
  const totalSeconds = 32 * 3_600 + 40 * 60;
  const dailyPoints = Array.from({ length: 30 }, (_, index) => {
    const date = new Date(`${selectedDate}T00:00:00Z`);
    date.setUTCDate(date.getUTCDate() - 29 + index);
    const investedSeconds = index % 7 === 5 || index % 11 === 0
      ? 0
      : [4_200, 5_400, 3_900, 6_600, 5_100, 4_500, 7_200][index % 7];
    const researchSeconds = Math.round(investedSeconds * .62);
    return {
      date: date.toISOString().slice(0, 10),
      investedSeconds,
      activitySeconds: investedSeconds,
      focusSeconds: Math.round(investedSeconds * .58),
      taskSeconds: {
        [tasks[0]?.id ?? "preview-research"]: researchSeconds,
        [tasks[1]?.id ?? "preview-vocabulary"]: investedSeconds - researchSeconds,
      },
      sharedSeconds: 0,
    };
  });
  return {
    summary: {
      projectId,
      lifecycleTotalSeconds: totalSeconds,
      activeDayAverageSeconds: 5_880,
      naturalDayAverageSeconds: 3_920,
      activeDayCount: 20,
      naturalDayCount: 30,
      evidenceCount: 28,
      completedTaskCount: 0,
      taskCount: 2,
      latestActivityAtMs: dayBounds(selectedDate).startMs + 10 * 3_600_000,
    },
    dailyPoints,
    taskContributions: tasks.map((task, index) => ({
      taskId: task.id,
      taskTitle: task.title,
      status: task.status,
      investedSeconds: Math.round(totalSeconds * (index === 0 ? .62 : .38)),
      evidenceCount: index === 0 ? 17 : 11,
    })),
    sharedSeconds: 0,
    focusSeconds: 18 * 3_600 + 20 * 60,
    longestContinuousSeconds: 5_760,
    switchCount: 28,
    switchesPerHour: .86,
    browserEvidenceCount: 17,
  };
}

export function WorkflowPage({ selectedDate, initialSnapshot, actions = defaultActions, onOpenTimeline, onOpenDate, reviewSubjectIds = emptyReviewSubjectIds, onOpenAiReview, requestedTaskId, refreshKey = 0 }: {
  selectedDate: string;
  initialSnapshot?: WorkLedgerSnapshot;
  actions?: WorkflowActions;
  onOpenTimeline?: (evidence: WorkLedgerEvidence) => void;
  onOpenDate?: (date: string) => void;
  reviewSubjectIds?: ReadonlySet<string>;
  onOpenAiReview?: (subjectId: string) => void;
  requestedTaskId?: string | null;
  refreshKey?: number;
}) {
  const localPreview = !initialSnapshot && actions === defaultActions && !isDesktopRuntime();
  const [snapshot, setSnapshot] = useState<WorkLedgerSnapshot | null>(() => initialSnapshot ?? (localPreview ? previewSnapshot(selectedDate) : null));
  const [loading, setLoading] = useState(!initialSnapshot && !localPreview);
  const [error, setError] = useState("");
  const [feedback, setFeedback] = useState("");
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(() => snapshot?.projects[0]?.id ?? null);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(() => snapshot?.tasks[0]?.id ?? null);
  const [projectEditor, setProjectEditor] = useState<WorkLedgerProject | "new" | null>(null);
  const [taskEditor, setTaskEditor] = useState<WorkLedgerTask | "new" | null>(null);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("overview");
  const [taskInsight, setTaskInsight] = useState<TaskTimeInsight | null>(() => (
    localPreview && snapshot?.tasks[0] ? previewTaskInsight(selectedDate, snapshot.tasks[0].id) : null
  ));
  const [taskInsightState, setTaskInsightState] = useState<{
    taskId: string;
    loading: boolean;
    error: string;
  }>({ taskId: "", loading: false, error: "" });
  const [projectRangeDays, setProjectRangeDays] = useState(30);
  const [projectInsight, setProjectInsight] = useState<ProjectTimeInsight | null>(null);
  const [projectInsightState, setProjectInsightState] = useState<{
    projectId: string;
    loading: boolean;
    error: string;
  }>({ projectId: "", loading: false, error: "" });
  const [projectDrawerOpen, setProjectDrawerOpen] = useState(false);
  const [taskDrawerOpen, setTaskDrawerOpen] = useState(false);
  const [singleContentLayout, setSingleContentLayout] = useState(() => (
    window.innerWidth <= 979 || (window.matchMedia?.("(max-width: 979px)").matches ?? false)
  ));
  const [selectedSuggestions, setSelectedSuggestions] = useState<Set<string>>(new Set());
  const drawerReturnFocusRef = useRef<HTMLElement | null>(null);
  const editorReturnFocusRef = useRef<HTMLElement | null>(null);
  const requestRef = useRef(0);
  const selectedBounds = dayBounds(selectedDate);
  const viewRef = useRef({ selectedDate, ...selectedBounds, version: 0 });
  if (viewRef.current.selectedDate !== selectedDate
    || viewRef.current.startMs !== selectedBounds.startMs
    || viewRef.current.endMs !== selectedBounds.endMs) {
    viewRef.current = { selectedDate, ...selectedBounds, version: viewRef.current.version + 1 };
  }
  const [pendingOperations, setPendingOperations] = useState<Set<string>>(new Set());
  const pendingOperationsRef = useRef(new Set<string>());

  type SnapshotOwner = typeof viewRef.current;

  function ownsSnapshot(owner: SnapshotOwner) {
    const current = viewRef.current;
    return current.version === owner.version
      && current.selectedDate === owner.selectedDate
      && current.startMs === owner.startMs
      && current.endMs === owner.endMs;
  }

  function operationKey(owner: SnapshotOwner, operation: string) {
    return `${owner.version}:${operation}`;
  }

  function setOperationPending(key: string, isPending: boolean) {
    if (isPending) pendingOperationsRef.current.add(key);
    else pendingOperationsRef.current.delete(key);
    setPendingOperations(new Set(pendingOperationsRef.current));
  }

  function beginOperation(operation: string) {
    const owner = { ...viewRef.current };
    const key = operationKey(owner, operation);
    if (pendingOperationsRef.current.has(key)) return null;
    setOperationPending(key, true);
    return { owner, key };
  }

  function isOperationPending(operation: string) {
    return pendingOperations.has(operationKey(viewRef.current, operation));
  }

  function reportError(owner: SnapshotOwner, reason: unknown) {
    if (!ownsSnapshot(owner)) return;
    setFeedback("");
    setError(reason instanceof Error ? reason.message : String(reason));
  }

  function reportSuccess(owner: SnapshotOwner, message: string) {
    if (!ownsSnapshot(owner)) return;
    setError("");
    setFeedback(message);
  }

  function applySnapshot(next: WorkLedgerSnapshot, preferredProjectId?: string | null, preferredTaskId?: string | null) {
    const nextProjects = next.projects.filter((project) => project.status === "active");
    const nextProjectId = preferredProjectId && nextProjects.some((project) => project.id === preferredProjectId)
      ? preferredProjectId
      : nextProjects.some((project) => project.id === selectedProjectId)
        ? selectedProjectId
        : nextProjects[0]?.id ?? null;
    const nextTaskId = preferredTaskId && next.tasks.some((task) => task.id === preferredTaskId && task.projectId === nextProjectId)
      ? preferredTaskId
      : next.tasks.some((task) => task.id === selectedTaskId && task.projectId === nextProjectId)
        ? selectedTaskId
        : next.tasks.find((task) => task.projectId === nextProjectId)?.id ?? null;
    setSnapshot(next);
    setSelectedProjectId(nextProjectId);
    setSelectedTaskId(nextTaskId);
  }

  async function reconcile(owner: SnapshotOwner, preferredProjectId?: string | null, preferredTaskId?: string | null) {
    if (localPreview) return ownsSnapshot(owner);
    const request = ++requestRef.current;
    const next = await actions.load(owner.startMs, owner.endMs);
    if (request !== requestRef.current || !ownsSnapshot(owner)) return false;
    applySnapshot(next, preferredProjectId, preferredTaskId);
    return true;
  }

  async function runAiSuggestions() {
    if (localPreview || !actions.runAiSuggestions) return;
    const operation = beginOperation("run-ai-suggestions");
    if (!operation) return;
    try {
      setError("");
      setFeedback("正在排队并分析近 30 天内可归集的工作证据…");
      const automation = await actions.getAiAutomationState?.();
      const result = await actions.runAiSuggestions(operation.owner.startMs, operation.owner.endMs);
      const queued = result.queuedCount;
      if (!await reconcile(operation.owner, selectedProjectId, selectedTaskId)) return;
      const mode = automation?.executionMode === "codex" ? "Codex" : "API key";
      reportSuccess(operation.owner, `Queued ${queued} new AI suggestion jobs · Current execution mode: ${mode}`);
      if (result.queuedCount === 0 && result.reusedCount === 0) {
        reportSuccess(operation.owner, `证据不足：当前窗口没有可分析的候选片段 · ${mode}`);
      } else {
        reportSuccess(operation.owner, `分析已启动：新排队 ${result.queuedCount} 个，复用 ${result.reusedCount} 个作业；高置信结果将自动创建，可随时取消 · ${mode}`);
      }
    } catch (reason) {
      await recover(operation.owner, reason);
    } finally {
      setOperationPending(operation.key, false);
    }
  }

  async function recover(owner: SnapshotOwner, reason: unknown) {
    try {
      await reconcile(owner);
    } catch {
      // The original operation error is more useful than a failed recovery request.
    }
    reportError(owner, reason);
  }

  useEffect(() => {
    const query = window.matchMedia?.("(max-width: 979px)");
    const update = () => setSingleContentLayout(
      window.innerWidth <= 979 || (query?.matches ?? false),
    );
    update();
    query?.addEventListener?.("change", update);
    window.addEventListener("resize", update);
    return () => {
      query?.removeEventListener?.("change", update);
      window.removeEventListener("resize", update);
    };
  }, []);

  useEffect(() => {
    if (initialSnapshot) return;
    if (localPreview) {
      const next = previewSnapshot(selectedDate);
      setSnapshot(next);
      setSelectedProjectId(next.projects[0]?.id ?? null);
      setSelectedTaskId(next.tasks[0]?.id ?? null);
      return;
    }
    const owner = { ...viewRef.current };
    const request = ++requestRef.current;
    setLoading(true);
    setError("");
    actions.load(owner.startMs, owner.endMs).then((next) => {
      if (request !== requestRef.current || !ownsSnapshot(owner)) return;
      applySnapshot(next);
    }).catch((reason: unknown) => {
      if (request === requestRef.current) reportError(owner, reason);
    }).finally(() => {
      if (request === requestRef.current && ownsSnapshot(owner)) setLoading(false);
    });
  }, [actions, initialSnapshot, localPreview, selectedDate]);

  useEffect(() => {
    if (!requestedTaskId || !snapshot) return;
    const task = snapshot.tasks.find((item) => item.id === requestedTaskId);
    if (!task) return;
    setSelectedProjectId(task.projectId);
    setSelectedTaskId(task.id);
    setInspectorTab("overview");
    setTaskDrawerOpen(true);
  }, [requestedTaskId, snapshot]);

  useEffect(() => {
    if (!refreshKey || initialSnapshot || localPreview) return;
    const owner = { ...viewRef.current };
    void reconcile(owner, selectedProjectId, requestedTaskId ?? selectedTaskId).catch((reason) => reportError(owner, reason));
  }, [refreshKey]);

  useEffect(() => {
    if (initialSnapshot || localPreview) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenWorkflowChanged(() => {
      if (disposed) return;
      const owner = { ...viewRef.current };
      void reconcile(owner, selectedProjectId, selectedTaskId).catch((reason) => {
        reportError(owner, reason);
      });
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [actions, initialSnapshot, localPreview, selectedProjectId, selectedTaskId]);

  useEffect(() => {
    if (!projectDrawerOpen && !taskDrawerOpen) return;
    const drawer = document.querySelector<HTMLElement>(projectDrawerOpen ? ".workflow-project-rail.drawer-open" : ".workflow-inspector.drawer-open");
    if (!drawer) return;
    const previouslyFocused = drawerReturnFocusRef.current ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    drawer.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setProjectDrawerOpen(false);
        setTaskDrawerOpen(false);
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(drawer.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex="0"]'));
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && (document.activeElement === first || document.activeElement === drawer)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      previouslyFocused?.focus();
    };
  }, [projectDrawerOpen, taskDrawerOpen]);

  function openDrawer(drawer: "project" | "task", trigger: HTMLElement) {
    drawerReturnFocusRef.current = trigger;
    setProjectDrawerOpen(drawer === "project");
    setTaskDrawerOpen(drawer === "task");
  }

  function closeDrawers() {
    setProjectDrawerOpen(false);
    setTaskDrawerOpen(false);
  }

  function openProjectEditor(project: WorkLedgerProject | "new") {
    editorReturnFocusRef.current = drawerReturnFocusRef.current ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    closeDrawers();
    setProjectEditor(project);
  }

  function openTaskEditor(task: WorkLedgerTask | "new") {
    editorReturnFocusRef.current = drawerReturnFocusRef.current ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    closeDrawers();
    setTaskEditor(task);
  }

  function closeProjectEditor() {
    setProjectEditor(null);
  }

  function closeTaskEditor() {
    setTaskEditor(null);
  }

  const activeProjects = useMemo(() => snapshot?.projects.filter((project) => project.status === "active") ?? [], [snapshot]);
  const selectedProject = activeProjects.find((project) => project.id === selectedProjectId) ?? null;
  const projectTasks = snapshot?.tasks.filter((task) => (
    task.projectId === selectedProjectId && task.status !== "cancelled"
  )) ?? [];
  const selectedTask = projectTasks.find((task) => task.id === selectedTaskId) ?? projectTasks[0] ?? null;
  const linked = snapshot?.linkedEvidence.filter((item) => item.taskId === selectedTask?.id) ?? [];
  const progress = snapshot?.progress.filter((entry) => entry.taskId === selectedTask?.id) ?? [];
  const relevantSuggestions = snapshot?.suggestions.filter((suggestion) => snapshot.tasks.some((task) => task.id === suggestion.taskId && task.projectId === selectedProjectId)) ?? [];

  useEffect(() => {
    if (!selectedTask) {
      setTaskInsight(null);
      setTaskInsightState({ taskId: "", loading: false, error: "" });
      return;
    }
    if (localPreview) {
      setTaskInsight(previewTaskInsight(selectedDate, selectedTask.id));
      setTaskInsightState({ taskId: selectedTask.id, loading: false, error: "" });
      return;
    }
    if (!actions.loadInsight) {
      setTaskInsight(null);
      setTaskInsightState({ taskId: selectedTask.id, loading: false, error: "" });
      return;
    }
    let current = true;
    setTaskInsightState({ taskId: selectedTask.id, loading: true, error: "" });
    actions.loadInsight(selectedTask.id, selectedBounds.endMs).then((insight) => {
      if (!current) return;
      setTaskInsight(insight);
      setTaskInsightState({ taskId: selectedTask.id, loading: false, error: "" });
    }).catch((reason: unknown) => {
      if (!current) return;
      setTaskInsight(null);
      setTaskInsightState({
        taskId: selectedTask.id,
        loading: false,
        error: reason instanceof Error ? reason.message : String(reason),
      });
    });
    return () => { current = false; };
  }, [actions, localPreview, selectedBounds.endMs, selectedTask?.id]);

  useEffect(() => {
    if (!selectedProject) {
      setProjectInsight(null);
      setProjectInsightState({ projectId: "", loading: false, error: "" });
      return;
    }
    if (localPreview && projectTasks[0]) {
      setProjectInsight(previewProjectInsight(selectedDate, selectedProject.id, projectTasks));
      setProjectInsightState({ projectId: selectedProject.id, loading: false, error: "" });
      return;
    }
    if (!actions.loadProjectInsight) {
      setProjectInsight(null);
      setProjectInsightState({ projectId: selectedProject.id, loading: false, error: "" });
      return;
    }
    let current = true;
    setProjectInsightState({ projectId: selectedProject.id, loading: true, error: "" });
    actions.loadProjectInsight(selectedProject.id, selectedBounds.endMs, projectRangeDays).then((insight) => {
      if (!current) return;
      setProjectInsight(insight);
      setProjectInsightState({ projectId: selectedProject.id, loading: false, error: "" });
    }).catch((reason: unknown) => {
      if (!current) return;
      setProjectInsight(null);
      setProjectInsightState({
        projectId: selectedProject.id,
        loading: false,
        error: reason instanceof Error ? reason.message : String(reason),
      });
    });
    return () => { current = false; };
  }, [
    actions,
    localPreview,
    projectRangeDays,
    selectedBounds.endMs,
    selectedDate,
    selectedProject?.id,
    selectedProject?.updatedAtMs,
    snapshot?.summary.linkedEvidenceCount,
    snapshot?.tasks.length,
  ]);

  async function mergeSelectedTask(targetTaskId: string) {
    if (!selectedTask || !actions.mergeTasks || selectedTask.id === targetTaskId) return;
    const operation = beginOperation(`merge-task:${selectedTask.id}`);
    if (!operation) return;
    try {
      const merged = await actions.mergeTasks(selectedTask.id, targetTaskId);
      if (!merged) {
        await recover(operation.owner, new Error("任务合并未完成，已重新加载最新数据"));
        return;
      }
      if (!await reconcile(operation.owner, selectedProjectId, targetTaskId)) return;
      setInspectorTab("overview");
      reportSuccess(operation.owner, "任务已合并，活动、进展与专注记录已迁移");
    } catch (reason) {
      await recover(operation.owner, reason);
    } finally {
      setOperationPending(operation.key, false);
    }
  }

  async function exportSelectedTask(format: ReportFormat) {
    if (!selectedTask || !actions.exportTask) return;
    const operation = beginOperation(`export-task:${selectedTask.id}`);
    if (!operation) return;
    try {
      const path = await actions.exportTask(selectedTask.id, operation.owner.endMs, format);
      reportSuccess(operation.owner, path ? `任务报告已保存：${path}` : "已取消导出");
    } catch (reason) {
      reportError(operation.owner, reason);
    } finally {
      setOperationPending(operation.key, false);
    }
  }

  async function exportSelectedProject(format: ReportFormat) {
    if (!selectedProject || !actions.exportProject) return;
    const operation = beginOperation(`export-project:${selectedProject.id}`);
    if (!operation) return;
    try {
      const path = await actions.exportProject(selectedProject.id, operation.owner.endMs, format);
      reportSuccess(operation.owner, path ? `项目报告已保存：${path}` : "已取消导出");
    } catch (reason) {
      reportError(operation.owner, reason);
    } finally {
      setOperationPending(operation.key, false);
    }
  }
  const workflowReviewSubjects = useMemo(() => {
    const evidenceIds = new Set([
      ...(snapshot?.unassignedEvidence.map((evidence) => evidence.id) ?? []),
      ...(snapshot?.linkedEvidence.map((item) => item.evidence.id) ?? []),
    ]);
    return [...reviewSubjectIds].filter((subjectId) => evidenceIds.has(subjectId));
  }, [reviewSubjectIds, snapshot]);
  const firstWorkflowReviewSubject = workflowReviewSubjects[0] ?? null;

  async function saveProject(request: WorkLedgerProjectSaveRequest) {
    const operation = beginOperation("save-project");
    if (!operation) return;
    try {
      const project = localPreview ? { ...request, status: "active" as const, createdAtMs: Date.now(), updatedAtMs: Date.now(), archivedAtMs: null } : await actions.saveProject(request);
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) setSnapshot((current) => current ? { ...current, projects: [...current.projects.filter((item) => item.id !== project.id), project] } : current);
      else if (!await reconcile(operation.owner, project.id)) return;
      setProjectEditor(null);
      reportSuccess(operation.owner, "项目已保存");
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  async function saveTask(request: WorkLedgerTaskSaveRequest) {
    const operation = beginOperation("save-task");
    if (!operation) return;
    try {
      const existing = snapshot?.tasks.find((task) => task.id === request.id);
      const task = localPreview
        ? { ...request, status: existing?.status ?? "todo" as const, createdAtMs: existing?.createdAtMs ?? Date.now(), updatedAtMs: Date.now(), completedAtMs: existing?.completedAtMs ?? null }
        : await actions.saveTask(request);
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) {
        setSnapshot((current) => current ? { ...current, tasks: [...current.tasks.filter((item) => item.id !== task.id), task] } : current);
        setSelectedProjectId(task.projectId);
        setSelectedTaskId(task.id);
      } else if (!await reconcile(operation.owner, task.projectId, task.id)) return;
      setTaskEditor(null);
      reportSuccess(operation.owner, "任务已保存");
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  async function completeTask(task: WorkLedgerTask) {
    const operation = beginOperation(`complete-task:${task.id}`);
    if (!operation) return;
    try {
      const next = localPreview ? { ...task, status: "completed" as const, completedAtMs: Date.now(), updatedAtMs: Date.now() } : await actions.updateTaskStatus(task.id, "completed");
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) setSnapshot((current) => current ? { ...current, tasks: current.tasks.map((item) => item.id === next.id ? next : item) } : current);
      else if (!await reconcile(operation.owner, next.projectId, next.id)) return;
      reportSuccess(operation.owner, "任务已完成");
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  async function cancelAiTask(task: WorkLedgerTask) {
    if (task.originKind !== "ai" || !actions.cancelAiTask) return;
    const operation = beginOperation(`cancel-ai-task:${task.id}`);
    if (!operation) return;
    try {
      const result = localPreview
        ? {
          task: { ...task, status: "cancelled" as const, completedAtMs: null, updatedAtMs: Date.now() },
          releasedEvidenceCount: snapshot?.linkedEvidence.filter((item) => item.taskId === task.id).length ?? 0,
          dismissedClusterCount: 1,
          projectArchived: false,
        }
        : await actions.cancelAiTask(task.id);
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) {
        setSnapshot((current) => current ? {
          ...current,
          tasks: current.tasks.map((item) => item.id === task.id ? result.task : item),
          linkedEvidence: current.linkedEvidence.filter((item) => item.taskId !== task.id),
        } : current);
        setSelectedTaskId(projectTasks.find((item) => item.id !== task.id)?.id ?? null);
      } else if (!await reconcile(operation.owner, result.projectArchived ? null : task.projectId)) {
        return;
      }
      reportSuccess(
        operation.owner,
        `已取消错误识别，释放 ${result.releasedEvidenceCount} 条自动证据${result.projectArchived ? "，空工作流已归档" : ""}`,
      );
    } catch (reason) {
      await recover(operation.owner, reason);
    } finally {
      setOperationPending(operation.key, false);
    }
  }

  async function addProgress(taskId: string, note: string): Promise<boolean> {
    const operation = beginOperation(`add-progress:${taskId}`);
    if (!operation) return false;
    try {
      const entry = localPreview ? { id: `progress-${Date.now()}`, taskId, note, createdAtMs: Date.now(), originKind: "manual" as const, sourceId: null, sourceDate: null } : await actions.addProgress(taskId, note);
      if (!ownsSnapshot(operation.owner)) return true;
      setSnapshot((current) => current ? {
        ...current,
        progress: current.progress.some((item) => item.id === entry.id)
          ? current.progress
          : [...current.progress, entry],
      } : current);
      if (!localPreview) {
        try {
          await reconcile(operation.owner, selectedProjectId, taskId);
        } catch {
          reportSuccess(operation.owner, "进展已保存，最新快照暂未刷新");
          return true;
        }
      }
      reportSuccess(operation.owner, "进展已记录");
      return true;
    } catch (reason) { await recover(operation.owner, reason); return false; } finally { setOperationPending(operation.key, false); }
  }

  async function assignEvidence(evidence: WorkLedgerEvidence) {
    if (!selectedTask) return;
    const operation = beginOperation(`assign-evidence:${evidence.kind}:${evidence.id}`);
    if (!operation) return;
    const request = { taskId: selectedTask.id, evidenceKind: evidence.kind, evidenceId: evidence.id, reason: "用户手动分配" } satisfies WorkLedgerEvidenceAssignmentRequest;
    try {
      const assigned = localPreview || await actions.assignEvidence(request);
      if (!assigned) { await recover(operation.owner, new Error("证据分配未完成，已重新加载最新数据")); return; }
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) setSnapshot((current) => current ? {
        ...current,
        unassignedEvidence: current.unassignedEvidence.filter((item) => !(item.kind === evidence.kind && item.id === evidence.id)),
        linkedEvidence: [...current.linkedEvidence.filter((item) => !(item.evidence.kind === evidence.kind && item.evidence.id === evidence.id)), { taskId: selectedTask.id, provenance: "manual", assignmentConfidence: 1, assignmentReason: "用户手动分配", assignedAtMs: Date.now(), evidence }],
        suggestions: current.suggestions.filter((item) => !(item.evidenceKind === evidence.kind && item.evidenceId === evidence.id)),
      } : current);
      else if (!await reconcile(operation.owner, selectedProjectId, selectedTask.id)) return;
      reportSuccess(operation.owner, "证据已分配");
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  async function removeEvidence(evidence: WorkLedgerEvidence) {
    if (!selectedTask) return;
    const operation = beginOperation(`remove-evidence:${evidence.kind}:${evidence.id}`);
    if (!operation) return;
    const request = { taskId: selectedTask.id, evidenceKind: evidence.kind, evidenceId: evidence.id } satisfies WorkLedgerEvidenceAssignmentRequest;
    try {
      const removed = localPreview || await actions.removeEvidence(request);
      if (!removed) { await recover(operation.owner, new Error("证据移除未完成，已重新加载最新数据")); return; }
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) setSnapshot((current) => current ? { ...current, linkedEvidence: current.linkedEvidence.filter((item) => !(item.taskId === selectedTask.id && item.evidence.kind === evidence.kind && item.evidence.id === evidence.id)), unassignedEvidence: current.unassignedEvidence.some((item) => item.kind === evidence.kind && item.id === evidence.id) ? current.unassignedEvidence : [...current.unassignedEvidence, evidence] } : current);
      else if (!await reconcile(operation.owner, selectedProjectId, selectedTask.id)) return;
      reportSuccess(operation.owner, "证据已移除");
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  async function confirmSuggestions() {
    if (!snapshot) return;
    const operation = beginOperation("confirm-suggestions");
    if (!operation) return;
    const selected = relevantSuggestions.filter((suggestion) => selectedSuggestions.has(`${suggestion.source}-${suggestion.evidenceKind}-${suggestion.evidenceId}-${suggestion.taskId}`));
    try {
      const results = localPreview
        ? selected.map(() => true)
        : await Promise.all(selected.map(async (suggestion) => {
          try {
            return await actions.applySuggestion({ ...suggestion, startMs: operation.owner.startMs, endMs: operation.owner.endMs });
          } catch {
            return false;
          }
        }));
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) setSnapshot((current) => current ? (() => {
        const acceptedKeys = new Set(selected.map((item) => `${item.evidenceKind}-${item.evidenceId}`));
        const newlyLinked = selected.flatMap((suggestion) => {
          const evidence = current.unassignedEvidence.find((item) => item.kind === suggestion.evidenceKind && item.id === suggestion.evidenceId);
          return evidence ? [{ taskId: suggestion.taskId, provenance: suggestion.source === "local" ? "rule" as const : "manual" as const, assignmentConfidence: suggestion.source === "local" ? suggestion.confidence : 1, assignmentReason: suggestion.reason, assignedAtMs: Date.now(), evidence }] : [];
        });
        return { ...current, suggestions: current.suggestions.filter((item) => !acceptedKeys.has(`${item.evidenceKind}-${item.evidenceId}`)), unassignedEvidence: current.unassignedEvidence.filter((item) => !acceptedKeys.has(`${item.kind}-${item.id}`)), linkedEvidence: [...current.linkedEvidence, ...newlyLinked] };
      })() : current);
      else if (!await reconcile(operation.owner, selectedProjectId, selectedTaskId)) return;
      setSelectedSuggestions(new Set());
      if (results.every(Boolean)) reportSuccess(operation.owner, `已确认 ${selected.length} 项建议`);
      else reportError(operation.owner, new Error("部分建议未完成，已重新加载最新数据"));
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  async function archiveProject(project: WorkLedgerProject) {
    const operation = beginOperation(`archive-project:${project.id}`);
    if (!operation) return;
    try {
      const archived = localPreview || await actions.archiveProject(project.id);
      if (!archived) { await recover(operation.owner, new Error("项目归档未完成，已重新加载最新数据")); return; }
      if (!ownsSnapshot(operation.owner)) return;
      if (localPreview) setSnapshot((current) => current ? { ...current, projects: current.projects.map((item) => item.id === project.id ? { ...item, status: "archived", archivedAtMs: Date.now() } : item) } : current);
      else if (!await reconcile(operation.owner)) return;
      const nextProject = activeProjects.find((item) => item.id !== project.id) ?? null;
      if (localPreview) {
        setSelectedProjectId(nextProject?.id ?? null);
        setSelectedTaskId(nextProject ? snapshot?.tasks.find((task) => task.projectId === nextProject.id)?.id ?? null : null);
      }
      reportSuccess(operation.owner, "项目已归档");
    } catch (reason) { await recover(operation.owner, reason); } finally { setOperationPending(operation.key, false); }
  }

  if (loading) return <section className="workflow-page workflow-state" role="status"><ClipboardList size={22} /><span>正在读取工作台账</span></section>;
  if (error && !snapshot) return <section className="workflow-page workflow-state error" role="alert"><strong>工作台账读取失败</strong><span>{error}</span></section>;

  return <section className="workflow-page" data-workflow-layout="dashboard">
    <div className="workflow-page-heading">
      <div className="workflow-page-title">
        <h1>工作流概览</h1>
        <label>
          <span className="sr-only">选择工作流</span>
          <select
            aria-label="选择工作流"
            value={selectedProjectId ?? ""}
            onChange={(event) => {
              const projectId = event.target.value || null;
              setSelectedProjectId(projectId);
              setSelectedTaskId(snapshot?.tasks.find((task) => (
                task.projectId === projectId && task.status !== "cancelled"
              ))?.id ?? null);
            }}
          >
            {!activeProjects.length && <option value="">暂无工作流</option>}
            {activeProjects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}
          </select>
        </label>
      </div>
      <div className="workflow-page-heading-meta">
        <p><Sparkles size={13} />高置信任务自动归集 · 识别错误可直接取消</p>
        <div className="workflow-heading-actions">
          <button type="button" title="新建工作流" aria-label="新建工作流" onClick={() => openProjectEditor("new")}><Plus size={14} /></button>
          <button type="button" title="编辑当前工作流" aria-label="编辑当前工作流" disabled={!selectedProject} onClick={() => selectedProject && openProjectEditor(selectedProject)}><Pencil size={14} /></button>
          <button type="button" title="归档当前工作流" aria-label="归档项目" disabled={!selectedProject} onClick={() => selectedProject && void archiveProject(selectedProject)}><Archive size={14} /></button>
          <button type="button" title="新建任务" aria-label="新建任务" disabled={!selectedProject} onClick={() => selectedProject && openTaskEditor("new")}><ClipboardList size={14} /></button>
          {!localPreview && <button type="button" className="workflow-auto-analyze-button" disabled={isOperationPending("run-ai-suggestions")} onClick={() => void runAiSuggestions()}><Sparkles size={14} />自动识别</button>}
          {selectedProject && actions.exportProject && <>
            <button type="button" title="导出 Markdown" aria-label="导出工作流 Markdown" onClick={() => void exportSelectedProject("markdown")}><Download size={14} />MD</button>
            <button type="button" title="导出 Word" aria-label="导出工作流 Word" onClick={() => void exportSelectedProject("docx")}><Download size={14} />Word</button>
          </>}
        </div>
      </div>
    </div>
    {(feedback || error) && <p className={`workflow-feedback ${error ? "error" : ""}`} role={error ? "alert" : "status"}>{error || feedback}</p>}
    {taskDrawerOpen && <button type="button" className="workflow-drawer-scrim" aria-label="关闭任务详情" onClick={closeDrawers} />}
    {selectedProject ? <ProjectDashboard
      project={selectedProject}
      tasks={projectTasks}
      insight={projectInsightState.projectId === selectedProject.id ? projectInsight : null}
      loading={projectInsightState.projectId === selectedProject.id && projectInsightState.loading}
      error={projectInsightState.projectId === selectedProject.id ? projectInsightState.error : ""}
      rangeDays={projectRangeDays}
      onRangeChange={setProjectRangeDays}
      selectedTaskId={selectedTask?.id}
      onSelectTask={(id) => {
        setSelectedTaskId(id);
        drawerReturnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        setTaskDrawerOpen(true);
      }}
      onOpenDate={onOpenDate}
      onCancelTask={(task) => void cancelAiTask(task)}
      pendingCancelTaskId={projectTasks.find((task) => isOperationPending(`cancel-ai-task:${task.id}`))?.id}
    /> : <div className="workflow-dashboard-empty-state">
      <ClipboardList size={28} />
      <strong>还没有可展示的工作流</strong>
      <span>高置信开发、搜索和学习活动将自动形成工作流；也可以手动新建。</span>
      <button type="button" className="workflow-primary-button" onClick={() => openProjectEditor("new")}><Plus size={15} />新建工作流</button>
    </div>}
    <aside id="workflow-task-drawer" className={`workflow-inspector ${taskDrawerOpen ? "drawer-open" : ""}`} hidden={!taskDrawerOpen} aria-label="任务检查器" role="dialog" aria-modal={taskDrawerOpen ? true : undefined} tabIndex={-1}>
      <header className="workflow-inspector-header">
        <div><span>任务详情</span><strong>{selectedTask?.title ?? "未选择任务"}</strong></div>
        <div className="workflow-inspector-header-actions">
          {selectedTask && <button type="button" className="workflow-icon-button" aria-label={`编辑任务：${selectedTask.title}`} title="编辑任务" onClick={() => openTaskEditor(selectedTask)}><Pencil size={15} /></button>}
          {selectedTask && selectedTask.status !== "completed" && <button type="button" className="workflow-icon-button" aria-label={`完成任务：${selectedTask.title}`} title="标记完成" disabled={isOperationPending(`complete-task:${selectedTask.id}`)} onClick={() => void completeTask(selectedTask)}><Check size={15} /></button>}
          <button type="button" className="workflow-icon-button workflow-drawer-close" aria-label="关闭任务抽屉" title="关闭任务详情" onClick={closeDrawers}><X size={16} /></button>
        </div>
      </header>
      {selectedTask ? <>
        <div className="workflow-inspector-tabs" role="tablist" aria-label="任务详情视图">
          {inspectorTabs.map((tab) => <button type="button" role="tab" aria-controls={`workflow-${tab}-panel`} id={`workflow-${tab}-tab`} aria-selected={inspectorTab === tab} tabIndex={inspectorTab === tab ? 0 : -1} data-inspector-tab={tab} key={tab} onClick={() => setInspectorTab(tab)} onKeyDown={(event) => { const next = nextInspectorTab(tab, event.key); if (!next) return; event.preventDefault(); setInspectorTab(next); requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(`button[data-inspector-tab="${next}"]`)?.focus()); }}>{tab === "overview" ? "概览" : tab === "progress" ? "进展" : "证据"}</button>)}
        </div>
        <div className="workflow-inspector-content" role="tabpanel" id={`workflow-${inspectorTab}-panel`} aria-labelledby={`workflow-${inspectorTab}-tab`}>
          {inspectorTab === "overview" && <TaskOverview
            insight={taskInsightState.taskId === selectedTask.id ? taskInsight : null}
            loading={taskInsightState.taskId === selectedTask.id && taskInsightState.loading}
            error={taskInsightState.taskId === selectedTask.id ? taskInsightState.error : ""}
            mergeTargets={(snapshot?.tasks ?? []).filter((task) => task.id !== selectedTask.id)}
            pending={isOperationPending(`merge-task:${selectedTask.id}`) || isOperationPending(`export-task:${selectedTask.id}`)}
            onMerge={(targetTaskId) => void mergeSelectedTask(targetTaskId)}
            onExport={(format) => void exportSelectedTask(format)}
          />}
          {inspectorTab === "progress" && <ProgressTimeline key={`${selectedDate}:${selectedTask.id}`} taskId={selectedTask.id} entries={progress} pending={isOperationPending(`add-progress:${selectedTask.id}`)} onAdd={addProgress} />}
          {inspectorTab === "evidence" && <EvidenceAssignmentPanel taskId={selectedTask.id} linked={linked} unassigned={snapshot?.unassignedEvidence ?? []} pendingAssignments={new Set((snapshot?.unassignedEvidence ?? []).filter((evidence) => isOperationPending(`assign-evidence:${evidence.kind}:${evidence.id}`)).map((evidence) => `${evidence.kind}:${evidence.id}`))} pendingRemovals={new Set(linked.filter((item) => isOperationPending(`remove-evidence:${item.evidence.kind}:${item.evidence.id}`)).map((item) => `${item.evidence.kind}:${item.evidence.id}`))} onAssign={(evidence) => void assignEvidence(evidence)} onRemove={(evidence) => void removeEvidence(evidence)} onOpenTimeline={onOpenTimeline} />}
        </div>
      </> : <p className="workflow-empty workflow-inspector-empty">选择任务后查看进展与证据。</p>}
    </aside>
    {projectEditor && <ProjectEditor project={projectEditor === "new" ? null : projectEditor} pending={isOperationPending("save-project")} onSave={(request) => void saveProject(request)} onClose={closeProjectEditor} restoreFocusRef={editorReturnFocusRef} />}
    {taskEditor && selectedProject && <TaskEditor task={taskEditor === "new" ? null : taskEditor} projectId={selectedProject.id} projects={activeProjects} pending={isOperationPending("save-task")} onSave={(request) => void saveTask(request)} onClose={closeTaskEditor} restoreFocusRef={editorReturnFocusRef} />}
  </section>;
}

function ProjectEditor({ project, pending, onSave, onClose, restoreFocusRef }: { project: WorkLedgerProject | null; pending: boolean; onSave: (request: WorkLedgerProjectSaveRequest) => void; onClose: () => void; restoreFocusRef?: RefObject<HTMLElement | null> }) {
  const [name, setName] = useState(project?.name ?? "");
  const [description, setDescription] = useState(project?.description ?? "");
  const [color, setColor] = useState(project?.color ?? "#2563eb");
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  useEffect(() => inputRef.current?.focus(), []);
  useEffect(() => () => restoreFocusRef?.current?.focus(), []);
  return <div className="workflow-dialog-layer" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
    <section ref={dialogRef} className="workflow-dialog workflow-project-dialog" role="dialog" aria-modal="true" aria-label={project ? "编辑项目" : "新建项目"} onKeyDown={(event) => trapDialogFocus(event, dialogRef.current, onClose)}>
      <header><div><span>PROJECT</span><h2>{project ? "编辑项目" : "新建项目"}</h2></div><button type="button" className="workflow-icon-button" aria-label="关闭项目编辑器" title="关闭" onClick={onClose}><X size={17} /></button></header>
      <form onSubmit={(event) => { event.preventDefault(); if (!pending && name.trim()) onSave({ id: project?.id ?? `project-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`, name: name.trim(), color, description: description.trim() }); }}>
        <label htmlFor="workflow-project-name">项目名称</label><input ref={inputRef} id="workflow-project-name" value={name} onInput={(event) => setName(event.currentTarget.value)} disabled={pending} required />
        <label htmlFor="workflow-project-description">项目说明</label><textarea id="workflow-project-description" value={description} onInput={(event) => setDescription(event.currentTarget.value)} disabled={pending} />
        <label htmlFor="workflow-project-color">项目颜色</label><div className="workflow-color-field"><input id="workflow-project-color" type="color" value={color} onChange={(event) => setColor(event.target.value)} disabled={pending} /><span>{color}</span></div>
        <footer><button type="button" className="workflow-secondary-button" onClick={onClose} disabled={pending}>取消</button><button type="submit" className="workflow-primary-button" disabled={pending}>保存项目</button></footer>
      </form>
    </section>
  </div>;
}
