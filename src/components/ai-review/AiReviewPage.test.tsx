import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AiReviewFilter, AiReviewRecord, AiReviewResolution } from "../../lib/desktop";
import { AiReviewPage, type AiReviewActions } from "./AiReviewPage";

type TestWindow = { Event: typeof Event };

function record(overrides: Partial<AiReviewRecord> = {}): AiReviewRecord {
  return {
    id: "pending-classification",
    kind: "classification",
    state: "pending",
    subjectId: "segment-1",
    beforeJson: JSON.stringify({ category: "pending", confidence: 0 }),
    proposedJson: JSON.stringify({
      category: "research",
      confidence: 0.76,
      reason: "编辑器标题与研究任务匹配",
      modelVersion: "review-model",
      candidates: [
        { label: "研究", score: 0.76, value: { category: "research", confidence: 0.76, reason: "候选一", modelVersion: "review-model", videoPurpose: "unknown" } },
        { label: "工作", score: 0.2, value: { category: "work", confidence: 0.2, reason: "候选二", modelVersion: "review-model", videoPurpose: "unknown" } },
      ],
    }),
    appliedJson: null,
    confidence: 0.76,
    evidenceSummary: "Code · AiReviewPage.tsx · 42 min",
    evidenceHash: "hash-classification",
    execution: {
      executionMode: "api-key",
      executorId: "openai",
      model: "gpt-4.1-mini",
      evidenceHash: "hash-classification",
      generation: 1,
      createdAtMs: 1_000,
      startedAtMs: 1_100,
      finishedAtMs: 1_500,
      durationMs: 400,
      exitCode: 0,
      errorKind: null,
      diagnostic: "",
    },
    createdAtMs: 1_000,
    resolvedAtMs: null,
    ...overrides,
  };
}

const records: AiReviewRecord[] = [
  record(),
  record({
    id: "pending-workflow",
    kind: "workflow_assignment",
    subjectId: "visit-1",
    evidenceHash: "hash-workflow",
    evidenceSummary: "react.dev · workflow evidence",
    proposedJson: JSON.stringify({
      evidenceKind: "browser",
      evidenceId: "visit-1",
      taskId: "task-1",
      confidence: 0.69,
      reason: "页面与任务匹配",
      createdAtMs: 1_000,
    }),
  }),
  record({ id: "auto", state: "auto_applied", appliedJson: record().proposedJson, evidenceHash: "hash-auto" }),
  record({
    id: "manual",
    state: "manual_override",
    appliedJson: JSON.stringify({ category: "work" }),
    evidenceHash: "hash-manual",
    execution: { ...record().execution, executionMode: null, executorId: null, model: null },
  }),
  record({ id: "dismissed", state: "dismissed", evidenceSummary: "用户忽略的历史记录", evidenceHash: "hash-dismissed" }),
  record({ id: "reverted", state: "reverted", evidenceSummary: "已撤销的历史记录", evidenceHash: "hash-reverted" }),
  record({
    id: "failure",
    state: "execution_error",
    proposedJson: "",
    confidence: null,
    evidenceHash: "hash-failure",
    execution: {
      ...record().execution,
      exitCode: 1,
      errorKind: "provider",
      diagnostic: "Authorization: Bearer secret https://api.example.test stderr: private output",
    },
  }),
];

