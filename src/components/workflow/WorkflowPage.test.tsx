import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  WorkLedgerEvidenceAssignmentRequest,
  WorkLedgerProjectSaveRequest,
  WorkLedgerSnapshot,
  WorkLedgerSuggestedAssignmentRequest,
  WorkLedgerTaskSaveRequest,
  WorkLedgerTaskStatus,
} from "../../lib/desktop";
import { WorkflowPage, type WorkflowActions } from "./WorkflowPage";

type TestWindow = {
  Event: typeof Event;
};

const snapshot: WorkLedgerSnapshot = {
  projects: [
    {
      id: "project-1",
      name: "桌面重写",
      color: "#2563eb",
      status: "active",
      description: "完成桌面端工作台",
      createdAtMs: 1_000,
      updatedAtMs: 5_000,
      archivedAtMs: null,
    },
  ],
  tasks: [
    {
      id: "task-1",
      projectId: "project-1",
      title: "构建工作台",
      status: "in_progress",
      priority: "high",
      expectedOutput: "可操作的三栏台账",
      dueDate: "2026-07-13",
      createdAtMs: 2_000,
      updatedAtMs: 5_000,
      completedAtMs: null,
    },
  ],
  progress: [{ id: "progress-1", taskId: "task-1", note: "完成契约核对", createdAtMs: 4_000, originKind: "manual", sourceId: null, sourceDate: null }],
  linkedEvidence: [
    {
      taskId: "task-1",
      provenance: "manual",
      assignmentConfidence: 1,
      assignmentReason: "用户确认",
      assignedAtMs: 4_500,
      evidence: {
        kind: "activity",
        id: "segment-1",
        occurredAtMs: new Date(2026, 6, 13, 9, 0).getTime(),
        durationSeconds: 3_600,
        application: "Visual Studio Code",
        title: "WorkflowPage.tsx",
        domain: "",
        classificationSource: "rule",
        classificationReason: "编辑器活动",
        classificationConfidence: 0.92,
        evidenceHash: "hash-1",
      },
    },
  ],
  unassignedEvidence: [
    {
      kind: "browser",
      id: "visit-1",
      occurredAtMs: new Date(2026, 6, 13, 10, 0).getTime(),
      durationSeconds: 0,
      application: "Chrome",
      title: "React documentation",
      domain: "react.dev",
      classificationSource: "unclassified",
      classificationReason: "",
      classificationConfidence: null,
      evidenceHash: "hash-2",
    },
  ],
  suggestions: [
    {
      evidenceKind: "browser",
      evidenceId: "visit-1",
      taskId: "task-1",
      confidence: 0.81,
      reason: "任务标题与页面标题匹配",
      evidenceHash: "hash-2",
      source: "local",
      canAutoApply: true,
    },
  ],
  ambiguousEvidenceHashes: [],
  summary: {
    projectCount: 1,
    taskCount: 1,
    progressCount: 1,
    linkedEvidenceCount: 1,
    unassignedEvidenceCount: 1,
    localSuggestionCount: 1,
    ambiguousEvidenceCount: 0,
  },
};

function copySnapshot(value: WorkLedgerSnapshot = snapshot): WorkLedgerSnapshot {
  return structuredClone(value);
}

