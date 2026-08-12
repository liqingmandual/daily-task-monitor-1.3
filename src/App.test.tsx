import { renderToStaticMarkup } from "react-dom/server";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import * as aiReviewLib from "./lib/ai-review";
import { activityDisplayRegistry } from "./lib/activity-composition";
import type { AiConnectionHealth, AiReviewRecord, CollectionHealth } from "./lib/desktop";
import type { Segment } from "./lib/metrics";

type TestWindow = {
  Event: typeof Event;
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

vi.mock("echarts", () => ({
  init: () => ({
    setOption: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
  }),
}));

vi.mock("echarts/core", () => ({
  use: vi.fn(),
  init: () => ({
    setOption: vi.fn(),
    on: vi.fn(),
    off: vi.fn(),
    dispatchAction: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
  }),
}));

vi.mock("echarts/charts", () => ({ PieChart: {} }));
vi.mock("echarts/components", () => ({ TooltipComponent: {} }));
vi.mock("echarts/renderers", () => ({ SVGRenderer: {} }));

const segments: Segment[] = [
  {
    id: "research",
    startMs: 8 * 3_600_000,
    endMs: 9 * 3_600_000,
    app: "Chrome",
    title: "Psychology research",
    category: "research",
    videoPurpose: "unknown",
    confidence: 0.94,
    needsReview: false,
  },
  {
    id: "social",
    startMs: 9 * 3_600_000,
    endMs: 9.5 * 3_600_000,
    app: "WeChat",
    title: "微信",
    category: "social",
    videoPurpose: "unknown",
    confidence: 0.98,
    needsReview: false,
  },
];

const appSettings = {
  idleThresholdMinutes: 6,
  monitoringEnabled: true,
  aiBackfillEnabled: false,
  aiExecutionMode: "codex",
  selectedApiProviderId: null,
  codexExecutable: "codex",
  codexModel: "",
  aiAutoResearchAnalysisEnabled: true,
  aiAutoClassificationEnabled: true,
  aiAutoWorkflowAssignmentEnabled: true,
  aiAutomationNoticeVersion: 0,
  excludedApps: [],
  excludedDomains: [],
  uiTheme: "classic-workbench",
  experimentalKnowledgeGraphEnabled: false,
} as const;

afterEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockReset();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function pendingClassificationReview(): AiReviewRecord {
  return {
    id: "pending-classification",
    kind: "classification",
    state: "pending",
    subjectId: "research",
    beforeJson: JSON.stringify({ category: "pending", confidence: 0 }),
    proposedJson: JSON.stringify({ category: "research", confidence: 0.92 }),
    appliedJson: null,
    confidence: 0.92,
    evidenceSummary: "Code | App.tsx | 60 min",
    evidenceHash: "hash-pending-classification",
    execution: {
      executionMode: "api-key",
      executorId: "openai",
      model: "gpt-test",
      evidenceHash: "hash-pending-classification",
      generation: 1,
      createdAtMs: 1,
      startedAtMs: 2,
      finishedAtMs: 3,
      durationMs: 1,
      exitCode: 0,
      errorKind: null,
      diagnostic: "",
    },
    createdAtMs: 1,
    resolvedAtMs: null,
  };
}

function aiConnectionHealth(overrides: Partial<AiConnectionHealth> = {}): AiConnectionHealth {
  return {
    executionMode: "api-key",
    executorId: "openai",
    executorLabel: "OpenAI",
    model: "gpt-test",
    status: "healthy",
    verificationLevel: "inference",
    source: "manual",
    checkedAtMs: 1_752_537_600_000,
    verifiedAtMs: 1_752_537_600_000,
    diagnostic: null,
    ...overrides,
  };
}

function collectionHealth(overrides: Partial<CollectionHealth> = {}): CollectionHealth {
  const healthy = { status: "healthy" as const, lastSuccessAtMs: 1_752_537_600_000, detail: "采集正常" };
  return {
    generatedAtMs: 1_752_537_600_000,
    platform: "macos",
    monitoringEnabled: true,
    desktop: healthy,
    windowTitle: healthy,
    idle: healthy,
    continuity: healthy,
    screenRecording: healthy,
    browserWatcher: healthy,
    browserHistory: healthy,
    watcherEndpoint: "http://127.0.0.1:27123/v1/heartbeat",
    watcherToken: "local-test-token",
    watcherSourceCount: 1,
    measuredBrowserSliceCount: 4,
    measuredBrowserSeconds: 120,
    ...overrides,
  };
}

function installDesktopWindow() {
  const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
  let activeElement: Element | null = null;
  const scrollIntoView = vi.fn();
  Object.defineProperty(document, "activeElement", {
    configurable: true,
    get: () => activeElement,
  });
  Object.defineProperty(window.HTMLElement.prototype, "focus", {
    configurable: true,
    value(this: HTMLElement) { activeElement = this; },
  });
  Object.defineProperty(window.HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scrollIntoView,
  });
  Object.assign(window, {
    __TAURI_INTERNALS__: {},
    localStorage: { getItem: () => null, setItem: () => undefined, removeItem: () => undefined },
    getComputedStyle: () => ({ getPropertyValue: () => "400px", width: "400px", height: "300px" }),
    requestAnimationFrame: (callback: FrameRequestCallback) => { callback(0); return 0; },
    cancelAnimationFrame: () => undefined,
    confirm: () => true,
  });
  vi.stubGlobal("window", window);
  vi.stubGlobal("document", document);
  vi.stubGlobal("navigator", window.navigator);
  vi.stubGlobal("HTMLElement", window.HTMLElement);
  vi.stubGlobal("Node", window.Node);
  vi.stubGlobal("Event", window.Event);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  return { document, window, scrollIntoView };
}

function installPreviewWindow() {
  const installed = installDesktopWindow();
  Object.assign(installed.window, { __TAURI_INTERNALS__: undefined });
  return installed;
}

function installMacDesktopWindow() {
  const installed = installDesktopWindow();
  vi.stubGlobal("navigator", {
    ...installed.window.navigator,
    userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
  });
  return installed;
}

function desktopCommandResult(command: string) {
  if (command === "get_settings") return { ...appSettings, aiAutomationNoticeVersion: 1 };
  if (command === "get_ai_connection_health" || command === "refresh_ai_connection_health") return aiConnectionHealth();
  if (command === "get_collection_health") return collectionHealth();
  if (command === "get_today_dashboard") return {
    timeline: [],
    totals: { monitoredSeconds: 0, activeSeconds: 0, idleSeconds: 0, learningSeconds: 0, categorySeconds: {} },
    workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
  };
  if (command === "get_daily_goal") return { date: "2026-07-15", goals: "", expectedOutput: "", actualOutput: "" };
  if (command === "list_daily_goal_task_links" || command === "list_ai_providers" || command === "get_browser_sources") return [];
  if (command === "get_work_ledger") return {
    projects: [], tasks: [], progress: [], linkedEvidence: [], unassignedEvidence: [], suggestions: [], ambiguousEvidenceHashes: [],
    summary: { projectCount: 0, taskCount: 0, progressCount: 0, linkedEvidenceCount: 0, unassignedEvidenceCount: 0, localSuggestionCount: 0, ambiguousEvidenceCount: 0 },
  };
  if (command === "get_daily_analysis") throw new Error("no saved analysis");
  return null;
}