function actions(): AiReviewActions {
  return {
    list: vi.fn().mockImplementation(async (filter: AiReviewFilter) => records.filter((item) => filter.states.length === 0 || filter.states.includes(item.state))),
    resolve: vi.fn().mockImplementation(async (request: AiReviewResolution) => records.filter((item) => request.reviewIds.includes(item.id))),
    revert: vi.fn().mockImplementation(async () => ({ ...records[2], state: "reverted" as const })),
    retry: vi.fn().mockResolvedValue(true),
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

async function mount(props: { subjectId?: string | null; actionSet?: AiReviewActions; onResolved?: (record: AiReviewRecord) => void } = {}) {
  const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
  let activeElement: Element | null = null;
  Object.defineProperty(document, "activeElement", { configurable: true, get: () => activeElement });
  Object.defineProperty(window.HTMLElement.prototype, "focus", { configurable: true, value(this: HTMLElement) { activeElement = this; } });
  Object.assign(window, {
    requestAnimationFrame: (callback: FrameRequestCallback) => { callback(0); return 1; },
    cancelAnimationFrame: () => undefined,
    confirm: vi.fn().mockReturnValue(true),
  });
  vi.stubGlobal("window", window);
  vi.stubGlobal("document", document);
  vi.stubGlobal("navigator", window.navigator);
  vi.stubGlobal("HTMLElement", window.HTMLElement);
  vi.stubGlobal("Node", window.Node);
  vi.stubGlobal("Event", window.Event);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const actionSet = props.actionSet ?? actions();
  const container = document.getElementById("root") as unknown as HTMLDivElement;
  const root = createRoot(container);
  await act(async () => root.render(<AiReviewPage subjectId={props.subjectId} actions={actionSet} onResolved={props.onResolved} />));
  await act(async () => undefined);
  return { actionSet, container, document, root, window };
}

function click(window: Window, element: Element | null) {
  if (!element) throw new Error("Expected element to exist");
  element.dispatchEvent(new (window as unknown as TestWindow).Event("click", { bubbles: true, cancelable: true }));
}

function change(window: Window, element: Element | null, value: string) {
  if (!element) throw new Error("Expected element to exist");
  if (element.localName === "select") {
    [...(element as HTMLSelectElement).options].forEach((option) => { option.selected = option.value === value; });
    Object.defineProperty(element, "value", { configurable: true, writable: true, value });
  } else {
    (element as HTMLInputElement).value = value;
  }
  element.dispatchEvent(new (window as unknown as TestWindow).Event("change", { bubbles: true }));
}

function keydown(window: Window, element: Element, key: string) {
  const event = new (window as unknown as TestWindow).Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "key", { value: key });
  element.dispatchEvent(event);
}

afterEach(() => vi.unstubAllGlobals());

describe("AiReviewPage", () => {
  it("renders five keyboard-accessible tabs and combines filters without losing the inspector", async () => {
    const { container, document, root, window } = await mount();
    try {
      const tabs = container.querySelectorAll<HTMLElement>('[role="tab"]');
      expect(tabs).toHaveLength(5);
      expect(tabs[0].getAttribute("tabindex")).toBe("0");
      await act(async () => keydown(window as unknown as Window, tabs[0], "ArrowRight"));
      expect(document.activeElement).toBe(tabs[1]);
      expect(tabs[1].getAttribute("aria-selected")).toBe("true");

      await act(async () => click(window as unknown as Window, tabs[0]));
      await act(async () => change(window as unknown as Window, container.querySelector('[aria-label="审核类型"]'), "classification"));
      await act(async () => change(window as unknown as Window, container.querySelector('[aria-label="执行方式"]'), "api-key"));
      await act(async () => change(window as unknown as Window, container.querySelector('[aria-label="执行者"]'), "openai"));
      expect(container.querySelectorAll(".ai-review-row")).toHaveLength(1);
      expect(container.querySelector(".ai-review-inspector")).not.toBeNull();

      await act(async () => click(window as unknown as Window, tabs[4]));
      expect(container.querySelector('[aria-label="刷新审核记录"]')).toBeNull();
      expect(container.textContent).toContain("执行队列");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("shows evidence, hashes, candidate scores, and manual records as human changes", async () => {
    const { container, root, window } = await mount();
    try {
      expect(container.textContent).toContain("Code · AiReviewPage.tsx · 42 min");
      expect(container.textContent).toContain("hash-classification");
      expect(container.textContent).toContain("研究");
      expect(container.textContent).toContain("76%");

      const manualTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((tab) => tab.textContent?.includes("人工改写"));
      await act(async () => click(window as unknown as Window, manualTab ?? null));
      expect(container.textContent).toContain("人工修改");
      expect(container.textContent).not.toContain("API Key · 人工修改");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("keeps dismissed and reverted records in the four-tab manual history without illegal actions", async () => {
    const { container, root, window } = await mount();
    try {
      const manualTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((item) => item.textContent?.includes("人工改写"));
      await act(async () => click(window as unknown as Window, manualTab ?? null));
      expect(container.querySelectorAll(".ai-review-row")).toHaveLength(3);
      expect(container.textContent).toContain("已忽略");
      expect(container.textContent).toContain("已撤销");

      const dismissedRow = [...container.querySelectorAll<HTMLButtonElement>(".ai-review-open")].find((button) => button.textContent?.includes("用户忽略的历史记录"));
      await act(async () => click(window as unknown as Window, dismissedRow ?? null));
      expect(container.querySelector(".ai-review-actions")?.querySelectorAll("button")).toHaveLength(0);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("sanitizes execution diagnostics and preserves successful retry semantics", async () => {
    const onResolved = vi.fn();
    const { actionSet, container, root, window } = await mount({ onResolved });
    try {
      const errorTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((tab) => tab.textContent?.includes("执行失败"));
      await act(async () => click(window as unknown as Window, errorTab ?? null));
      expect(container.textContent).not.toContain("secret");
      expect(container.textContent).not.toContain("api.example.test");
      expect(container.textContent).not.toContain("private output");

      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="按原模式重试"]')));
      expect(actionSet.retry).toHaveBeenLastCalledWith("failure", false);
      expect(container.querySelector('[role="status"]')?.textContent).toContain("已按原模式重新入队");
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="按当前模式重试"]')));
      expect(actionSet.retry).toHaveBeenLastCalledWith("failure", true);
      expect(container.querySelector('[role="status"]')?.textContent).toContain("已按当前模式重新入队");
      expect(onResolved).toHaveBeenCalledTimes(2);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it.each([
    ["original", "按原模式重试", false],
    ["current", "按当前模式重试", true],
  ])("does not refresh or report success when the %s retry creates no task", async (_label, buttonLabel, useCurrentMode) => {
    const actionSet = actions();
    actionSet.retry = vi.fn().mockResolvedValue(false);
    const onResolved = vi.fn();
    const { container, root, window } = await mount({ actionSet, onResolved });
    try {
      const errorTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((item) => item.textContent?.includes("执行失败"));
      await act(async () => click(window as unknown as Window, errorTab ?? null));
      const listCallsBeforeRetry = vi.mocked(actionSet.list).mock.calls.length;

      await act(async () => click(window as unknown as Window, container.querySelector(`[aria-label="${buttonLabel}"]`)));

      expect(actionSet.retry).toHaveBeenCalledWith("failure", useCurrentMode);
      expect(actionSet.list).toHaveBeenCalledTimes(listCallsBeforeRetry);
      expect(onResolved).not.toHaveBeenCalled();
      expect(container.querySelector('[role="status"]')?.textContent).toContain("未创建新任务/无需重复入队");
      expect(container.textContent).not.toContain("已按");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("binds accept, alternate change, ignore, and same-kind batch accept to evidence hashes", async () => {
    const actionSet = actions();
    const { container, root, window } = await mount({ actionSet });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="接受建议"]')));
      expect(actionSet.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ action: "accept", reviewIds: ["pending-classification"], evidenceHashes: { "pending-classification": "hash-classification" } }));

      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="应用替代值"]')));
      expect(actionSet.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ action: "change", changedJson: expect.stringContaining('"category":"research"') }));

      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="忽略建议"]')));
      expect(actionSet.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ action: "ignore" }));

      const selected = container.querySelector('[aria-label="选择审核 pending-classification"]');
      await act(async () => click(window as unknown as Window, selected));
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="批量接受同类"]')));
      expect(actionSet.resolve).toHaveBeenLastCalledWith(expect.objectContaining({ action: "accept", reviewIds: ["pending-classification"] }));
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("rejects mixed-kind batches and confirms auto-apply reverts", async () => {
    const actionSet = actions();
    const { container, root, window } = await mount({ actionSet });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="选择审核 pending-classification"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="选择审核 pending-workflow"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="批量接受同类"]')));
      expect(container.querySelector('[role="alert"]')?.textContent).toContain("同一种审核类型");
      expect(actionSet.resolve).not.toHaveBeenCalled();

      const autoTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((tab) => tab.textContent?.includes("已自动应用"));
      await act(async () => click(window as unknown as Window, autoTab ?? null));
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="撤销自动应用"]')));
      expect(window.confirm).toHaveBeenCalled();
      expect(actionSet.revert).toHaveBeenCalledWith("auto");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("intersects selected ids with visible records before batch accept", async () => {
    const actionSet = actions();
    const { container, root, window } = await mount({ actionSet });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="选择审核 pending-classification"]')));
      expect(container.querySelector(".ai-review-batchbar")?.textContent).toContain("已选择 1 项");

      await act(async () => change(window as unknown as Window, container.querySelector('[aria-label="审核类型"]'), "workflow_assignment"));
      expect(container.querySelector(".ai-review-batchbar")?.textContent).toContain("已选择 0 项");
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="选择审核 pending-workflow"]')));
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="批量接受同类"]')));

      expect(actionSet.resolve).toHaveBeenCalledWith(expect.objectContaining({ reviewIds: ["pending-workflow"], action: "accept" }));
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("commits only the newest list response when filters race", async () => {
    const first = deferred<AiReviewRecord[]>();
    const second = deferred<AiReviewRecord[]>();
    const actionSet = actions();
    actionSet.list = vi.fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const { container, root, window } = await mount({ actionSet });
    try {
      await act(async () => change(window as unknown as Window, container.querySelector('[aria-label="审核类型"]'), "workflow_assignment"));
      expect(actionSet.list).toHaveBeenCalledTimes(2);

      await act(async () => second.resolve([records[1]]));
      expect(container.textContent).toContain("react.dev · workflow evidence");
      await act(async () => first.resolve([records[0]]));
      expect(container.textContent).toContain("react.dev · workflow evidence");
      expect(container.querySelectorAll(".ai-review-row")).toHaveLength(1);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("ignores a deferred list response after unmount", async () => {
    const pending = deferred<AiReviewRecord[]>();
    const actionSet = actions();
    actionSet.list = vi.fn().mockReturnValue(pending.promise);
    const { container, root } = await mount({ actionSet });

    await act(async () => root.unmount());
    await act(async () => pending.resolve([records[0]]));

    expect(container.childNodes).toHaveLength(0);
  });

  it("does not refresh or notify when resolve completes after unmount", async () => {
    const pendingResolve = deferred<AiReviewRecord[]>();
    const actionSet = actions();
    actionSet.resolve = vi.fn().mockReturnValue(pendingResolve.promise);
    const onResolved = vi.fn();
    const { actionSet: mountedActions, container, root, window } = await mount({ actionSet, onResolved });
    const listCallsBeforeAction = vi.mocked(mountedActions.list).mock.calls.length;

    await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="接受建议"]')));
    await act(async () => root.unmount());
    await act(async () => pendingResolve.resolve([{ ...records[0], state: "manual_override" }]));

    expect(mountedActions.list).toHaveBeenCalledTimes(listCallsBeforeAction);
    expect(onResolved).not.toHaveBeenCalled();
  });

  it("does not refresh or notify when revert completes after unmount", async () => {
    const pendingRevert = deferred<AiReviewRecord>();
    const actionSet = actions();
    actionSet.revert = vi.fn().mockReturnValue(pendingRevert.promise);
    const onResolved = vi.fn();
    const { actionSet: mountedActions, container, root, window } = await mount({ actionSet, onResolved });
    const autoTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((item) => item.textContent?.includes("已自动应用"));
    await act(async () => click(window as unknown as Window, autoTab ?? null));
    const listCallsBeforeAction = vi.mocked(mountedActions.list).mock.calls.length;

    await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="撤销自动应用"]')));
    await act(async () => root.unmount());
    await act(async () => pendingRevert.resolve({ ...records[2], state: "reverted" }));

    expect(mountedActions.list).toHaveBeenCalledTimes(listCallsBeforeAction);
    expect(onResolved).not.toHaveBeenCalled();
  });

  it("does not refresh or notify when retry completes after unmount", async () => {
    const pendingRetry = deferred<boolean>();
    const actionSet = actions();
    actionSet.retry = vi.fn().mockReturnValue(pendingRetry.promise);
    const onResolved = vi.fn();
    const { actionSet: mountedActions, container, root, window } = await mount({ actionSet, onResolved });
    const errorTab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((item) => item.textContent?.includes("执行失败"));
    await act(async () => click(window as unknown as Window, errorTab ?? null));
    const listCallsBeforeAction = vi.mocked(mountedActions.list).mock.calls.length;

    await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="按原模式重试"]')));
    await act(async () => root.unmount());
    await act(async () => pendingRetry.resolve(true));

    expect(mountedActions.list).toHaveBeenCalledTimes(listCallsBeforeAction);
    expect(onResolved).not.toHaveBeenCalled();
  });

  it("rejects malformed editor JSON and invalid candidates before invoking the backend", async () => {
    const actionSet = actions();
    const { container, root, window } = await mount({ actionSet });
    try {
      const editor = container.querySelector<HTMLTextAreaElement>('[aria-label="替代 JSON"]')!;
      editor.value = "not-json";
      await act(async () => editor.dispatchEvent(new (window as unknown as TestWindow).Event("input", { bubbles: true })));
      expect(container.querySelector(".ai-review-json-error")?.textContent).toContain("不是有效对象");
      expect(container.querySelector<HTMLButtonElement>('[aria-label="应用替代值"]')?.disabled).toBe(true);
      expect(actionSet.resolve).not.toHaveBeenCalled();

      const candidateRadios = container.querySelectorAll<HTMLInputElement>('[name="candidate-pending-classification"]');
      candidateRadios[1].checked = true;
      await act(async () => click(window as unknown as Window, candidateRadios[1]));
      expect(container.querySelector(".ai-review-json-error")?.textContent).toContain("活动分类");
      expect(container.querySelector<HTMLButtonElement>('[aria-label="应用替代值"]')?.disabled).toBe(true);
      expect(actionSet.resolve).not.toHaveBeenCalled();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("preserves duplicate-subject and stale-evidence backend errors verbatim", async () => {
    const actionSet = actions();
    actionSet.resolve = vi.fn().mockRejectedValue(new Error("Duplicate review subject: stale evidence hash"));
    const { container, root, window } = await mount({ actionSet });
    try {
      await act(async () => click(window as unknown as Window, container.querySelector('[aria-label="接受建议"]')));
      expect(container.querySelector('[role="alert"]')?.textContent).toBe("Duplicate review subject: stale evidence hash");
      expect(actionSet.resolve).toHaveBeenCalledTimes(1);
    } finally {
      await act(async () => root.unmount());
    }
  });
});
