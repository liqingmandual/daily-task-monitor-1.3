import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildTrendRangeArguments,
  exportTrendMarkdown,
  loadWorkLedger,
  saveWorkLedgerProject,
  saveWorkLedgerTask,
  updateWorkLedgerTaskStatus,
  addWorkLedgerProgress,
  assignWorkLedgerEvidence,
  removeWorkLedgerEvidence,
  applyWorkLedgerSuggestion,
  beginFocus,
  dayBounds,
  buildDailyGoalRows,
  completeFocus,
  confirmDailyGoalTask,
  classifySegment,
  getAiConnectionHealth,
  getCodexHealth,
  getCollectionHealth,
  getFocusTimerStatus,
  getDailyGoal,
  listDailyGoalTaskLinks,
  recordDailyActualOutputProgress,
  saveDailyGoal,
  refreshAiConnectionHealth,
  listenAiConnectionHealthChanged,
  listenActivityChanged,
  listenCollectionHealthChanged,
  listenOpenSettings,
  listenFocusTimerChanged,
  testCodexCli,
  loadTrendAnalysis,
  loadTrendRange,
  loadTrendWorkbench,
  loadTrendResearchAnalysis,
  isTrendWorkbenchError,
  loadDailyAnalysis,
  enqueueDailyAnalysis,
  listenDailyAnalysisChanged,
  loadKnowledgeGraph,
  getAppSettings,
  resolveAppIdentities,
  toUiSegments,
  queueTrendAnalysis,
  queueTrendResearchAnalysis,
  updateSettings,
  updateKnowledgeGraphExperiment,
  updateUiFont,
  updateUiTheme,
  type AppSettings,
  type AiConnectionHealth,
  type BackendSegment,
  type CodexHealth,
  type CollectionHealth,
  type TrendPayload,
  type TrendAnalysisResult,
  type TrendWorkbenchPayload,
  type TrendWorkbenchRequest,
  type WorkLedgerSnapshot,
  type DailyGoalRecord,
  type WorkLedgerProgressEntry,
} from "./desktop";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("desktop bridge", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
  });

  it("uses the collection health command and event contracts", async () => {
    const health = { generatedAtMs: 1_000 } as CollectionHealth;
    const stop = vi.fn();
    vi.mocked(invoke).mockResolvedValueOnce(health);
    vi.mocked(listen).mockResolvedValueOnce(stop);
    const onChanged = vi.fn();

    await expect(getCollectionHealth()).resolves.toBe(health);
    await expect(listenCollectionHealthChanged(onChanged)).resolves.toBe(stop);

    expect(invoke).toHaveBeenCalledWith("get_collection_health");
    expect(listen).toHaveBeenCalledWith("collection-health-changed", expect.any(Function));
    const eventHandler = vi.mocked(listen).mock.calls[0][1];
    eventHandler({ payload: undefined } as never);
    expect(onChanged).toHaveBeenCalledOnce();
  });

  it("listens for the native settings menu event", async () => {
    const stop = vi.fn();
    const onOpen = vi.fn();
    vi.mocked(listen).mockResolvedValueOnce(stop);

    await expect(listenOpenSettings(onOpen)).resolves.toBe(stop);

    expect(listen).toHaveBeenCalledWith("open-settings", onOpen);
  });

  it("loads and subscribes to the persisted focus countdown", async () => {
    const status = {
      sessionId: "focus-1",
      goalDate: "2026-08-13",
      goalText: "Ship timer",
      plannedMinutes: 25,
      startedAtMs: 1_000,
      endsAtMs: 1_501_000,
      remainingSeconds: 1_500,
      expired: false,
      paused: false,
      taskId: null,
    };
    const stop = vi.fn();
    const onChanged = vi.fn();
    vi.mocked(invoke).mockResolvedValueOnce(status);
    vi.mocked(listen).mockResolvedValueOnce(stop);

    await expect(getFocusTimerStatus()).resolves.toBe(status);
    await expect(listenFocusTimerChanged(onChanged)).resolves.toBe(stop);

    expect(invoke).toHaveBeenCalledWith("get_focus_timer_status");
    expect(listen).toHaveBeenCalledWith("focus-timer-changed", expect.any(Function));
    const eventHandler = vi.mocked(listen).mock.calls[0][1];
    eventHandler({ payload: status } as never);
    expect(onChanged).toHaveBeenCalledWith(status);
  });

  it("accepts an inclusive 366-day trend range", () => {
    const args = buildTrendRangeArguments("2024-01-01", "2024-12-31");

    expect(args.dayBoundariesMs).toHaveLength(367);
    expect(args.comparisonDayBoundariesMs).toHaveLength(367);
  });

  it("rejects an inclusive 367-day trend range before invoking the backend", async () => {
    vi.mocked(invoke).mockClear();

    await expect(loadTrendRange("2024-01-01", "2025-01-01")).rejects.toThrow(
      "Trend range cannot exceed 366 days",
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  it.each([
    ["2024-1-01", "2024-01-02"],
    ["2024-02-30", "2024-03-01"],
    ["2024-01-02", "2024-01-01"],
  ])("rejects invalid trend dates %s to %s", (startDate, endDate) => {
    expect(() => buildTrendRangeArguments(startDate, endDate)).toThrow(RangeError);
  });

  it("converts epoch timestamps to day-relative dashboard segments", () => {
    const dayStart = 1_700_000_000_000;
    const record: BackendSegment = {
      id: "one",
      startedAtMs: dayStart + 3_600_000,
      endedAtMs: dayStart + 3_660_000,
      app: "Codex",
      appPath: "C:\\Program Files\\OpenAI\\Codex.exe",
      title: "Desktop rewrite",
      category: "creation_development",
      videoPurpose: "unknown",
      confidence: 0.91,
      source: "rule",
      reason: "Known development tool",
      modelVersion: "rules-v1",
      needsReview: false,
    };

    expect(toUiSegments([record], dayStart, dayStart + 86_400_000)[0]).toMatchObject({
      startMs: 3_600_000,
      endMs: 3_660_000,
      appPath: "C:\\Program Files\\OpenAI\\Codex.exe",
      category: "creation_development",
    });
  });

  it("clips segments that cross the end of the selected day", () => {
    const dayStart = new Date("2026-07-10T00:00:00").getTime();
    const dayEnd = dayStart + 86_400_000;
    const record: BackendSegment = {
      id: "cross-midnight",
      startedAtMs: dayEnd - 60_000,
      endedAtMs: dayEnd + 20 * 60_000,
      app: "Codex",
      appPath: "",
      title: "Late work",
      category: "creation_development",
      videoPurpose: "unknown",
      confidence: 0.9,
      source: "rule",
      reason: "test",
      modelVersion: "test-v1",
      needsReview: false,
    };

    const clipped = toUiSegments([record], dayStart, dayEnd)[0];
    expect(clipped.endMs - clipped.startMs).toBe(60_000);
  });

  it("persists the UI theme through the settings patch", async () => {
    await updateUiTheme("knowledge-space");

    expect(invoke).toHaveBeenCalledWith("update_settings", {
      patch: { uiTheme: "knowledge-space" },
    });
  });

  it("persists the UI font through the settings patch", async () => {
    await updateUiFont("Inter");

    expect(invoke).toHaveBeenCalledWith("update_settings", {
      patch: { uiFont: "Inter" },
    });
  });

  it("persists the experimental knowledge graph switch", async () => {
    await updateKnowledgeGraphExperiment(true);

    expect(invoke).toHaveBeenCalledWith("update_settings", {
      patch: { experimentalKnowledgeGraphEnabled: true },
    });
  });

  it("scopes daily analysis requests and normalizes legacy results to all", async () => {
    const legacyResult = {
      portrait: "Legacy analysis",
      recommendation: "Keep working",
      source: "ai" as const,
      evidenceHash: "legacy-hash",
      generatedAtMs: 123,
    };
    vi.mocked(invoke).mockResolvedValueOnce(legacyResult).mockResolvedValueOnce("job-1");

    await expect(loadDailyAnalysis("2026-08-07", "meaningful")).resolves.toEqual({
      ...legacyResult,
      activityScope: "all",
    });
    await expect(enqueueDailyAnalysis("2026-08-07", "meaningful")).resolves.toBe("job-1");

    const { startMs, endMs } = dayBounds("2026-08-07");
    expect(invoke).toHaveBeenNthCalledWith(1, "get_daily_analysis", {
      date: "2026-08-07",
      startMs,
      endMs,
      activityScope: "meaningful",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "queue_daily_analysis", {
      date: "2026-08-07",
      startMs,
      endMs,
      activityScope: "meaningful",
    });
  });

  it("forwards the complete analysis-changed payload", async () => {
    const received: unknown[] = [];
    let emit: (payload: unknown) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      expect(event).toBe("analysis-changed");
      const callback = handler as (event: { payload: unknown }) => void;
      emit = (payload) => callback({ payload });
      return () => undefined;
    });

    await listenDailyAnalysisChanged((event) => received.push(event));
    const payload = {
      page: "daily",
      scope: "meaningful",
      date: "2026-08-07",
      rangeStart: "2026-08-07",
      rangeEnd: "2026-08-07",
      evidenceHash: "scope-hash",
    };
    emit(payload);

    expect(received).toEqual([payload]);
  });

  it("forwards activity-changed timestamps", async () => {
    const received: unknown[] = [];
    let emit: (payload: unknown) => void = () => undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      expect(event).toBe("activity-changed");
      const callback = handler as (event: { payload: unknown }) => void;
      emit = (payload) => callback({ payload });
      return () => undefined;
    });

    await listenActivityChanged((event) => received.push(event));
    emit({ observedAtMs: 1_786_521_600_000 });

    expect(received).toEqual([{ observedAtMs: 1_786_521_600_000 }]);
  });

  it("preserves an explicitly selected video purpose in manual classification", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true);

    await classifySegment("video-one", "video_input", "learning");

    expect(invoke).toHaveBeenCalledWith("save_manual_classification", {
      request: {
        segmentId: "video-one",
        category: "video_input",
        videoPurpose: "learning",
        reason: "User correction",
      },
    });
  });

  it("normalizes video purpose to unknown for non-video manual classification", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true);

    await classifySegment("code-one", "creation_development", "leisure");

    expect(invoke).toHaveBeenCalledWith("save_manual_classification", {
      request: {
        segmentId: "code-one",
        category: "creation_development",
        videoPurpose: "unknown",
        reason: "User correction",
      },
    });
  });

  it("loads and patches the AI execution settings with exact camelCase fields", async () => {
    const settings = {
      idleThresholdMinutes: 6,
      monitoringEnabled: true,
      aiBackfillEnabled: false,
      aiExecutionMode: "codex",
      selectedApiProviderId: null,
      codexExecutable: "codex",
      codexModel: "",
      aiAutoResearchAnalysisEnabled: true,
      aiAutoClassificationEnabled: false,
      aiAutoWorkflowAssignmentEnabled: true,
      aiAutomationNoticeVersion: 0,
      excludedApps: [],
      excludedDomains: [],
      uiTheme: "classic-workbench",
      uiFont: "Ubuntu",
      experimentalKnowledgeGraphEnabled: false,
    } satisfies AppSettings;
    vi.mocked(invoke).mockResolvedValueOnce(settings);

    await expect(getAppSettings()).resolves.toEqual(settings);
    await updateSettings({
      selectedApiProviderId: "openai",
      codexExecutable: "C:\\Tools\\codex.exe",
      codexModel: "gpt-5-codex",
      aiAutoResearchAnalysisEnabled: false,
      aiAutoClassificationEnabled: false,
      aiAutoWorkflowAssignmentEnabled: true,
      aiAutomationNoticeVersion: 1,
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "get_settings");
    expect(invoke).toHaveBeenNthCalledWith(2, "update_settings", {
      patch: {
        selectedApiProviderId: "openai",
        codexExecutable: "C:\\Tools\\codex.exe",
        codexModel: "gpt-5-codex",
        aiAutoResearchAnalysisEnabled: false,
        aiAutoClassificationEnabled: false,
        aiAutoWorkflowAssignmentEnabled: true,
        aiAutomationNoticeVersion: 1,
      },
    });
  });

  it.each([
    ["get_codex_health", getCodexHealth],
    ["test_codex_cli", testCodexCli],
  ] as const)("returns the exact structured Codex health contract from %s", async (command, requestHealth) => {
    const health = {
      configuredPath: "codex",
      detectedPath: "C:\\Tools\\codex.exe",
      version: "codex-cli 1.2.3",
      checkedAtMs: 1_752_537_600_000,
      status: "healthy",
      diagnostic: null,
    } satisfies CodexHealth;
    vi.mocked(invoke).mockResolvedValueOnce(health);

    await expect(requestHealth()).resolves.toEqual(health);
    expect(invoke).toHaveBeenCalledWith(command);
  });

  it.each([
    ["get_ai_connection_health", getAiConnectionHealth],
    ["refresh_ai_connection_health", refreshAiConnectionHealth],
  ] as const)("returns the unified AI health contract from %s", async (command, requestHealth) => {
    const health = {
      executionMode: "api-key",
      executorId: "openai",
      executorLabel: "OpenAI",
      model: "gpt-4.1-mini",
      status: "healthy",
      verificationLevel: "inference",
      source: "manual",
      checkedAtMs: 1_752_537_600_000,
      verifiedAtMs: 1_752_537_600_000,
      diagnostic: null,
    } satisfies AiConnectionHealth;
    vi.mocked(invoke).mockResolvedValueOnce(health);

    await expect(requestHealth()).resolves.toEqual(health);
    expect(invoke).toHaveBeenCalledWith(command);
  });

  it("listens for Rust-owned AI health changes and unwraps the event payload", async () => {
    const unlisten = vi.fn();
    const handler = vi.fn();
    vi.mocked(listen).mockImplementationOnce(async (event, callback) => {
      expect(event).toBe("ai-connection-health-changed");
      callback({
        event,
        id: 1,
        payload: {
          executionMode: "codex",
          executorId: "codex",
          executorLabel: "Codex CLI",
          model: "cli-default",
          status: "checking",
          verificationLevel: "connectivity",
          source: "background",
          checkedAtMs: 1_752_537_600_000,
          verifiedAtMs: null,
          diagnostic: null,
        },
      });
      return unlisten;
    });

    await expect(listenAiConnectionHealthChanged(handler)).resolves.toBe(unlisten);
    expect(handler).toHaveBeenCalledWith(expect.objectContaining({
      executionMode: "codex",
      status: "checking",
    }));
  });

  it("loads the knowledge graph with the selected range and local timezone", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ nodes: [], links: [], counts: {}, totalSeconds: 0, startMs: 10, endMs: 20 });

    await loadKnowledgeGraph(10, 20, "research", ["activity"]);

    expect(invoke).toHaveBeenCalledWith("get_knowledge_graph", {
      startMs: 10,
      endMs: 20,
      filters: {
        search: "research",
        nodeTypes: ["activity"],
        timezoneOffsetMinutes: new Date().getTimezoneOffset(),
      },
    });
  });

  it("loads an authoritative trend payload for inclusive local dates", async () => {
    const payload: TrendPayload = {
      range: { startMs: 10, endMs: 20, startDate: "2026-07-10", endDate: "2026-07-11", dayCount: 2 },
      days: [],
      summary: {
        monitoredSeconds: 7_200,
        activeSeconds: 5_400,
        idleSeconds: 1_800,
        learningSeconds: 3_600,
        switchCount: 2,
        longestFocusSeconds: 1_800,
        averageMonitoredSeconds: 3_600,
        averageActiveSeconds: 2_700,
        averageIdleSeconds: 900,
        averageLearningSeconds: 1_800,
        averageSwitchCount: 1,
        learningRatio: 2 / 3,
        switchesPerActiveHour: 4 / 3,
        productiveDayCount: 2,
        focusDayCount: 2,
        categoryBreakdown: [],
        appBreakdown: [],
      },
      comparison: {
        previousRange: { startMs: 0, endMs: 10, startDate: "2026-07-08", endDate: "2026-07-09", dayCount: 2 },
        dayCount: 2,
        previousMonitoredSeconds: 1_800,
        previousActiveSeconds: 1_800,
        previousIdleSeconds: 0,
        previousLearningSeconds: 0,
        previousSwitchCount: 0,
        previousLongestFocusSeconds: 1_800,
        previousLearningRatio: 0,
        previousSwitchesPerActiveHour: 0,
        previousClassificationCoverage: 1,
        previousCategoryBreakdown: [],
        previousAppBreakdown: [],
        monitoredSecondsDeltaPercent: 300,
        activeSecondsDeltaPercent: 200,
        idleSecondsDeltaPercent: null,
        learningSecondsDeltaPercent: null,
        switchCountDeltaPercent: null,
        longestFocusSecondsDeltaPercent: 0,
        learningRatioDeltaPercent: null,
        switchesPerActiveHourDeltaPercent: null,
        classificationCoverageDeltaPercent: -25,
      },
      quality: {
        recordedDayCount: 2,
        missingDayCount: 0,
        classifiedSeconds: 5_400,
        pendingSeconds: 1_800,
        lowConfidenceSeconds: 0,
        classificationCoverage: 0.75,
      },
      workLedger: { startMs: 10, endMs: 20, projects: [], tasks: [] },
      evidenceHash: "abc123",
    };
    vi.mocked(invoke).mockResolvedValueOnce(payload);

    await expect(loadTrendRange("2026-07-10", "2026-07-11")).resolves.toBe(payload);
    expect(payload.comparison.learningSecondsDeltaPercent).toBeNull();

    expect(invoke).toHaveBeenCalledWith("get_trends", {
      startDate: "2026-07-10",
      endDate: "2026-07-11",
      dayBoundariesMs: [
        new Date("2026-07-10T00:00:00").getTime(),
        new Date("2026-07-11T00:00:00").getTime(),
        new Date("2026-07-12T00:00:00").getTime(),
      ],
      comparisonStartDate: "2026-07-08",
      comparisonEndDate: "2026-07-09",
      comparisonDayBoundariesMs: [
        new Date("2026-07-08T00:00:00").getTime(),
        new Date("2026-07-09T00:00:00").getTime(),
        new Date("2026-07-10T00:00:00").getTime(),
      ],
      timezoneOffsetMinutes: new Date().getTimezoneOffset(),
    });
  });

  it("bridges the trend workbench request without rebuilding legacy day boundaries", async () => {
    const request: TrendWorkbenchRequest = {
      startDate: "2026-07-10",
      endDate: "2026-07-11",
      timezoneOffsetMinutes: -480,
      granularity: "week",
      metric: "activeSeconds",
      customBaseline: {
        startDate: "2026-06-10",
        endDate: "2026-06-11",
      },
    };
    const payload = { evidenceHash: "all-evidence-hash" } as unknown as TrendWorkbenchPayload;
    vi.mocked(invoke).mockResolvedValueOnce(payload);

    await expect(loadTrendWorkbench(request)).resolves.toMatchObject({
      evidenceHash: "all-evidence-hash",
      analysisEvidenceHash: "all-evidence-hash",
      analysisActivityScope: "all",
    });

    expect(invoke).toHaveBeenCalledWith("get_trend_workbench", { request });
  });

  it("keeps the full workbench payload while binding AI evidence to a meaningful scope", async () => {
    const request: TrendWorkbenchRequest = {
      startDate: "2026-07-10",
      endDate: "2026-07-11",
      timezoneOffsetMinutes: -480,
      granularity: "day",
      metric: "activeSeconds",
      activityScope: "meaningful",
      customBaseline: null,
    };
    const full = {
      evidenceHash: "full-evidence-hash",
      activityComposition: { marker: "full-payload" },
      summary: { marker: "full-summary" },
      evidence: [{ id: "full-evidence" }],
    } as unknown as TrendWorkbenchPayload;
    const scoped = {
      evidenceHash: "meaningful-evidence-hash",
      activityComposition: { marker: "meaningful-payload" },
      summary: { marker: "meaningful-summary" },
      evidence: [{ id: "meaningful-evidence" }],
    } as unknown as TrendWorkbenchPayload;
    vi.mocked(invoke).mockResolvedValueOnce(full).mockResolvedValueOnce(scoped);

    const result = await loadTrendWorkbench(request);

    expect(result.evidenceHash).toBe("full-evidence-hash");
    expect(result.activityComposition).toEqual({ marker: "full-payload" });
    expect(result.analysisEvidenceHash).toBe("meaningful-evidence-hash");
    expect(result.analysisActivityScope).toBe("meaningful");
    expect(result.analysisSummary).toEqual({ marker: "meaningful-summary" });
    expect(result.analysisEvidence).toEqual([{ id: "meaningful-evidence" }]);
    const { activityScope: _activityScope, ...backendRequest } = request;
    expect(invoke).toHaveBeenNthCalledWith(1, "get_trend_workbench", { request: backendRequest });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_trend_workbench", {
      request: backendRequest,
      activityScope: "meaningful",
    });
  });

  it("recognizes stable typed trend workbench errors", () => {
    expect(isTrendWorkbenchError({
      code: "metricUnavailable",
      message: "linkedTaskSeconds is unavailable until task aggregation is implemented",
      metric: "linkedTaskSeconds",
    })).toBe(true);
    expect(isTrendWorkbenchError({ code: "unknown", message: "no" })).toBe(false);
    expect(isTrendWorkbenchError({ code: "metricUnavailable", message: "no metric" })).toBe(false);
    expect(isTrendWorkbenchError(new Error("network"))).toBe(false);
  });

  it("loads the exact trend analysis mirror with authoritative range boundaries", async () => {
    const analysis: TrendAnalysisResult = {
      rangeStart: "2026-07-10",
      rangeEnd: "2026-07-11",
      evidenceHash: "abc123",
      summary: "本区间记录显示活动结构保持稳定",
      observations: ["活动时长保持稳定"],
      suggestions: ["建议尝试延续当前节奏"],
      source: "local",
      model: "deterministic-v1",
      confidence: 0.75,
      generatedAtMs: 1_752_163_200_000,
      activityScope: "all",
    };
    vi.mocked(invoke).mockResolvedValueOnce(analysis);
    const rangeArgs = buildTrendRangeArguments("2026-07-10", "2026-07-11");

    await expect(loadTrendAnalysis("2026-07-10", "2026-07-11")).resolves.toEqual(analysis);
    expect(invoke).toHaveBeenCalledWith("get_trend_analysis", { ...rangeArgs, activityScope: "all" });
  });

  it("queues forced trend analysis with the same authoritative boundaries", async () => {
    vi.mocked(invoke).mockResolvedValueOnce("trend-job-1");
    const rangeArgs = buildTrendRangeArguments("2026-07-10", "2026-07-11");

    await expect(queueTrendAnalysis("2026-07-10", "2026-07-11", true)).resolves.toBe("trend-job-1");
    expect(invoke).toHaveBeenCalledWith("queue_trend_analysis", { ...rangeArgs, force: true, activityScope: "all" });
  });

  it("exports trend Markdown through the native default path", async () => {
    vi.mocked(invoke).mockResolvedValueOnce("D:\\Reports\\trend.md");
    const rangeArgs = buildTrendRangeArguments("2026-07-10", "2026-07-11");
    const request: TrendWorkbenchRequest = {
      startDate: "2026-07-10",
      endDate: "2026-07-11",
      timezoneOffsetMinutes: -480,
      granularity: "month",
      metric: "linkedTaskSeconds",
      customBaseline: { startDate: "2026-06-10", endDate: "2026-06-11" },
    };

    await expect(exportTrendMarkdown(request)).resolves.toBe("D:\\Reports\\trend.md");
    expect(invoke).toHaveBeenCalledWith("export_trend_markdown", {
      ...rangeArgs,
      path: "",
      request,
    });
  });

  it("isolates trend research requests and legacy responses by activity scope", async () => {
    const request: TrendWorkbenchRequest = {
      startDate: "2026-07-10",
      endDate: "2026-07-11",
      timezoneOffsetMinutes: -480,
      granularity: "day",
      metric: "activeSeconds",
      activityScope: "meaningful",
      customBaseline: null,
    };
    const legacy = {
      status: "limitations_only" as const,
      findings: [],
      limitations: ["legacy"],
      source: "local",
      model: "policy-v1",
      evidenceHash: "scope-hash",
    };
    vi.mocked(invoke).mockResolvedValueOnce(legacy).mockResolvedValueOnce("trend-research-job");

    await expect(loadTrendResearchAnalysis(request)).resolves.toEqual({ ...legacy, activityScope: "all" });
    await expect(queueTrendResearchAnalysis(request, true)).resolves.toBe("trend-research-job");
    expect(invoke).toHaveBeenNthCalledWith(1, "get_trend_research_analysis", {
      request,
      activityScope: "meaningful",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "queue_trend_research_analysis", {
      request,
      force: true,
      activityScope: "meaningful",
    });
  });

  it("resolves unique executable identities in one native batch", async () => {
    vi.mocked(invoke).mockClear();
    vi.mocked(invoke).mockResolvedValueOnce([{
      rawName: "ChatGPT",
      displayName: "ChatGPT",
      executablePath: "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\\app\\ChatGPT.exe",
      productName: "Codex",
      iconDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    }]);
    const apps = [
      { app: "ChatGPT", appPath: "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\\app\\ChatGPT.exe" },
      { app: "ChatGPT", appPath: "C:\\Program Files\\WindowsApps\\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\\app\\ChatGPT.exe" },
    ];

    const identities = await resolveAppIdentities(apps);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("resolve_app_identities", {
      apps: [{ rawName: "ChatGPT", executablePath: apps[0].appPath }],
    });
    expect([...identities.values()][0].displayName).toBe("ChatGPT");
  });

  it("loads the work ledger with authoritative half-open range arguments", async () => {
    const ledger = {
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
    } satisfies WorkLedgerSnapshot;
    vi.mocked(invoke).mockResolvedValueOnce(ledger);

    await expect(loadWorkLedger(1_000, 2_000, "project-1")).resolves.toBe(ledger);
    expect(invoke).toHaveBeenCalledWith("get_work_ledger", {
      startMs: 1_000,
      endMs: 2_000,
      projectId: "project-1",
    });
  });

  it("saves manual work ledger assignments and protects suggested applications", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: "project-1" });
    await saveWorkLedgerProject({
      id: "project-1",
      name: "Desktop rewrite",
      color: "#3182ce",
      description: "",
    });
    expect(invoke).toHaveBeenCalledWith("save_work_ledger_project", {
      request: {
        id: "project-1",
        name: "Desktop rewrite",
        color: "#3182ce",
        description: "",
      },
    });

    await assignWorkLedgerEvidence({
      taskId: "task-1",
      evidenceKind: "activity",
      evidenceId: "segment-1",
      reason: "confirmed",
    });
    expect(invoke).toHaveBeenCalledWith("assign_work_ledger_evidence", {
      request: {
        taskId: "task-1",
        evidenceKind: "activity",
        evidenceId: "segment-1",
        reason: "confirmed",
      },
    });

    await applyWorkLedgerSuggestion({
      taskId: "task-1",
      evidenceKind: "activity",
      evidenceId: "segment-1",
      evidenceHash: "evidence-hash",
      source: "local",
      startMs: 1_000,
      endMs: 2_000,
      confidence: 0.72,
      reason: "local match",
    });
    expect(invoke).toHaveBeenCalledWith("apply_work_ledger_suggestion", {
      request: {
        taskId: "task-1",
        evidenceKind: "activity",
        evidenceId: "segment-1",
        evidenceHash: "evidence-hash",
        source: "local",
        startMs: 1_000,
        endMs: 2_000,
        confidence: 0.72,
        reason: "local match",
      },
    });
  });

  it("uses the exact task, status, progress, and removal command contracts", async () => {
    await saveWorkLedgerTask({
      id: "task-1",
      projectId: "project-1",
      title: "Bridge coverage",
      priority: "high",
      expectedOutput: "All wrappers",
      dueDate: null,
    });
    expect(invoke).toHaveBeenCalledWith("save_work_ledger_task", {
      request: {
        id: "task-1",
        projectId: "project-1",
        title: "Bridge coverage",
        priority: "high",
        expectedOutput: "All wrappers",
        dueDate: null,
      },
    });

    await updateWorkLedgerTaskStatus("task-1", "in_progress");
    expect(invoke).toHaveBeenCalledWith("update_work_ledger_task_status", {
      request: { taskId: "task-1", status: "in_progress" },
    });

    await addWorkLedgerProgress("task-1", "Implemented bridge coverage");
    expect(invoke).toHaveBeenCalledWith("add_work_ledger_progress", {
      request: { taskId: "task-1", note: "Implemented bridge coverage" },
    });

    await removeWorkLedgerEvidence({
      taskId: "task-1",
      evidenceKind: "browser",
      evidenceId: "visit-1",
    });
    expect(invoke).toHaveBeenCalledWith("remove_work_ledger_evidence", {
      request: {
        taskId: "task-1",
        evidenceKind: "browser",
        evidenceId: "visit-1",
      },
    });
  });

  it("uses the Task 4 daily goal, linking, focus, and output progress contracts", async () => {
    const goal: DailyGoalRecord = {
      date: "2026-07-13",
      goals: "Write bridge tests",
      expectedOutput: "Typed desktop calls",
      actualOutput: "Bridge tests pass",
    };
    const progress: WorkLedgerProgressEntry = {
      id: "progress-1",
      taskId: "task-1",
      note: "Bridge tests pass",
      createdAtMs: 1_000,
      originKind: "daily_actual_output",
      sourceId: "2026-07-13",
      sourceDate: "2026-07-13",
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce(goal)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ link: { goalRowId: "goal-2026", goalDate: goal.date, goalText: goal.goals, taskId: "task-1", confirmedAtMs: 1_000 }, task: { id: "task-1" }, taskCreated: false })
      .mockResolvedValueOnce(progress)
      .mockResolvedValueOnce("focus-1")
      .mockResolvedValueOnce(true);

    await expect(getDailyGoal(goal.date)).resolves.toBe(goal);
    await saveDailyGoal(goal);
    await expect(listDailyGoalTaskLinks(goal.date)).resolves.toEqual([]);
    await confirmDailyGoalTask({
      goalRowId: "goal-2026",
      goalDate: goal.date,
      goalText: goal.goals,
      taskId: "task-1",
    });
    await expect(recordDailyActualOutputProgress("task-1", goal.date)).resolves.toBe(progress);
    await expect(beginFocus(goal.date, goal.goals, 45, "task-1")).resolves.toBe("focus-1");
    await expect(completeFocus("focus-1", "Bridge tests pass")).resolves.toBe(true);

    expect(invoke).toHaveBeenNthCalledWith(1, "get_daily_goal", { date: goal.date });
    expect(invoke).toHaveBeenNthCalledWith(2, "save_daily_goal", goal);
    expect(invoke).toHaveBeenNthCalledWith(3, "list_daily_goal_task_links", { date: goal.date });
    expect(invoke).toHaveBeenNthCalledWith(4, "confirm_daily_goal_task", {
      request: {
        goalRowId: "goal-2026",
        goalDate: goal.date,
        goalText: goal.goals,
        taskId: "task-1",
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(5, "record_daily_actual_output_progress", {
      request: { taskId: "task-1", date: goal.date },
    });
    expect(invoke).toHaveBeenNthCalledWith(6, "start_focus_session", {
      goalDate: goal.date,
      goalText: goal.goals,
      plannedMinutes: 45,
      taskId: "task-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(7, "complete_focus_session", {
      id: "focus-1",
      outcome: "Bridge tests pass",
    });
  });

  it("creates deterministic non-numeric daily goal row identities for duplicates", () => {
    expect(buildDailyGoalRows("2026-07-13", "  Read  paper\nRead paper\n\nWrite tests ")).toEqual([
      { id: "goal-2026-07-13-Read%20paper-1", text: "Read paper", goalText: "Read  paper" },
      { id: "goal-2026-07-13-Read%20paper-2", text: "Read paper", goalText: "Read paper" },
      { id: "goal-2026-07-13-Write%20tests-1", text: "Write tests", goalText: "Write tests" },
    ]);
  });
});