function actions(overrides: Partial<WorkflowActions> = {}): WorkflowActions {
  return {
    load: vi.fn().mockImplementation(async () => copySnapshot()),
    saveProject: vi.fn().mockImplementation(async (request: WorkLedgerProjectSaveRequest) => ({
      id: request.id,
      name: request.name,
      color: request.color,
      status: "active" as const,
      description: request.description,
      createdAtMs: 6_000,
      updatedAtMs: 6_000,
      archivedAtMs: null,
    })),
    archiveProject: vi.fn().mockResolvedValue(true),
    saveTask: vi.fn().mockImplementation(async (request: WorkLedgerTaskSaveRequest) => ({
      ...request,
      status: "todo" as const,
      createdAtMs: 6_000,
      updatedAtMs: 6_000,
      completedAtMs: null,
    })),
    updateTaskStatus: vi.fn().mockImplementation(async (taskId: string, status: WorkLedgerTaskStatus) => ({
      ...snapshot.tasks[0], id: taskId, status,
    })),
    addProgress: vi.fn().mockImplementation(async (taskId: string, note: string) => ({
      id: "progress-new", taskId, note, createdAtMs: 7_000,
    })),
    assignEvidence: vi.fn().mockResolvedValue(true),
    removeEvidence: vi.fn().mockResolvedValue(true),
    applySuggestion: vi.fn().mockResolvedValue(true),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function mount({
  actionOverrides = {},
  selectedDate = "2026-07-13",
  mobile = false,
  viewportWidth,
  onOpenAiReview,
  reviewSubjectIds = new Set(),
}: {
  actionOverrides?: Partial<WorkflowActions>;
  selectedDate?: string;
  mobile?: boolean;
  viewportWidth?: number;
  onOpenAiReview?: (subjectId: string) => void;
  reviewSubjectIds?: ReadonlySet<string>;
} = {}) {
  const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
  let activeElement: Element | null = null;
  Object.defineProperty(document, "activeElement", {
    configurable: true,
    get: () => activeElement,
  });
  Object.defineProperty(window.HTMLElement.prototype, "focus", {
    configurable: true,
    value(this: HTMLElement) { activeElement = this; },
  });
  const mediaListeners = new Set<(event: MediaQueryListEvent) => void>();
  const effectiveWidth = viewportWidth ?? (mobile ? 599 : 1200);
  const mediaQuery = {
    matches: effectiveWidth <= 979,
    media: "(max-width: 979px)",
    onchange: null,
    addEventListener: (_type: string, listener: EventListenerOrEventListenerObject) => {
      if (typeof listener === "function") mediaListeners.add(listener as (event: MediaQueryListEvent) => void);
    },
    removeEventListener: (_type: string, listener: EventListenerOrEventListenerObject) => {
      if (typeof listener === "function") mediaListeners.delete(listener as (event: MediaQueryListEvent) => void);
    },
    addListener: (listener: (event: MediaQueryListEvent) => void) => mediaListeners.add(listener),
    removeListener: (listener: (event: MediaQueryListEvent) => void) => mediaListeners.delete(listener),
    dispatchEvent: () => true,
  } as MediaQueryList;
  Object.assign(window, {
    innerWidth: effectiveWidth,
    matchMedia: vi.fn().mockReturnValue(mediaQuery),
    requestAnimationFrame: (callback: FrameRequestCallback) => { callback(0); return 1; },
    cancelAnimationFrame: () => undefined,
  });
  vi.stubGlobal("window", window);
  vi.stubGlobal("document", document);
  vi.stubGlobal("navigator", window.navigator);
  vi.stubGlobal("HTMLElement", window.HTMLElement);
  vi.stubGlobal("HTMLDialogElement", window.HTMLDialogElement);
  vi.stubGlobal("Node", window.Node);
  vi.stubGlobal("Event", window.Event);
  vi.stubGlobal("MouseEvent", window.MouseEvent);
  vi.stubGlobal("requestAnimationFrame", window.requestAnimationFrame);
  vi.stubGlobal("cancelAnimationFrame", window.cancelAnimationFrame);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const actionSet = actions(actionOverrides);
  const container = document.getElementById("root") as unknown as HTMLDivElement;
  const root = createRoot(container);
  const render = async (date = selectedDate, onOpenTimeline?: (evidence: WorkLedgerSnapshot["unassignedEvidence"][number]) => void, navigation?: { requestedTaskId?: string | null; refreshKey?: number }) => {
    await act(async () => {
      root.render(<WorkflowPage selectedDate={date} actions={actionSet} onOpenTimeline={onOpenTimeline} onOpenAiReview={onOpenAiReview} reviewSubjectIds={reviewSubjectIds} requestedTaskId={navigation?.requestedTaskId} refreshKey={navigation?.refreshKey} />);
    });
    await act(async () => undefined);
  };
  await render();
  return { actionSet, container, document, render, root, window };
}

function click(window: Window, element: Element | null) {
  if (!element) throw new Error("Expected element to exist");
  element.dispatchEvent(new (window as unknown as TestWindow).Event("click", { bubbles: true, cancelable: true }));
}

function input(window: Window, element: Element | null, value: string) {
  if (!element) throw new Error("Expected input to exist");
  (element as HTMLInputElement).value = value;
  element.dispatchEvent(new (window as unknown as TestWindow).Event("input", { bubbles: true }));
  element.dispatchEvent(new (window as unknown as TestWindow).Event("change", { bubbles: true }));
}

function select(window: Window, element: Element | null, value: string) {
  if (!element) throw new Error("Expected select to exist");
  Object.defineProperty(element, "value", { configurable: true, value });
  element.dispatchEvent(new (window as unknown as TestWindow).Event("change", { bubbles: true }));
}

function submit(window: Window, form: Element | null) {
  if (!form) throw new Error("Expected form to exist");
  form.dispatchEvent(new (window as unknown as TestWindow).Event("submit", { bubbles: true, cancelable: true }));
}

function keydown(window: Window, element: Element, key: string, shiftKey = false) {
  const event = new (window as unknown as TestWindow).Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    key: { value: key },
    shiftKey: { value: shiftKey },
  });
  element.dispatchEvent(event);
}