function keydown(window: Window, target: EventTarget, key: string, shiftKey = false) {
  const event = new (window as unknown as TestWindow).Event("keydown", { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    key: { value: key },
    shiftKey: { value: shiftKey },
  });
  target.dispatchEvent(event);
  return event;
}

describe("App", () => {
  it("uses the compact desktop header at the default 1360px window width", async () => {
    const { document, window } = installDesktopWindow();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1360 });
    vi.mocked(invoke).mockImplementation(async (command) => desktopCommandResult(command));
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));

      expect(rootElement.querySelector(".app-frame")?.getAttribute("data-header-layout")).toBe("compact");
      expect(rootElement.querySelectorAll(".main-tabs button")).toHaveLength(5);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("renders evidence-based portrait and recommendation sections", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);

    expect(html).toContain('class="panel ai-analysis-panel"');
    expect(html).toContain("事实观察");
    expect(html).toContain("如何验证");
    expect(html).toContain("预期产出");
    expect(html).toContain("实际产出");
  });

  it("renders the data-first today workspace", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);

    expect(html).toContain("活动构成");
    expect(html).toContain("时间分布");
    expect(html).toContain("应用排行");
    expect(html).toContain("活动时间线");
    expect(html).toContain("搜索/调研");
    expect(html).toContain("Chrome");
  });

  it("shows only the four today time-structure metrics with monitored-time shares", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);
    const { document } = parseHTML(html);
    const metrics = [...document.querySelectorAll("[aria-label='今日核心指标'] .metric-card")];
    const text = metrics.map((card) => card.textContent ?? "");

    expect(metrics).toHaveLength(4);
    expect(text[0]).toContain("活跃");
    expect(text[0]).toContain("占总监测 100.0%");
    expect(text[1]).toContain("学习");
    expect(text[1]).toContain("占总监测 66.7%");
    expect(text[2]).toContain("最长专注");
    expect(text[2]).toContain("占总监测 66.7%");
    expect(text[3]).toContain("切换频率");
    expect(text[3]).toContain("次/小时");
    expect(text[3]).toContain("共 1 次");
    expect(text.join(" ")).not.toContain("切换负荷");
  });

  it("shows an unavailable share instead of NaN when today has no monitored time", () => {
    const html = renderToStaticMarkup(<App initialSegments={[]} />);
    expect(html).toContain("占总监测 —");
    expect(html).not.toContain("NaN");
    expect(html).not.toContain("Infinity");
  });

  it("keeps operational details behind the settings action", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);

    expect(html).toContain("<h1>Orbit</h1>");
    expect(html).not.toContain("每日任务监测系统");
    expect(html).toContain("aria-label=\"打开设置\"");
    expect(html).not.toContain("高级与校准</h2>");
  });

  it("opens settings from the native macOS menu event", async () => {
    const { document } = installMacDesktopWindow();
    let openSettings: (() => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "open-settings") openSettings = handler as () => void;
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command) => desktopCommandResult(command));
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      expect(rootElement.querySelector('button[aria-label="打开设置"]')).toBeNull();
      await act(async () => openSettings?.());

      expect(openSettings).toBeTypeOf("function");
      expect(rootElement.querySelector('[role="dialog"][aria-label="设置"]')).not.toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("renders the classic theme, stable analysis order, and compact visual summaries", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);

    expect(html).toContain('data-theme="classic-workbench"');
    expect(html).toContain('data-scroll-container="dashboard"');
    expect((html.match(/data-time-bucket=/g) ?? []).length).toBe(12);
    expect((html.match(/class="donut-center"/g) ?? []).length).toBe(2);
    expect(html).toContain('data-analysis-layout="three-column"');
    expect(html).toContain('class="two-hour-chart"');
    expect(html).toContain("120 min");
    expect(html).not.toContain('class="time-detail-rail"');
    expect(html).toContain('aria-controls="activity-timeline"');
    expect((html.match(/data-interaction-model="hover-preview-click-select"/g) ?? []).length).toBe(3);
    expect(html).toContain('data-app-icon="chrome"');
    expect(html).toContain("使用左右方向键选择");
    expect(html).toContain('class="time-series-bar series-active"');
    expect(html).toContain('class="time-series-bar series-learning"');

    const distributionIndex = html.indexOf("distribution-panel");
    const appsIndex = html.indexOf("apps-panel");
    const timeIndex = html.indexOf("time-panel");
    expect(distributionIndex).toBeLessThan(appsIndex);
    expect(appsIndex).toBeLessThan(timeIndex);
  });

  it("composes Today in the approved heading-to-analysis order", () => {
    const html = renderToStaticMarkup(<App initialSegments={segments} />);

    expect(html).toContain('data-analysis-layout="three-column"');
    expect(html).toContain("120 min");
    expect(html).toContain("事实观察");
    expect(html).toContain("如何验证");
    expect(html).not.toContain("今日行动队列");
    expect(html).toContain('aria-label="打开专注工具"');

    const headingIndex = html.indexOf('class="page-heading"');
    const metricsIndex = html.indexOf('class="metric-grid"');
    const analysisIndex = html.indexOf('data-analysis-layout="three-column"');
    const timelineIndex = html.indexOf('id="activity-timeline"');
    const goalIndex = html.indexOf('class="goal-grid"');
    const aiAnalysisIndex = html.indexOf('class="panel ai-analysis-panel"');

    expect(headingIndex).toBeGreaterThanOrEqual(0);
    expect(metricsIndex).toBeGreaterThanOrEqual(0);
    expect(analysisIndex).toBeGreaterThanOrEqual(0);
    expect(timelineIndex).toBeGreaterThanOrEqual(0);
    expect(goalIndex).toBeGreaterThanOrEqual(0);
    expect(aiAnalysisIndex).toBeGreaterThanOrEqual(0);
    expect(headingIndex).toBeLessThan(metricsIndex);
    expect(metricsIndex).toBeLessThan(analysisIndex);
    expect(analysisIndex).toBeLessThan(timelineIndex);
    expect(timelineIndex).toBeLessThan(goalIndex);
    expect(goalIndex).toBeLessThan(aiAnalysisIndex);
  });

  it("shows the first-run AI automation notice and persists acknowledgement", async () => {
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
    Object.assign(window, {
      __TAURI_INTERNALS__: {},
      localStorage: {
        getItem: () => null,
        setItem: () => undefined,
        removeItem: () => undefined,
      },
      getComputedStyle: () => ({
        getPropertyValue: () => "400px",
        width: "400px",
        height: "300px",
      }),
      requestAnimationFrame: (callback: FrameRequestCallback) => {
        callback(0);
        return 0;
      },
      cancelAnimationFrame: () => undefined,
    });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_settings") return appSettings;
      if (command === "list_ai_providers" || command === "get_browser_sources") return [];
      if (command === "get_today_dashboard") return {
        timeline: [],
        totals: { monitoredSeconds: 0, activeSeconds: 0, idleSeconds: 0, learningSeconds: 0, categorySeconds: {} },
        workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
      };
      if (command === "get_daily_goal") return {
        date: "2026-07-14",
        goals: "继续完善桌面版任务监测系统",
        expectedOutput: "完成分类、采集和首页数据结构",
        actualOutput: "",
      };
      if (command === "list_daily_goal_task_links") return [];
      if (command === "get_work_ledger") return {
        projects: [],
        tasks: [],
        progress: [],
        linkedEvidence: [],
        unassignedEvidence: [],
        suggestions: [],
        ambiguousEvidenceHashes: [],
        summary: {
          projectCount: 0,
          taskCount: 0,
          progressCount: 0,
          linkedEvidenceCount: 0,
          unassignedEvidenceCount: 0,
          localSuggestionCount: 0,
          ambiguousEvidenceCount: 0,
        },
      };
      if (command === "get_daily_analysis") throw new Error("no saved analysis");
      if (command === "update_settings") return { ...appSettings, ...(args as { patch: Partial<typeof appSettings> }).patch };
      return null;
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={segments} />);
      });
      await act(async () => undefined);

      expect(rootElement.textContent).toContain("AI 自动化首次说明");
      expect(rootElement.textContent).toContain("本机 Codex");
      expect(rootElement.textContent).toContain("深度分析自动化");
      const noticeDialog = rootElement.querySelector<HTMLElement>('[role="dialog"][aria-modal="true"]');
      const noticeButtons = Array.from(rootElement.querySelectorAll<HTMLButtonElement>("button[aria-label=\"关闭 AI 自动化说明\"]"));
      const closeButton = noticeButtons[0];
      const acknowledgeButton = noticeButtons.at(-1);
      expect(noticeDialog).not.toBeNull();
      expect(noticeButtons).toHaveLength(2);
      expect(document.activeElement).toBe(noticeDialog);

      acknowledgeButton?.focus();
      const forwardTab = keydown(window as unknown as Window, document, "Tab");
      expect(forwardTab.defaultPrevented).toBe(true);
      expect(document.activeElement).toBe(closeButton);

      closeButton?.focus();
      const backwardTab = keydown(window as unknown as Window, document, "Tab", true);
      expect(backwardTab.defaultPrevented).toBe(true);
      expect(document.activeElement).toBe(acknowledgeButton);

      await act(async () => {
        closeButton?.dispatchEvent(new window.Event("click", { bubbles: true }));
      });

      expect(invoke).toHaveBeenCalledWith("update_settings", {
        patch: { aiAutomationNoticeVersion: 1 },
      });
      expect(rootElement.textContent).not.toContain("AI 自动化首次说明");
      const tabAfterClose = keydown(window as unknown as Window, document, "Tab");
      expect(tabAfterClose.defaultPrevented).toBe(false);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("acknowledges the AI automation notice in preview without native persistence", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    Object.assign(window, {
      __TAURI_INTERNALS__: undefined,
      localStorage: {
        getItem: () => null,
        setItem: () => undefined,
        removeItem: () => undefined,
      },
      getComputedStyle: () => ({
        getPropertyValue: () => "400px",
        width: "400px",
        height: "300px",
      }),
      requestAnimationFrame: (callback: FrameRequestCallback) => {
        callback(0);
        return 0;
      },
      cancelAnimationFrame: () => undefined,
    });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={segments} />);
      });
      await act(async () => undefined);

      const acknowledgeButton = Array.from(rootElement.querySelectorAll<HTMLButtonElement>(
        "button[aria-label=\"关闭 AI 自动化说明\"]",
      )).at(-1);
      expect(acknowledgeButton).toBeDefined();

      await act(async () => {
        acknowledgeButton?.dispatchEvent(new window.Event("click", { bubbles: true }));
      });

      expect(rootElement.textContent).not.toContain("AI 自动化首次说明");
      expect(invoke).not.toHaveBeenCalled();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("loads exact preview review markers and removes them after resolution without changing the date", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    Object.assign(window, {
      __TAURI_INTERNALS__: undefined,
      localStorage: {
        getItem: () => null,
        setItem: () => undefined,
        removeItem: () => undefined,
      },
      getComputedStyle: () => ({
        getPropertyValue: () => "400px",
        width: "400px",
        height: "300px",
      }),
      requestAnimationFrame: (callback: FrameRequestCallback) => { callback(0); return 0; },
      cancelAnimationFrame: () => undefined,
    });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);
    const previewSegments = [{ ...segments[0], id: "2", needsReview: false }];

    try {
      await act(async () => root.render(<App initialSegments={previewSegments} />));
      await act(async () => undefined);
      const acknowledgeButton = Array.from(rootElement.querySelectorAll<HTMLButtonElement>(
        'button[aria-label="关闭 AI 自动化说明"]',
      )).at(-1);
      await act(async () => acknowledgeButton?.dispatchEvent(new window.Event("click", { bubbles: true })));

      const selectedDate = rootElement.querySelector<HTMLInputElement>('input[aria-label="选择日期"]')?.value;
      const marker = rootElement.querySelector<HTMLButtonElement>('[data-ai-review-subject="2"]');
      expect(marker).not.toBeNull();
      await act(async () => marker?.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(rootElement.textContent).toContain("AI 审核中心");

      await act(async () => rootElement.querySelector<HTMLButtonElement>('[aria-label="接受建议"]')?.dispatchEvent(new window.Event("click", { bubbles: true })));
      const todayButton = [...rootElement.querySelectorAll<HTMLButtonElement>(".main-tabs button")].find((button) => button.textContent === "今日");
      await act(async () => todayButton?.dispatchEvent(new window.Event("click", { bubbles: true })));

      expect(rootElement.querySelector('[data-ai-review-subject="2"]')).toBeNull();
      expect(rootElement.querySelector<HTMLInputElement>('input[aria-label="选择日期"]')?.value).toBe(selectedDate);
      expect(invoke).not.toHaveBeenCalled();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("shows and refreshes the structured Codex health controls without hiding providers", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    Object.assign(window, {
      __TAURI_INTERNALS__: {},
      localStorage: {
        getItem: () => null,
        setItem: () => undefined,
        removeItem: () => undefined,
      },
      getComputedStyle: () => ({
        getPropertyValue: () => "400px",
        width: "400px",
        height: "300px",
      }),
      requestAnimationFrame: (callback: FrameRequestCallback) => {
        callback(0);
        return 0;
      },
      cancelAnimationFrame: () => undefined,
    });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);

    const settings = { ...appSettings, aiAutomationNoticeVersion: 1 };
    const healthStates = [
      {
        configuredPath: "codex",
        detectedPath: "C:\\Tools\\codex.exe",
        version: "codex-cli 1.2.3",
        checkedAtMs: 1_752_537_600_000,
        status: "permission-denied",
        diagnostic: "WindowsApps access denied",
      },
      {
        configuredPath: "codex",
        detectedPath: null,
        version: null,
        checkedAtMs: 1_752_537_601_000,
        status: "unavailable",
        diagnostic: "codex was not found",
      },
      {
        configuredPath: "codex",
        detectedPath: "C:\\Tools\\codex.exe",
        version: null,
        checkedAtMs: 1_752_537_602_000,
        status: "timed-out",
        diagnostic: "version check timed out",
      },
    ] as const;
    let refreshIndex = 0;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_settings") return settings;
      if (command === "get_codex_health") return healthStates[0];
      if (command === "test_codex_cli") {
        refreshIndex += 1;
        return healthStates[refreshIndex];
      }
      if (command === "list_ai_providers") return [{
        id: "openai",
        name: "OpenAI",
        baseUrl: "https://api.openai.com/v1",
        model: "gpt-test",
        enabled: true,
        priority: 0,
        hasCredential: true,
      }];
      if (command === "get_browser_sources") return [];
      if (command === "get_today_dashboard") return {
        timeline: [],
        totals: { monitoredSeconds: 0, activeSeconds: 0, idleSeconds: 0, learningSeconds: 0, categorySeconds: {} },
        workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
      };
      if (command === "get_daily_goal") return {
        date: "2026-07-15",
        goals: "继续桌面重写",
        expectedOutput: "完成 Task 3",
        actualOutput: "",
      };
      if (command === "list_daily_goal_task_links") return [];
      if (command === "get_work_ledger") return {
        projects: [],
        tasks: [],
        progress: [],
        linkedEvidence: [],
        unassignedEvidence: [],
        suggestions: [],
        ambiguousEvidenceHashes: [],
        summary: {
          projectCount: 0,
          taskCount: 0,
          progressCount: 0,
          linkedEvidenceCount: 0,
          unassignedEvidenceCount: 0,
          localSuggestionCount: 0,
          ambiguousEvidenceCount: 0,
        },
      };
      if (command === "update_settings") return { ...settings, ...(args as { patch: Partial<typeof settings> }).patch };
      if (command === "get_daily_analysis") throw new Error("no saved analysis");
      return null;
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={segments} />);
      });
      await act(async () => undefined);
      await act(async () => {
        rootElement.querySelector<HTMLButtonElement>('button[aria-label="打开设置"]')
          ?.dispatchEvent(new window.Event("click", { bubbles: true }));
      });
      await act(async () => undefined);

      const modeButtons = Array.from(rootElement.querySelectorAll<HTMLButtonElement>(".ai-mode-switch button"));
      expect(modeButtons.map((button) => button.textContent)).toEqual(["API Key", "本机 Codex"]);
      expect(modeButtons[1]?.getAttribute("aria-pressed")).toBe("true");
      expect(rootElement.querySelector<HTMLInputElement>('#codex-executable')?.value).toBe("codex");
      expect(rootElement.querySelector<HTMLInputElement>('#codex-model')?.value).toBe("");
      expect(rootElement.textContent).toContain("C:\\Tools\\codex.exe");
      expect(rootElement.textContent).toContain("codex-cli 1.2.3");
      expect(rootElement.textContent).toContain("权限被拒绝");
      expect(rootElement.textContent).toContain("OpenAI");
      expect(rootElement.textContent).toContain("gpt-test");
      expect(rootElement.querySelector('[data-codex-health-status="permission-denied"] svg')).not.toBeNull();

      const refreshButton = rootElement.querySelector<HTMLButtonElement>('button[aria-label="刷新并测试 Codex"]');
      expect(refreshButton?.title).toBe("刷新并测试 Codex");
      await act(async () => {
        refreshButton?.dispatchEvent(new window.Event("click", { bubbles: true }));
      });
      expect(rootElement.textContent).toContain("不可用");
      expect(rootElement.querySelector('[data-codex-health-status="unavailable"]')).not.toBeNull();

      await act(async () => {
        refreshButton?.dispatchEvent(new window.Event("click", { bubbles: true }));
      });
      expect(rootElement.textContent).toContain("检查超时");
      expect(rootElement.querySelector('[data-codex-health-status="timed-out"]')).not.toBeNull();
      expect(invoke).toHaveBeenCalledWith("get_codex_health");
      expect(invoke).toHaveBeenCalledWith("test_codex_cli");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("keeps the newest pending-review marker result when an older request resolves last", async () => {
    const { document, window } = installDesktopWindow();
    const deriveMarkers = vi.spyOn(aiReviewLib, "pendingReviewSubjectIds");
    const oldMarkerRequest = deferred<AiReviewRecord[]>();
    const newMarkerRequest = deferred<AiReviewRecord[]>();
    const pendingReview = pendingClassificationReview();
    const resolvedReview = { ...pendingReview, state: "manual_override" as const, appliedJson: pendingReview.proposedJson, resolvedAtMs: 10 };
    let listCall = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "list_ai_reviews") {
        listCall += 1;
        if (listCall === 1) return oldMarkerRequest.promise;
        if (listCall === 2) return [pendingReview];
        if (listCall === 3) return [];
        if (listCall === 4) return newMarkerRequest.promise;
      }
      if (command === "resolve_ai_review") return [resolvedReview];
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      const reviewTab = [...rootElement.querySelectorAll<HTMLButtonElement>(".main-tabs button")]
        .find((button) => button.textContent === "AI 审核");
      await act(async () => reviewTab?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);
      expect(listCall).toBe(2);

      await act(async () => rootElement.querySelector<HTMLButtonElement>('[aria-label="接受建议"]')
        ?.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(listCall).toBe(4);

      await act(async () => newMarkerRequest.resolve([]));
      await act(async () => oldMarkerRequest.resolve([pendingReview]));
      const todayTab = [...rootElement.querySelectorAll<HTMLButtonElement>(".main-tabs button")]
        .find((button) => button.textContent === "今日");
      await act(async () => todayTab?.dispatchEvent(new window.Event("click", { bubbles: true })));

      expect(rootElement.querySelector('[data-ai-review-subject="research"]')).toBeNull();
      expect(deriveMarkers).toHaveBeenLastCalledWith([]);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("does not derive pending-review markers after App unmounts", async () => {
    const { document } = installDesktopWindow();
    const markerRequest = deferred<AiReviewRecord[]>();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "list_ai_reviews") return markerRequest.promise;
      return desktopCommandResult(command);
    });
    const deriveMarkers = vi.spyOn(aiReviewLib, "pendingReviewSubjectIds");
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    await act(async () => root.render(<App initialSegments={segments} />));
    const callsBeforeUnmount = deriveMarkers.mock.calls.length;
    await act(async () => root.unmount());
    await act(async () => markerRequest.resolve([pendingClassificationReview()]));

    expect(deriveMarkers).toHaveBeenCalledTimes(callsBeforeUnmount);
  });

  it("shows a neutral AI status in preview without invoking or listening to desktop health", async () => {
    const { document } = installPreviewWindow();
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => undefined);

      const status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
      expect(status?.textContent).toContain("AI · 桌面检测不可用");
      expect(status?.getAttribute("data-ai-health-status")).toBe("preview");
      expect(invoke).not.toHaveBeenCalled();
      expect(listen).not.toHaveBeenCalled();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("loads cached desktop AI health, maps event statuses, and unsubscribes", async () => {
    const { document, window } = installDesktopWindow();
    const unlisten = vi.fn();
    let emitHealth: (health: AiConnectionHealth) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      expect(event).toBe("ai-connection-health-changed");
      const callback = handler as (event: { payload: AiConnectionHealth }) => void;
      emitHealth = (health) => callback({ payload: health });
      return unlisten;
    });
    vi.mocked(invoke).mockImplementation(async (command) => desktopCommandResult(command));
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    await act(async () => root.render(<App initialSegments={segments} />));
    await act(async () => undefined);

    let status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
    expect(status?.textContent).toContain("AI · OpenAI 可用");
    expect(status?.getAttribute("data-ai-health-status")).toBe("healthy");
    expect(status?.title).toContain("执行方式：API Key");
    expect(status?.title).toContain("执行器：OpenAI (openai)");
    expect(status?.title).toContain("模型：gpt-test");
    expect(status?.title).toContain(new Date(1_752_537_600_000).toLocaleString("zh-CN", { hour12: false }));
    expect(invoke).toHaveBeenCalledWith("get_ai_connection_health");

    let nextCheckedAtMs = 1_752_537_600_001;
    await act(async () => emitHealth(aiConnectionHealth({
      status: "checking",
      checkedAtMs: nextCheckedAtMs++,
      diagnostic: "probing",
    })));
    status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
    expect(status?.textContent).toContain("AI · 检测中");
    expect(status?.classList.contains("checking")).toBe(true);

    await act(async () => emitHealth(aiConnectionHealth({
      status: "unconfigured",
      executorLabel: "API",
      checkedAtMs: nextCheckedAtMs++,
    })));
    status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
    expect(status?.textContent).toContain("AI · API 未配置");
    expect(status?.classList.contains("unconfigured")).toBe(true);

    await act(async () => emitHealth(aiConnectionHealth({
      executionMode: "codex",
      executorId: "codex",
      executorLabel: "Codex",
      model: "",
      status: "unconfigured",
      checkedAtMs: nextCheckedAtMs++,
    })));
    status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
    expect(status?.textContent).toContain("AI · Codex 未配置");

    for (const [failure, label] of [
      ["unavailable", "AI · OpenAI 不可用"],
      ["permission-denied", "AI · OpenAI 权限异常"],
      ["timed-out", "AI · OpenAI 超时"],
      ["rate-limited", "AI · OpenAI 限流"],
      ["error", "AI · OpenAI 错误"],
    ] as const) {
      await act(async () => emitHealth(aiConnectionHealth({
        status: failure,
        checkedAtMs: nextCheckedAtMs++,
        diagnostic: `exact ${failure}`,
      })));
      status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
      expect(status?.textContent).toContain(label);
      expect(status?.classList.contains("failure")).toBe(true);
      expect(status?.getAttribute("aria-label")).toContain(failure);
      expect(status?.getAttribute("aria-label")).toContain(`exact ${failure}`);
    }

    await act(async () => root.unmount());
    expect(unlisten).toHaveBeenCalledTimes(1);
    emitHealth(aiConnectionHealth({ executorLabel: "late event" }));
    expect(rootElement.querySelector(".ai-status-button")).toBeNull();
    expect(window).toBeDefined();
  });

  it("refreshes daily analysis only for the active scope and matching date range", async () => {
    const { document, window } = installDesktopWindow();
    const todayScopeKey = "daily-task-monitor-today-activity-scope-v1";
    Object.assign(window, {
      localStorage: {
        getItem: (key: string) => key === todayScopeKey ? "meaningful" : null,
        setItem: () => undefined,
        removeItem: () => undefined,
      },
    });
    let emitAnalysis: (payload: unknown) => void = () => undefined;
    const requestedScopes: unknown[] = [];
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "analysis-changed") {
        const callback = handler as (event: { payload: unknown }) => void;
        emitAnalysis = (payload) => callback({ payload });
      }
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_daily_analysis") {
        const activityScope = (args as { activityScope?: unknown }).activityScope;
        requestedScopes.push(activityScope);
        return {
          portrait: "Scoped analysis",
          recommendation: "Continue",
          source: "ai",
          evidenceHash: "scope-hash",
          generatedAtMs: 1,
          activityScope,
        };
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => undefined);
      const selectedDate = rootElement.querySelector<HTMLInputElement>('input[type="date"]')?.value;
      const initialRequestCount = requestedScopes.length;
      expect(requestedScopes).toContain("meaningful");

      await act(async () => emitAnalysis({
        page: "daily",
        scope: "all",
        date: selectedDate,
        rangeStart: selectedDate,
        rangeEnd: selectedDate,
        evidenceHash: "all-hash",
      }));
      expect(requestedScopes).toHaveLength(initialRequestCount);

      await act(async () => emitAnalysis({
        page: "daily",
        scope: "meaningful",
        date: selectedDate,
        rangeStart: selectedDate,
        rangeEnd: selectedDate,
        evidenceHash: "meaningful-hash",
      }));
      expect(requestedScopes).toHaveLength(initialRequestCount + 1);
      expect(requestedScopes.at(-1)).toBe("meaningful");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("uses deep-analysis automation as the auto-queue gate while keeping manual analysis available", async () => {
    const { document, window } = installDesktopWindow();
    let queued = 0;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_settings") return {
        ...appSettings,
        aiBackfillEnabled: true,
        aiAutoResearchAnalysisEnabled: false,
        aiAutomationNoticeVersion: 1,
      };
      if (command === "get_daily_analysis") return {
        portrait: "Local facts",
        recommendation: "Keep recording",
        findings: [],
        protocolVersion: 2,
        source: "local",
        evidenceHash: "all-current-hash",
        generatedAtMs: 1,
        activityScope: (args as { activityScope?: string }).activityScope ?? "all",
      };
      if (command === "queue_daily_analysis") {
        queued += 1;
        return "manual-job";
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={segments} />);
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(queued).toBe(0);

      const reanalyze = [...rootElement.querySelectorAll("button")]
        .find((button) => button.textContent === "重新分析");
      expect(reanalyze).toBeTruthy();
      await act(async () => reanalyze!.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(queued).toBe(1);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("refreshes authoritative today compositions and scoped analysis after workflow evidence changes", async () => {
    const { document, window } = installDesktopWindow();
    const todayScopeKey = "daily-task-monitor-today-activity-scope-v1";
    Object.assign(window, {
      localStorage: {
        getItem: (key: string) => key === todayScopeKey ? "meaningful" : null,
        setItem: () => undefined,
        removeItem: () => undefined,
      },
    });
    let emitWorkflow: () => void = () => undefined;
    let dashboardRequests = 0;
    let analysisRequests = 0;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "workflow-changed") {
        const callback = handler as (event: { payload: { status: string } }) => void;
        emitWorkflow = () => callback({ payload: { status: "changed" } });
      }
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_today_dashboard") {
        dashboardRequests += 1;
        return {
          timeline: [],
          totals: { monitoredSeconds: 0, activeSeconds: 0, idleSeconds: 0, learningSeconds: 0, categorySeconds: {} },
          workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
          activityComposition: dashboardRequests === 1
            ? {
              all: { totalSeconds: 1_800, items: [{ key: "social", category: "social", videoPurpose: null, seconds: 1_800, share: 1, meaningfulReason: "workflow_link" }] },
              meaningful: { totalSeconds: 0, items: [] },
            }
            : {
              all: { totalSeconds: 1_800, items: [{ key: "social", category: "social", videoPurpose: null, seconds: 1_800, share: 1, meaningfulReason: "workflow_link" }] },
              meaningful: { totalSeconds: 1_800, items: [{ key: "social", category: "social", videoPurpose: null, seconds: 1_800, share: 1, meaningfulReason: "workflow_link" }] },
            },
        };
      }
      if (command === "get_daily_analysis") {
        analysisRequests += 1;
        throw new Error("use local evidence");
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={segments} />);
        await Promise.resolve();
      });
      expect(rootElement.querySelector(".composition-empty")).not.toBeNull();
      const initialAnalysisRequests = analysisRequests;

      await act(async () => {
        emitWorkflow();
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(dashboardRequests).toBe(2);
      expect(analysisRequests).toBeGreaterThan(initialAnalysisRequests);
      expect(rootElement.querySelector(".composition-empty")).toBeNull();
      expect(rootElement.querySelector(".category-list")?.textContent).toContain(
        activityDisplayRegistry.find((item) => item.key === "social")!.label,
      );
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("refreshes the dashboard after a persisted activity event", async () => {
    vi.useFakeTimers();
    const { document } = installDesktopWindow();
    let emitActivity: (observedAtMs: number) => void = () => undefined;
    let dashboardRequests = 0;
    const observedAtMs = Date.now();
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "activity-changed") {
        const callback = handler as (event: { payload: { observedAtMs: number } }) => void;
        emitActivity = (value) => callback({ payload: { observedAtMs: value } });
      }
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_today_dashboard") {
        dashboardRequests += 1;
        return {
          timeline: dashboardRequests === 1 ? [] : [{
            id: "live-segment",
            startedAtMs: observedAtMs - 300_000,
            endedAtMs: observedAtMs,
            app: "Cursor",
            appPath: "/Applications/Cursor.app/Contents/MacOS/Cursor",
            title: "Live activity",
            category: "creation_development",
            videoPurpose: "unknown",
            confidence: 0.9,
            source: "rule",
            reason: "test",
            modelVersion: "test-v1",
            needsReview: false,
          }],
          totals: { monitoredSeconds: 300, activeSeconds: 300, idleSeconds: 0, learningSeconds: 300, categorySeconds: { creation_development: 300 } },
          workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
        };
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={[]} />);
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(dashboardRequests).toBe(1);

      await act(async () => {
        emitActivity(observedAtMs - 2 * 86_400_000);
        await vi.advanceTimersByTimeAsync(500);
      });
      expect(dashboardRequests).toBe(1);

      await act(async () => {
        emitActivity(observedAtMs);
        emitActivity(observedAtMs);
        emitActivity(observedAtMs);
        await vi.advanceTimersByTimeAsync(59_499);
      });

      expect(dashboardRequests).toBe(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1);
      });

      expect(dashboardRequests).toBe(2);
      expect(rootElement.textContent).toContain("活跃5 分钟");
    } finally {
      await act(async () => root.unmount());
      vi.useRealTimers();
    }
  });

  it("retries an empty initial dashboard after the first sampling interval", async () => {
    vi.useFakeTimers();
    const { document } = installDesktopWindow();
    let dashboardRequests = 0;
    const observedAtMs = Date.now();
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_today_dashboard") {
        dashboardRequests += 1;
        return {
          timeline: dashboardRequests === 1 ? [] : [{
            id: "retry-segment",
            startedAtMs: observedAtMs - 60_000,
            endedAtMs: observedAtMs,
            app: "Terminal",
            appPath: "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal",
            title: "Local test",
            category: "creation_development",
            videoPurpose: "unknown",
            confidence: 0.9,
            source: "rule",
            reason: "test",
            modelVersion: "test-v1",
            needsReview: false,
          }],
          totals: { monitoredSeconds: 60, activeSeconds: 60, idleSeconds: 0, learningSeconds: 60, categorySeconds: { creation_development: 60 } },
          workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
        };
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={[]} />);
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(dashboardRequests).toBe(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(60_000);
      });

      expect(dashboardRequests).toBe(2);
      expect(rootElement.textContent).toContain("活跃1 分钟");
    } finally {
      await act(async () => root.unmount());
      vi.useRealTimers();
    }
  });

  it("periodically reconciles today's dashboard when activity events are missed", async () => {
    vi.useFakeTimers();
    const { document } = installDesktopWindow();
    let dashboardRequests = 0;
    const today = new Date().toLocaleDateString("sv-SE");
    const segmentStartMs = new Date(`${today}T01:00:00`).getTime();
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_today_dashboard") {
        dashboardRequests += 1;
        const activeMinutes = dashboardRequests === 1 ? 162 : 181;
        return {
          timeline: [{
            id: "periodically-refreshed-segment",
            startedAtMs: segmentStartMs,
            endedAtMs: segmentStartMs + activeMinutes * 60_000,
            app: "Cursor",
            appPath: "/Applications/Cursor.app/Contents/MacOS/Cursor",
            title: "Live activity",
            category: "creation_development",
            videoPurpose: "unknown",
            confidence: 0.9,
            source: "rule",
            reason: "test",
            modelVersion: "test-v1",
            needsReview: false,
          }],
          totals: {
            monitoredSeconds: activeMinutes * 60,
            activeSeconds: activeMinutes * 60,
            idleSeconds: 0,
            learningSeconds: activeMinutes * 60,
            categorySeconds: { creation_development: activeMinutes * 60 },
          },
          workLedger: { startMs: 0, endMs: 0, projects: [], tasks: [] },
        };
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => {
        root.render(<App initialSegments={[]} />);
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(dashboardRequests).toBe(1);
      expect(rootElement.textContent).toContain("活跃2 小时 42 分钟");

      await act(async () => {
        await vi.advanceTimersByTimeAsync(60_000);
      });

      expect(dashboardRequests).toBe(2);
      expect(rootElement.textContent).toContain("活跃3 小时 1 分钟");
    } finally {
      await act(async () => root.unmount());
      vi.useRealTimers();
    }
  });

  it("cleans up an AI health subscription that resolves after unmount", async () => {
    const { document } = installDesktopWindow();
    const cachedHealth = deferred<AiConnectionHealth>();
    const subscription = deferred<() => void>();
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation((event) => (
      event === "ai-connection-health-changed" ? subscription.promise : Promise.resolve(() => undefined)
    ));
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_ai_connection_health") return cachedHealth.promise;
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    await act(async () => root.render(<App initialSegments={segments} />));
    await act(async () => root.unmount());
    await act(async () => cachedHealth.resolve(aiConnectionHealth({ executorLabel: "late cache" })));
    await act(async () => subscription.resolve(unlisten));

    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(rootElement.querySelector(".ai-status-button")).toBeNull();
  });

  it("does not let a stale cached health overwrite a newer event or a same-time final status", async () => {
    const { document } = installDesktopWindow();
    const cachedHealth = deferred<AiConnectionHealth>();
    let emitHealth: (health: AiConnectionHealth) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event !== "ai-connection-health-changed") return () => undefined;
      const callback = handler as (event: { payload: AiConnectionHealth }) => void;
      emitHealth = (health) => callback({ payload: health });
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_ai_connection_health") return cachedHealth.promise;
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => emitHealth(aiConnectionHealth({
        executorLabel: "Newest event",
        status: "unavailable",
        checkedAtMs: 300,
        diagnostic: "new final",
      })));
      await act(async () => emitHealth(aiConnectionHealth({
        executorLabel: "same-time checking",
        status: "checking",
        checkedAtMs: 300,
      })));
      await act(async () => cachedHealth.resolve(aiConnectionHealth({
        executorLabel: "stale cache",
        status: "healthy",
        checkedAtMs: 100,
      })));

      const status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
      expect(status?.getAttribute("data-ai-health-status")).toBe("unavailable");
      expect(status?.title).toContain("Newest event");
      expect(status?.title).not.toContain("stale cache");
      expect(status?.title).not.toContain("same-time checking");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("does not let a stale explicit refresh overwrite a newer health event", async () => {
    const { document, window } = installDesktopWindow();
    const refreshedHealth = deferred<AiConnectionHealth>();
    let emitHealth: (health: AiConnectionHealth) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event !== "ai-connection-health-changed") return () => undefined;
      const callback = handler as (event: { payload: AiConnectionHealth }) => void;
      emitHealth = (health) => callback({ payload: health });
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_ai_connection_health") return aiConnectionHealth({ checkedAtMs: 100 });
      if (command === "refresh_ai_connection_health") return refreshedHealth.promise;
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => undefined);
      await act(async () => rootElement.querySelector<HTMLButtonElement>(".ai-status-button")
        ?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => emitHealth(aiConnectionHealth({
        executorLabel: "Newest refresh event",
        status: "permission-denied",
        checkedAtMs: 300,
        diagnostic: "new refresh final",
      })));
      await act(async () => refreshedHealth.resolve(aiConnectionHealth({
        executorLabel: "stale refresh",
        status: "healthy",
        checkedAtMs: 200,
      })));

      const status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
      expect(status?.getAttribute("data-ai-health-status")).toBe("permission-denied");
      expect(status?.title).toContain("Newest refresh event");
      expect(status?.title).not.toContain("stale refresh");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("redacts and bounds event diagnostics and explicit refresh errors in accessible status text", async () => {
    const { document, window } = installDesktopWindow();
    const secret = "https://user:pass@example.test/models?api_key=secret";
    let emitHealth: (health: AiConnectionHealth) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      const callback = handler as (event: { payload: AiConnectionHealth }) => void;
      emitHealth = (health) => callback({ payload: health });
      return () => undefined;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "refresh_ai_connection_health") throw new Error(`${secret} ${"x".repeat(2_000)}`);
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      const cases = [
        { diagnostic: secret, exposed: ["user", "pass", "api_key", "secret"] },
        { diagnostic: "Authorization: Bearer bearer-secret", exposed: ["bearer-secret"] },
        { diagnostic: "API key: api-secret", exposed: ["api-secret"] },
        { diagnostic: "credential sk-1234567890", exposed: ["sk-1234567890"] },
        { diagnostic: "y".repeat(2_000), exposed: [] },
      ];
      let checkedAtMs = 1_752_537_600_100;
      for (const testCase of cases) {
        await act(async () => emitHealth(aiConnectionHealth({
          status: "error",
          checkedAtMs: checkedAtMs++,
          diagnostic: testCase.diagnostic,
        })));
        const status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
        for (const exposed of testCase.exposed) {
          expect(status?.title).not.toContain(exposed);
          expect(status?.getAttribute("aria-label")).not.toContain(exposed);
        }
        expect(status?.title.length).toBeLessThan(512);
        expect(status?.getAttribute("aria-label")?.length).toBeLessThan(600);
      }

      let status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
      await act(async () => status?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);
      status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");
      for (const exposed of ["user", "pass", "api_key", "secret"]) {
        expect(status?.title).not.toContain(exposed);
        expect(status?.getAttribute("aria-label")).not.toContain(exposed);
      }
      expect(status?.title.length).toBeLessThan(512);
      expect(status?.getAttribute("aria-label")?.length).toBeLessThan(600);
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("refreshes health and opens settings focused at the AI provider section", async () => {
    const { document, window, scrollIntoView } = installDesktopWindow();
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async (command) => desktopCommandResult(command));
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => undefined);
      const status = rootElement.querySelector<HTMLButtonElement>(".ai-status-button");

      await act(async () => status?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const aiSection = rootElement.querySelector<HTMLElement>("#ai-provider-settings");
      expect(invoke).toHaveBeenCalledWith("refresh_ai_connection_health");
      expect(aiSection).not.toBeNull();
      expect(document.activeElement).toBe(aiSection);
      expect(scrollIntoView).toHaveBeenCalledWith({ behavior: "smooth", block: "start" });
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("shows the saved idle threshold and rolls back a failed update", async () => {
    const { document, window } = installDesktopWindow();
    const settings = {
      ...appSettings,
      idleThresholdMinutes: 15,
      aiAutomationNoticeVersion: 1,
    };
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_settings") return settings;
      if (command === "update_settings") {
        const minutes = (args as { patch: { idleThresholdMinutes?: number } }).patch.idleThresholdMinutes;
        if (minutes === 6) throw new Error("database unavailable");
        if (minutes) settings.idleThresholdMinutes = minutes;
        return { ...settings };
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => rootElement.querySelector<HTMLButtonElement>('button[aria-label="打开设置"]')
        ?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const threshold = rootElement.querySelector<HTMLSelectElement>('select[aria-label="不活跃阈值"]');
      expect(threshold?.value).toBe("15");
      let selectedValue = threshold?.value ?? "";
      if (threshold) {
        Object.defineProperty(threshold, "value", {
          configurable: true,
          get: () => selectedValue,
          set: (value: string) => { selectedValue = value; },
        });
      }

      await act(async () => {
        selectedValue = "10";
        threshold?.dispatchEvent(new window.Event("change", { bubbles: true }));
      });
      expect(threshold?.value).toBe("10");
      expect(invoke).toHaveBeenCalledWith("update_settings", {
        patch: { idleThresholdMinutes: 10 },
      });

      await act(async () => {
        selectedValue = "6";
        threshold?.dispatchEvent(new window.Event("change", { bubbles: true }));
      });
      if (threshold) Reflect.deleteProperty(threshold, "value");
      expect(threshold?.value).toBe("10");
      expect(rootElement.textContent).toContain("不活跃阈值保存失败");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("shows collection channel diagnostics and local watcher credentials", async () => {
    const { document, window } = installDesktopWindow();
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async (command) => desktopCommandResult(command));
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      const healthTab = [...rootElement.querySelectorAll<HTMLButtonElement>(".main-tabs button")]
        .find((button) => button.textContent === "健康诊断");
      await act(async () => healthTab?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const diagnostics = rootElement.querySelector<HTMLElement>('[aria-label="采集健康诊断"]');
      expect(diagnostics).not.toBeNull();
      expect(diagnostics?.textContent).toContain("浏览器实时 watcher");
      expect(diagnostics?.textContent).toContain("2 分钟");
      expect(diagnostics?.textContent).toContain("4 个可信心跳切片");
      expect(rootElement.querySelector<HTMLInputElement>("#browser-watcher-endpoint")?.value)
        .toBe("http://127.0.0.1:27123/v1/heartbeat");
      expect(rootElement.querySelector<HTMLInputElement>("#browser-watcher-token")?.value)
        .toBe("local-test-token");
      expect(invoke).toHaveBeenCalledWith("get_collection_health");
      expect(rootElement.querySelector('[role="dialog"][aria-label="设置"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("refreshes unified health after AI mode, provider, custom provider, Codex save, and manual tests", async () => {
    const { document, window } = installDesktopWindow();
    const settings = { ...appSettings, aiExecutionMode: "api-key" as const, aiAutomationNoticeVersion: 1 };
    const providers = [
      { id: "openai", name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-test", enabled: true, priority: 0, hasCredential: true },
      { id: "custom", name: "Custom", baseUrl: "https://old.example/v1", model: "old-model", enabled: true, priority: 1, hasCredential: true },
    ];
    const prompts = vi.fn()
      .mockReturnValueOnce("")
      .mockReturnValueOnce("https://new.example/v1")
      .mockReturnValueOnce("new-model")
      .mockReturnValueOnce("new-key");
    Object.assign(window, { prompt: prompts });
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_settings") return settings;
      if (command === "list_ai_providers") return providers;
      if (command === "get_codex_health" || command === "test_codex_cli") return {
        configuredPath: "codex", detectedPath: "C:\\Tools\\codex.exe", version: "codex-cli test",
        checkedAtMs: 1_752_537_600_000, status: "healthy", diagnostic: null,
      };
      if (command === "test_ai_provider") throw new Error("provider rejected");
      if (command === "update_settings") return { ...settings, ...(args as { patch: Partial<typeof settings> }).patch };
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => rootElement.querySelector<HTMLButtonElement>('button[aria-label="打开设置"]')
        ?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const providerRow = (name: string) => [...rootElement.querySelectorAll<HTMLElement>(".provider-row")]
        .find((row) => row.textContent?.includes(name));
      const rowButton = (name: string, label: string) => [...(providerRow(name)?.querySelectorAll<HTMLButtonElement>("button") ?? [])]
        .find((button) => button.textContent === label);

      await act(async () => rowButton("OpenAI", "配置")?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => rowButton("Custom", "配置")?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => rowButton("OpenAI", "测试")?.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(rootElement.textContent).toContain("provider rejected");
      const codexMode = [...rootElement.querySelectorAll<HTMLButtonElement>(".ai-mode-switch button")]
        .find((button) => button.textContent === "本机 Codex");
      await act(async () => codexMode?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const executable = rootElement.querySelector<HTMLInputElement>("#codex-executable");
      await act(async () => executable?.dispatchEvent(new window.Event("focusout", { bubbles: true })));
      const codexTest = rootElement.querySelector<HTMLButtonElement>('button[aria-label="刷新并测试 Codex"]');
      await act(async () => codexTest?.dispatchEvent(new window.Event("click", { bubbles: true })));

      const refreshCalls = vi.mocked(invoke).mock.calls.filter(([command]) => command === "refresh_ai_connection_health");
      expect(refreshCalls).toHaveLength(6);
      expect(invoke).toHaveBeenCalledWith("save_ai_provider_key", { providerId: "openai", apiKey: "" });
      expect(invoke).toHaveBeenCalledWith("save_custom_ai_provider", { baseUrl: "https://new.example/v1", model: "new-model" });
      expect(invoke).toHaveBeenCalledWith("test_ai_provider", { providerId: "openai" });
      expect(invoke).toHaveBeenCalledWith("update_settings", { patch: { aiExecutionMode: "codex" } });
      expect(invoke).toHaveBeenCalledWith("test_codex_cli");
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("requires a credentialed primary provider before API mode can be selected", async () => {
    const { document, window } = installDesktopWindow();
    const settings = {
      ...appSettings,
      aiExecutionMode: "codex" as const,
      selectedApiProviderId: null as string | null,
      aiAutomationNoticeVersion: 1,
    };
    const providers = [
      { id: "openai", name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-frozen", enabled: true, priority: 0, hasCredential: true },
      { id: "zhipu", name: "Zhipu", baseUrl: "https://example.test/v1", model: "glm", enabled: true, priority: 1, hasCredential: false },
    ];
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_settings") return settings;
      if (command === "list_ai_providers") return providers;
      if (command === "update_settings") {
        Object.assign(settings, (args as { patch: Partial<typeof settings> }).patch);
        return { ...settings };
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => rootElement.querySelector<HTMLButtonElement>('button[aria-label="打开设置"]')
        ?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const apiMode = [...rootElement.querySelectorAll<HTMLButtonElement>(".ai-mode-switch button")]
        .find((button) => button.textContent === "API Key");
      const openaiPrimary = rootElement.querySelector<HTMLInputElement>('input[name="primary-ai-provider"][value="openai"]');
      const zhipuPrimary = rootElement.querySelector<HTMLInputElement>('input[name="primary-ai-provider"][value="zhipu"]');
      expect(apiMode?.disabled).toBe(true);
      expect(openaiPrimary?.disabled).toBe(false);
      expect(zhipuPrimary?.disabled).toBe(true);

      await act(async () => openaiPrimary?.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(openaiPrimary?.checked).toBe(true);
      expect(apiMode?.disabled).toBe(false);
      expect(invoke).toHaveBeenCalledWith("update_settings", {
        patch: { selectedApiProviderId: "openai" },
      });

      await act(async () => apiMode?.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(invoke).toHaveBeenCalledWith("update_settings", {
        patch: { aiExecutionMode: "api-key" },
      });
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("clears the primary selection when its saved key is deleted", async () => {
    const { document, window } = installDesktopWindow();
    const settings = {
      ...appSettings,
      aiExecutionMode: "api-key" as const,
      selectedApiProviderId: "openai" as string | null,
      aiAutomationNoticeVersion: 1,
    };
    const configured = { id: "openai", name: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-frozen", enabled: true, priority: 0, hasCredential: true };
    let providerLoads = 0;
    Object.assign(window, { prompt: vi.fn().mockReturnValue("") });
    vi.mocked(listen).mockResolvedValue(vi.fn());
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_settings") return settings;
      if (command === "list_ai_providers") {
        providerLoads += 1;
        return [{ ...configured, hasCredential: providerLoads === 1 }];
      }
      return desktopCommandResult(command);
    });
    const rootElement = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(rootElement);

    try {
      await act(async () => root.render(<App initialSegments={segments} />));
      await act(async () => rootElement.querySelector<HTMLButtonElement>('button[aria-label="打开设置"]')
        ?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const primary = rootElement.querySelector<HTMLInputElement>('input[name="primary-ai-provider"][value="openai"]');
      const configure = rootElement.querySelector<HTMLButtonElement>(".provider-row .provider-actions button");
      expect(primary?.checked).toBe(true);
      await act(async () => configure?.dispatchEvent(new window.Event("click", { bubbles: true })));
      await act(async () => undefined);

      const apiMode = [...rootElement.querySelectorAll<HTMLButtonElement>(".ai-mode-switch button")]
        .find((button) => button.textContent === "API Key");
      expect(primary?.checked).toBe(false);
      expect(apiMode?.disabled).toBe(true);
      expect(invoke).toHaveBeenCalledWith("save_ai_provider_key", {
        providerId: "openai",
        apiKey: "",
      });
    } finally {
      await act(async () => root.unmount());
    }
  });
});