afterEach(() => vi.unstubAllGlobals());

describe("WorkflowPage", () => {
  it("opens selected tasks on the transparent overview by default", async () => {
    const { container, root } = await mount();
    try {
      expect(container.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toContain("概览");
      expect(container.querySelector('[role="tabpanel"]')?.id).toBe("workflow-overview-panel");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("lets an edited task move to another project", async () => {
    const twoProjects = copySnapshot();
    twoProjects.projects.push({
      ...twoProjects.projects[0],
      id: "project-2",
      name: "研究收集箱",
    });
    twoProjects.summary.projectCount = 2;
    const saveTask = vi.fn().mockImplementation(async (request: WorkLedgerTaskSaveRequest) => ({
      ...twoProjects.tasks[0],
      ...request,
      updatedAtMs: 8_000,
    }));
    const { container, root, window } = await mount({
      actionOverrides: {
        load: vi.fn().mockResolvedValue(twoProjects),
        saveTask,
      },
    });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="编辑任务：构建工作台"]')));
      const projectSelect = container.querySelector("#workflow-task-project");
      expect(projectSelect?.textContent).toContain("研究收集箱");
      await act(async () => select(window as unknown as Window, projectSelect, "project-2"));
      await act(async () => submit(window as unknown as Window, container.querySelector(".workflow-dialog form")));
      expect(saveTask).toHaveBeenCalledWith(expect.objectContaining({
        id: "task-1",
        projectId: "project-2",
      }));
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("does not surface legacy confirmation markers in automatic workflow mode", async () => {
    const onOpenAiReview = vi.fn();
    const { container, root } = await mount({ onOpenAiReview, reviewSubjectIds: new Set(["visit-1"]) });
    try {
      expect(container.querySelector('[aria-label="在 AI 审核中心查看待确认建议"]')).toBeNull();
      expect(onOpenAiReview).not.toHaveBeenCalled();
      expect(container.textContent).toContain("高置信任务自动归集");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("does not infer a marker from workflow suggestions", async () => {
    const { container, root } = await mount({ onOpenAiReview: vi.fn(), reviewSubjectIds: new Set() });
    try {
      expect(snapshot.suggestions).toHaveLength(1);
      expect(container.querySelector('[aria-label="在 AI 审核中心查看待确认建议"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("selects an externally requested task and preserves it through an external refresh", async () => {
    const twoTasks = copySnapshot();
    twoTasks.tasks.push({ ...twoTasks.tasks[0], id: "task-2", title: "Externally selected", createdAtMs: 3_000, updatedAtMs: 6_000 });
    twoTasks.summary.taskCount = 2;
    const load = vi.fn().mockResolvedValue(twoTasks);
    const { actionSet, container, render, root } = await mount({ actionOverrides: { load } });
    try {
      await render("2026-07-13", undefined, { requestedTaskId: "task-2", refreshKey: 0 });
      const selected = [...container.querySelectorAll<HTMLButtonElement>(".workflow-task-select")].find((button) => button.textContent?.includes("Externally selected"));
      expect(selected?.getAttribute("aria-pressed")).toBe("true");
      expect(container.querySelector(".workflow-inspector-header")?.textContent).toContain("Externally selected");

      await render("2026-07-13", undefined, { requestedTaskId: "task-2", refreshKey: 1 });
      expect(actionSet.load).toHaveBeenCalledTimes(2);
      const refreshed = [...container.querySelectorAll<HTMLButtonElement>(".workflow-task-select")].find((button) => button.textContent?.includes("Externally selected"));
      expect(refreshed?.getAttribute("aria-pressed")).toBe("true");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("mounts the reference-aligned workflow dashboard with task inspection", async () => {
    const { container, root } = await mount();
    try {
      expect(container.querySelector(".workflow-project-dashboard")).not.toBeNull();
      expect(container.querySelector(".workflow-project-visuals")).not.toBeNull();
      expect(container.querySelector(".workflow-task-summary-card")).not.toBeNull();
      expect(container.querySelector(".workflow-inspector")).not.toBeNull();
      expect(container.textContent).toContain("任务构成");
      expect(container.textContent).toContain("每日投入");
      expect(container.textContent).toContain("任务列表");
      expect(container.textContent).toContain("AI 洞察");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("keeps manual AI suggestions enabled with automation off and reports queue count and execution mode", async () => {
    const runAiSuggestions = vi.fn()
      .mockResolvedValueOnce({ queuedCount: 3, reusedCount: 1, jobIds: ["a", "b", "c", "d"] })
      .mockResolvedValueOnce({ queuedCount: 0, reusedCount: 0, jobIds: [] });
    const getAiAutomationState = vi.fn().mockResolvedValue({
      enabled: false,
      executionMode: "codex" as const,
    });
    const { container, root, window } = await mount({
      actionOverrides: { runAiSuggestions, getAiAutomationState },
    });
    try {
      const trigger = [...container.querySelectorAll<HTMLButtonElement>("button")]
        .find((button) => button.textContent?.includes("自动识别"));
      expect(trigger).not.toBeUndefined();
      expect(trigger?.disabled).toBe(false);

      await act(async () => click(window as unknown as Window, trigger ?? null));

      expect(runAiSuggestions).toHaveBeenCalledTimes(1);
      expect(container.querySelector(".workflow-feedback")?.textContent).toContain("新排队 3 个");
      expect(container.querySelector(".workflow-feedback")?.textContent).toContain("复用 1 个");
      expect(container.querySelector(".workflow-feedback")?.textContent).toContain("Codex");

      await act(async () => click(window as unknown as Window, trigger ?? null));

      expect(runAiSuggestions).toHaveBeenCalledTimes(2);
      expect(container.querySelector(".workflow-feedback")?.textContent).toContain("证据不足");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("does not render legacy semantic drafts that require confirmation", async () => {
    const next = copySnapshot();
    next.projectDrafts = [{
      reviewId: "review-ielts",
      subjectId: "workflow-batch-ielts",
      proposal: {
        targetProjectId: null,
        name: "学习雅思",
        description: "准备雅思考试并积累核心词汇",
        tasks: [
          { key: "research", title: "检索雅思备考资料", expectedOutput: "整理备考资料", clusterIds: ["cluster-browser"] },
          { key: "vocabulary", title: "背诵雅思词汇", expectedOutput: "完成词汇复习", clusterIds: ["cluster-pdf"] },
        ],
        confidence: 0.93,
        reasonCode: "shared_goal",
      },
      evidenceCount: 18,
      accumulatedSeconds: 2_400,
      createdAtMs: 10_000,
    }];
    const onOpenAiReview = vi.fn();
    const { container, root, window } = await mount({
      actionOverrides: { load: vi.fn().mockResolvedValue(next) },
      onOpenAiReview,
    });
    try {
      expect(container.textContent).not.toContain("编辑并确认");
      expect(container.textContent).not.toContain("18 条证据");
      expect(onOpenAiReview).not.toHaveBeenCalled();
      expect(container.textContent).toContain("高置信任务自动归集");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("keeps create, edit, archive, and completion flows authoritative", async () => {
    let authoritative = copySnapshot();
    const load = vi.fn().mockImplementation(async () => copySnapshot(authoritative));
    const saveProject = vi.fn().mockImplementation(async (request: WorkLedgerProjectSaveRequest) => {
      const existing = authoritative.projects.find((project) => project.id === request.id);
      const project = {
        ...request,
        status: "active" as const,
        createdAtMs: existing?.createdAtMs ?? 6_000,
        updatedAtMs: 6_000,
        archivedAtMs: null,
      };
      authoritative.projects = [...authoritative.projects.filter((item) => item.id !== project.id), project];
      authoritative.summary.projectCount = authoritative.projects.length;
      return project;
    });
    const archiveProject = vi.fn().mockImplementation(async (projectId: string) => {
      authoritative.projects = authoritative.projects.map((project) => project.id === projectId
        ? { ...project, status: "archived", archivedAtMs: 8_000 }
        : project);
      authoritative.summary.projectCount = authoritative.projects.filter((project) => project.status === "active").length;
      return true;
    });
    const updateTaskStatus = vi.fn().mockImplementation(async (taskId: string, status: WorkLedgerTaskStatus) => {
      const task = { ...authoritative.tasks.find((item) => item.id === taskId)!, status };
      authoritative.tasks = authoritative.tasks.map((item) => item.id === taskId ? task : item);
      return task;
    });
    const { actionSet, container, root, window } = await mount({ actionOverrides: { load, saveProject, archiveProject, updateTaskStatus } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="完成任务：构建工作台"]')));
      expect(actionSet.updateTaskStatus).toHaveBeenCalledWith("task-1", "completed");
      expect(container.textContent).toContain("已完成");

      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="新建工作流"]')));
      await act(async () => input(window as unknown as Window, container.querySelector("#workflow-project-name"), "发布准备"));
      await act(async () => submit(window as unknown as Window, container.querySelector(".workflow-dialog form")));
      expect(actionSet.saveProject).toHaveBeenCalledWith(expect.objectContaining({ name: "发布准备" }));
      expect(container.textContent).toContain("发布准备");

      await act(async () => select(window as unknown as Window, container.querySelector('select[aria-label="选择工作流"]'), "project-1"));
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="归档项目"]')));
      expect(actionSet.archiveProject).toHaveBeenCalledWith("project-1");
      expect(container.textContent).toContain("项目已归档");
      expect(container.textContent).not.toContain("构建工作台");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("treats false evidence mutations as errors, reloads, updates summary counts, and clears the error after success", async () => {
    const reconciledFailure = copySnapshot();
    const reconciledSuccess = copySnapshot();
    const evidence = reconciledSuccess.unassignedEvidence[0];
    reconciledSuccess.unassignedEvidence = [];
    reconciledSuccess.suggestions = [];
    reconciledSuccess.linkedEvidence.push({
      taskId: "task-1",
      provenance: "manual",
      assignmentConfidence: 1,
      assignmentReason: "用户手动分配",
      assignedAtMs: 9_000,
      evidence,
    });
    reconciledSuccess.summary = {
      ...reconciledSuccess.summary,
      linkedEvidenceCount: 2,
      unassignedEvidenceCount: 0,
      localSuggestionCount: 0,
    };
    const load = vi.fn()
      .mockResolvedValueOnce(copySnapshot())
      .mockResolvedValueOnce(reconciledFailure)
      .mockResolvedValueOnce(reconciledSuccess);
    const assignEvidence = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const { actionSet, container, root, window } = await mount({ actionOverrides: { load, assignEvidence } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="evidence"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="分配证据：React documentation"]')));
      expect(actionSet.load).toHaveBeenCalledTimes(2);
      expect(container.querySelector('[role="alert"]')?.textContent).toContain("未完成");
      expect(container.textContent).toContain("1 条已关联证据");
      expect(container.querySelector('button[aria-label="分配证据：React documentation"]')).not.toBeNull();

      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="分配证据：React documentation"]')));
      expect(actionSet.load).toHaveBeenCalledTimes(3);
      expect(container.querySelector('[role="alert"]')).toBeNull();
      expect(container.textContent).toContain("2 条已关联证据");
      expect(container.querySelector('button[aria-label="分配证据：React documentation"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("treats a false direct removal as a rejection and restores the authoritative link", async () => {
    const reconciled = copySnapshot();
    const load = vi.fn()
      .mockResolvedValueOnce(copySnapshot())
      .mockResolvedValueOnce(reconciled);
    const removeEvidence = vi.fn().mockResolvedValue(false);
    const { actionSet, container, root, window } = await mount({ actionOverrides: { load, removeEvidence } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="evidence"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="移除证据：WorkflowPage.tsx"]')));

      expect(actionSet.removeEvidence).toHaveBeenCalledWith({
        taskId: "task-1", evidenceKind: "activity", evidenceId: "segment-1",
      } satisfies WorkLedgerEvidenceAssignmentRequest);
      expect(actionSet.load).toHaveBeenCalledTimes(2);
      expect(container.querySelector('[role="alert"]')?.textContent).toContain("未完成");
      expect(container.querySelector('button[aria-label="移除证据：WorkflowPage.tsx"]')).not.toBeNull();
      expect(container.textContent).toContain("1 条已关联证据");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("disables the relevant evidence control while its mutation owns the request", async () => {
    const assignment = deferred<boolean>();
    const success = copySnapshot();
    const evidence = success.unassignedEvidence[0];
    success.unassignedEvidence = [];
    success.suggestions = [];
    success.linkedEvidence.push({ taskId: "task-1", provenance: "manual", assignmentConfidence: 1, assignmentReason: "用户手动分配", assignedAtMs: 9_000, evidence });
    success.summary = { ...success.summary, linkedEvidenceCount: 2, unassignedEvidenceCount: 0, localSuggestionCount: 0 };
    const load = vi.fn().mockResolvedValueOnce(copySnapshot()).mockResolvedValueOnce(success);
    const assignEvidence = vi.fn().mockReturnValue(assignment.promise);
    const { container, root, window } = await mount({ actionOverrides: { load, assignEvidence } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="evidence"]')));
      const assignButton = container.querySelector<HTMLButtonElement>('button[aria-label="分配证据：React documentation"]')!;
      await act(async () => click(window as unknown as Window, assignButton));
      expect(assignButton.disabled).toBe(true);
      await act(async () => click(window as unknown as Window, assignButton));
      expect(assignEvidence).toHaveBeenCalledTimes(1);
      await act(async () => assignment.resolve(true));
      expect(container.querySelector('button[aria-label="分配证据：React documentation"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("preserves progress text and disables submit until persistence succeeds", async () => {
    const progressRequest = deferred<never>();
    const addProgress = vi.fn().mockReturnValue(progressRequest.promise);
    const load = vi.fn().mockImplementation(async () => copySnapshot());
    const { container, root, window } = await mount({ actionOverrides: { load, addProgress } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="progress"]')));
      const textarea = container.querySelector<HTMLTextAreaElement>("#workflow-progress-note")!;
      await act(async () => input(window as unknown as Window, textarea, "完成响应式布局"));
      await act(async () => submit(window as unknown as Window, container.querySelector(".workflow-progress-form")));
      expect(container.querySelector<HTMLButtonElement>('button[aria-label="添加进展"]')?.disabled).toBe(true);
      expect(textarea.value).toBe("完成响应式布局");
      await act(async () => progressRequest.reject(new Error("进展保存失败")));
      expect(textarea.value).toBe("完成响应式布局");
      expect(container.querySelector('[role="alert"]')?.textContent).toContain("进展保存失败");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("clears a progress draft when the selected task changes", async () => {
    const twoTasks = copySnapshot();
    twoTasks.tasks.push({
      ...twoTasks.tasks[0],
      id: "task-2",
      title: "验证另一个任务",
      createdAtMs: 3_000,
      updatedAtMs: 6_000,
    });
    twoTasks.summary.taskCount = 2;
    const load = vi.fn().mockResolvedValue(twoTasks);
    const { container, root, window } = await mount({ actionOverrides: { load } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="progress"]')));
      const textarea = container.querySelector<HTMLTextAreaElement>("#workflow-progress-note")!;
      await act(async () => input(window as unknown as Window, textarea, "只属于第一个任务"));
      const taskButtons = [...container.querySelectorAll<HTMLButtonElement>(".workflow-task-select")];
      await act(async () => click(window as unknown as Window, taskButtons[1]));
      expect(container.querySelector<HTMLTextAreaElement>("#workflow-progress-note")?.value).toBe("");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("treats persisted progress as successful when the follow-up snapshot refresh fails", async () => {
    const saved = { id: "progress-persisted", taskId: "task-1", note: "已落库的进展", createdAtMs: 8_000 };
    const load = vi.fn()
      .mockResolvedValueOnce(copySnapshot())
      .mockRejectedValue(new Error("快照刷新失败"));
    const addProgress = vi.fn().mockResolvedValue(saved);
    const { container, root, window } = await mount({ actionOverrides: { load, addProgress } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="progress"]')));
      const textarea = container.querySelector<HTMLTextAreaElement>("#workflow-progress-note")!;
      await act(async () => input(window as unknown as Window, textarea, saved.note));
      await act(async () => submit(window as unknown as Window, container.querySelector(".workflow-progress-form")));
      expect(addProgress).toHaveBeenCalledTimes(1);
      expect(textarea.value).toBe("");
      expect(container.textContent).toContain(saved.note);
      expect(container.querySelector(".workflow-feedback")?.textContent).toContain("已保存");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("cancels an incorrectly recognized AI task and reconciles released evidence", async () => {
    const automatic = copySnapshot();
    automatic.tasks[0] = {
      ...automatic.tasks[0],
      originKind: "ai",
      originKey: "cluster-1",
      originConfidence: .92,
      reviewState: "confirmed",
    };
    automatic.linkedEvidence[0] = {
      ...automatic.linkedEvidence[0],
      provenance: "ai",
      assignmentConfidence: .92,
    };
    const reconciled = copySnapshot(automatic);
    reconciled.tasks[0] = { ...reconciled.tasks[0], status: "cancelled" };
    reconciled.linkedEvidence = [];
    reconciled.summary = { ...reconciled.summary, taskCount: 0, linkedEvidenceCount: 0 };
    const load = vi.fn().mockResolvedValueOnce(automatic).mockResolvedValueOnce(reconciled);
    const cancelAiTask = vi.fn().mockResolvedValue({
      task: reconciled.tasks[0],
      releasedEvidenceCount: 1,
      dismissedClusterCount: 1,
      projectArchived: false,
    });
    const { actionSet, container, root, window } = await mount({ actionOverrides: { load, cancelAiTask } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="取消错误识别：构建工作台"]')));
      expect(actionSet.cancelAiTask).toHaveBeenCalledWith("task-1");
      expect(actionSet.load).toHaveBeenCalledTimes(2);
      expect(container.querySelector(".workflow-feedback")?.textContent).toContain("释放 1 条自动证据");
      expect(container.querySelector('button[aria-label="取消错误识别：构建工作台"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("does not let an old-date mutation overwrite the newer date snapshot", async () => {
    const assignment = deferred<boolean>();
    const secondDate = copySnapshot();
    secondDate.projects[0].name = "第二天项目";
    secondDate.tasks[0].title = "第二天任务";
    secondDate.unassignedEvidence = [];
    secondDate.suggestions = [];
    secondDate.summary = { ...secondDate.summary, unassignedEvidenceCount: 0, localSuggestionCount: 0 };
    const load = vi.fn()
      .mockResolvedValueOnce(copySnapshot())
      .mockResolvedValueOnce(secondDate)
      .mockResolvedValueOnce(secondDate);
    const assignEvidence = vi.fn().mockReturnValue(assignment.promise);
    const { container, render, root, window } = await mount({ actionOverrides: { load, assignEvidence } });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="evidence"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="分配证据：React documentation"]')));
      await render("2026-07-14");
      expect(container.textContent).toContain("第二天任务");

      await act(async () => assignment.resolve(true));
      expect(container.textContent).toContain("第二天任务");
      expect(container.textContent).not.toContain("React documentation");
      expect(container.textContent).not.toContain("证据已分配");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("opens the exact activity record in Today through a mounted interaction", async () => {
    const onOpenTimeline = vi.fn();
    const { container, render, root, window } = await mount();
    try {
      await render("2026-07-13", onOpenTimeline);
      await act(async () => click(window as unknown as Window, container.querySelector('button[data-inspector-tab="evidence"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('button[aria-label="在今日时间线中查看：WorkflowPage.tsx"]')));
      expect(onOpenTimeline).toHaveBeenCalledWith(expect.objectContaining({ kind: "activity", id: "segment-1" }));
      expect(container.textContent).toContain("react.dev");
      expect(container.textContent).toContain("未分类");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("keeps the dashboard responsive with one trapped modal surface and roving inspector tabs", async () => {
    const { container, document, root, window } = await mount({ mobile: true });
    try {
      const page = container.querySelector<HTMLElement>(".workflow-page")!;
      expect(page.dataset.workflowLayout).toBe("dashboard");
      expect(container.querySelector('select[aria-label="选择工作流"]')).not.toBeNull();
      expect(container.querySelector<HTMLElement>(".workflow-inspector")?.hidden).toBe(true);

      const newWorkflow = container.querySelector<HTMLButtonElement>('button[aria-label="新建工作流"]')!;
      newWorkflow.focus();
      await act(async () => click(window as unknown as Window, newWorkflow));
      expect(container.querySelectorAll('[aria-modal="true"]')).toHaveLength(1);
      expect(container.querySelector('[role="dialog"][aria-label="新建项目"]')).not.toBeNull();
      await act(async () => keydown(window as unknown as Window, container.querySelector('[role="dialog"][aria-label="新建项目"]')!, "Escape"));
      expect(container.querySelectorAll('[aria-modal="true"]')).toHaveLength(0);
      expect(document.activeElement).toBe(newWorkflow);

      const taskTrigger = container.querySelector<HTMLButtonElement>(".workflow-task-select")!;
      taskTrigger.focus();
      await act(async () => click(window as unknown as Window, taskTrigger));
      const taskDrawer = container.querySelector<HTMLElement>('.workflow-inspector[role="dialog"]')!;
      const overviewTab = taskDrawer.querySelector<HTMLButtonElement>('button[data-inspector-tab="overview"]')!;
      const progressTab = taskDrawer.querySelector<HTMLButtonElement>('button[data-inspector-tab="progress"]')!;
      const evidenceTab = taskDrawer.querySelector<HTMLButtonElement>('button[data-inspector-tab="evidence"]')!;
      expect(overviewTab.getAttribute("tabindex")).toBe("0");
      expect(progressTab.getAttribute("tabindex")).toBe("-1");
      expect(evidenceTab.getAttribute("tabindex")).toBe("-1");
      overviewTab.focus();
      await act(async () => keydown(window as unknown as Window, overviewTab, "ArrowRight"));
      expect(progressTab.getAttribute("aria-selected")).toBe("true");
      expect(document.activeElement).toBe(progressTab);

      await act(async () => keydown(window as unknown as Window, taskDrawer, "Escape"));
      expect(container.querySelectorAll('[aria-modal="true"]')).toHaveLength(0);
      expect(document.activeElement).toBe(taskTrigger);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("uses the responsive dashboard through the intermediate 760px width", async () => {
    const { container, root } = await mount({ viewportWidth: 760 });
    try {
      expect(container.querySelector<HTMLElement>(".workflow-page")?.dataset.workflowLayout).toBe("dashboard");
      expect(container.querySelector(".workflow-project-dashboard")).not.toBeNull();
      expect(container.querySelector<HTMLElement>(".workflow-inspector")?.hidden).toBe(true);
    } finally {
      await act(async () => root.unmount());
    }
  });

});
