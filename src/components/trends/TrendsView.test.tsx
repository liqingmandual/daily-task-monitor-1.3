import { act } from "react";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { parseHTML } from "linkedom";
import { describe, expect, it, vi } from "vitest";
import type {
  TrendPayload,
  TrendResearchAnalysis as TrendResearchAnalysisResult,
  TrendWorkbenchPayload,
  TrendWorkbenchRequest,
} from "../../lib/desktop";
import { buildLocalTrendAnalysis } from "../../lib/trend-analysis";
import { ACTIVITY_SCOPE_STORAGE_KEYS } from "../../lib/activity-composition";
import {
  createTrendAnalysisRequestCoordinator,
  createTrendActionRequestCoordinator,
  createTrendRangeRequestCoordinator,
  createTrendWorkbenchRequestCoordinator,
  buildPreviewTrendPayload,
  buildPreviewWorkbenchPayload,
  TrendsView,
  TrendWorkbench,
  trendRangeKey,
  visibleTrendWorkbenchLoadState,
  visibleTrendLoadState,
  type TrendLoadState,
  type TrendAnalysisLoadState,
  type TrendActionState,
} from "./TrendsView";
import { mergeTrendBreakdowns } from "./TrendBreakdownPanel";
import { TrendResearchAnalysis } from "./TrendResearchAnalysis";
import { TrendDrilldown } from "./TrendDrilldown";

const payload: TrendPayload = {
  range: {
    startMs: 1,
    endMs: 2,
    startDate: "2026-07-06",
    endDate: "2026-07-12",
    dayCount: 7,
  },
  days: [
    {
      date: "2026-07-06",
      label: "周一",
      monitoredSeconds: 18_000,
      activeSeconds: 14_400,
      idleSeconds: 3_600,
      learningSeconds: 7_200,
      switchCount: 12,
      longestFocusSeconds: 3_600,
      classificationCoverage: 0.92,
      topCategory: { name: "research", seconds: 7_200, share: 0.5 },
      topApp: { name: "Chrome", seconds: 7_200, share: 0.5 },
    },
  ],
  summary: {
    monitoredSeconds: 18_000,
    activeSeconds: 14_400,
    idleSeconds: 3_600,
    learningSeconds: 7_200,
    switchCount: 12,
    longestFocusSeconds: 3_600,
    averageMonitoredSeconds: 18_000 / 7,
    averageActiveSeconds: 14_400 / 7,
    averageIdleSeconds: 3_600 / 7,
    averageLearningSeconds: 7_200 / 7,
    averageSwitchCount: 12 / 7,
    learningRatio: 0.5,
    switchesPerActiveHour: 3,
    productiveDayCount: 1,
    focusDayCount: 1,
    categoryBreakdown: [
      { name: "research", seconds: 7_200, share: 0.5 },
      { name: "creation_development", seconds: 7_200, share: 0.5 },
    ],
    appBreakdown: [
      { name: "Chrome", seconds: 7_200, share: 0.5 },
      { name: "Code", seconds: 7_200, share: 0.5 },
    ],
  },
  comparison: {
    previousRange: {
      startMs: 0,
      endMs: 1,
      startDate: "2026-06-29",
      endDate: "2026-07-05",
      dayCount: 7,
    },
    dayCount: 7,
    previousMonitoredSeconds: 14_400,
    previousActiveSeconds: 12_000,
    previousIdleSeconds: 2_400,
    previousLearningSeconds: 0,
    previousSwitchCount: 10,
    previousLongestFocusSeconds: 3_000,
    previousLearningRatio: 0,
    previousSwitchesPerActiveHour: 3,
    previousClassificationCoverage: 0.8,
    previousCategoryBreakdown: [{ name: "research", seconds: 6_000, share: 0.5 }],
    previousAppBreakdown: [{ name: "Chrome", seconds: 6_000, share: 0.5 }],
    monitoredSecondsDeltaPercent: 25,
    activeSecondsDeltaPercent: 20,
    idleSecondsDeltaPercent: 50,
    learningSecondsDeltaPercent: null,
    switchCountDeltaPercent: 20,
    longestFocusSecondsDeltaPercent: 20,
    learningRatioDeltaPercent: null,
    switchesPerActiveHourDeltaPercent: 0,
    classificationCoverageDeltaPercent: 15,
  },
  quality: {
    recordedDayCount: 1,
    missingDayCount: 6,
    classifiedSeconds: 16_560,
    pendingSeconds: 1_440,
    lowConfidenceSeconds: 600,
    classificationCoverage: 0.92,
  },
  workLedger: { startMs: 1, endMs: 2, projects: [], tasks: [] },
  evidenceHash: "abcdef0123456789",
};

const activityComposition = {
  all: {
    totalSeconds: 7_200,
    items: [
      {
        key: "creation_development" as const,
        category: "creation_development" as const,
        videoPurpose: null,
        seconds: 3_600,
        share: 0.5,
        meaningfulReason: "core" as const,
      },
      {
        key: "idle" as const,
        category: "idle" as const,
        videoPurpose: null,
        seconds: 3_600,
        share: 0.5,
        meaningfulReason: "excluded" as const,
      },
    ],
  },
  meaningful: {
    totalSeconds: 3_600,
    items: [{
      key: "creation_development" as const,
      category: "creation_development" as const,
      videoPurpose: null,
      seconds: 3_600,
      share: 1,
      meaningfulReason: "core" as const,
    }],
  },
};

const workbenchPayload: TrendWorkbenchPayload = {
  range: payload.range,
  granularity: "day",
  metric: "activeSeconds",
  metricAvailability: [
    "monitoredSeconds", "activeSeconds", "learningSeconds", "idleSeconds",
    "switchCount", "longestFocusSeconds", "classificationCoverage",
    "completedTaskCount", "linkedTaskSeconds",
  ].map((metric) => ({
    metric: metric as TrendWorkbenchPayload["metric"],
    status: "available" as const,
    reasonCode: null,
  })),
  buckets: [{
    id: "2026-07-06_2026-07-06",
    startDate: "2026-07-06",
    endDate: "2026-07-06",
    values: {
      monitoredSeconds: 18_000,
      activeSeconds: 14_400,
      learningSeconds: 7_200,
      idleSeconds: 3_600,
      switchCount: 12,
      longestFocusSeconds: 3_600,
      classificationCoverage: 0.92,
      completedTaskCount: 2,
      linkedTaskSeconds: 5_400,
    },
    recordedDayCount: 1,
    missingDayCount: 0,
    evidenceIds: ["bucket-evidence"],
    activityComposition,
    drilldown: {
      bucketId: "2026-07-06_2026-07-06",
      rawRows: [{
        rowId: "raw-1",
        bucketId: "2026-07-06_2026-07-06",
        evidenceKind: "activity",
        evidenceId: "activity-1",
        date: "2026-07-06",
        startTime: "09:00:00",
        endTime: "10:00:00",
        app: "Codex",
        titleSummary: "Codex / creation_development",
        category: "creation_development",
        videoPurpose: null,
        meaningful: true,
        meaningfulReason: "core",
        taskId: "task-1",
        taskTitle: "实现趋势工作台",
        projectId: "project-1",
        projectName: "每日任务监测系统",
        clippedDurationSeconds: 3_600,
        confidence: 0.92,
        reviewState: "pending",
        shared: true,
      }],
      applicationDistribution: [{ key: "codex", label: "Codex", seconds: 3_600 }],
      categoryDistribution: [{ key: "creation_development", label: "创作开发", seconds: 3_600 }],
      completedTasks: [{ taskId: "task-1", taskTitle: "实现趋势工作台", projectId: "project-1", projectName: "每日任务监测系统", completedAtMs: 1_752_000_000_000 }],
      linkedTaskRollups: [{ taskId: "task-1", taskTitle: "实现趋势工作台", projectId: "project-1", projectName: "每日任务监测系统", linkedSeconds: 3_600, activitySeconds: 3_600, focusSeconds: 900, evidenceCount: 2, sharedEvidenceCount: 1 }],
      linkedProjectRollups: [{ projectId: "project-1", projectName: "每日任务监测系统", linkedSeconds: 3_600, activitySeconds: 3_600, focusSeconds: 900, evidenceCount: 2, sharedEvidenceCount: 1 }],
      workflowOwnership: [{ ownershipId: "owner-1", evidenceKind: "activity", evidenceId: "activity-1", taskId: "task-1", taskTitle: "实现趋势工作台", projectId: "project-1", projectName: "每日任务监测系统", shared: true }],
      dataQuality: { recordedDayCount: 1, missingDayCount: 0, classifiedSeconds: 3_312, classificationCoverage: 0.92, lowConfidenceSeconds: 120, pendingSeconds: 300 },
    },
  }],
  summary: {
    totals: { monitoredSeconds: 18_000, activeSeconds: 14_400, learningSeconds: 7_200, idleSeconds: 3_600, switchCount: 12, longestFocusSeconds: 3_600, classificationCoverage: 0.92, completedTaskCount: 2, linkedTaskSeconds: 5_400 },
    dailyAverage: { monitoredSeconds: 18_000, activeSeconds: 14_400, learningSeconds: 7_200, idleSeconds: 3_600 },
    averageSampleDayCount: 1,
    switchesPerActiveHour: 3,
    meanPerBucket: { monitoredSeconds: 18_000, activeSeconds: 14_400, learningSeconds: 7_200, idleSeconds: 3_600, switchCount: 12, longestFocusSeconds: 3_600, classificationCoverage: 0.92, completedTaskCount: 2, linkedTaskSeconds: 5_400 },
    dailyMedian: { monitoredSeconds: 18_000, activeSeconds: 14_400, learningSeconds: 7_200, idleSeconds: 3_600, switchCount: 12, longestFocusSeconds: 3_600, classificationCoverage: 0.92, completedTaskCount: 2, linkedTaskSeconds: 5_400 },
    dailyMax: { monitoredSeconds: 18_000, activeSeconds: 14_400, learningSeconds: 7_200, idleSeconds: 3_600, switchCount: 12, longestFocusSeconds: 3_600, classificationCoverage: 0.92, completedTaskCount: 2, linkedTaskSeconds: 5_400 },
    dailySampleStddev: { monitoredSeconds: 0, activeSeconds: 0, learningSeconds: 0, idleSeconds: 0, switchCount: 0, longestFocusSeconds: 0, classificationCoverage: 0, completedTaskCount: 0, linkedTaskSeconds: 0 },
    dailyCoefficientOfVariation: { monitoredSeconds: 0, activeSeconds: 0, learningSeconds: 0, idleSeconds: 0, switchCount: 0, longestFocusSeconds: 0, classificationCoverage: 0, completedTaskCount: 0, linkedTaskSeconds: 0 },
    recordedDayCount: 1,
    effectiveActivityDayCount: 1,
    missingDayCount: 6,
    classifiedSeconds: 16_560,
    classificationCoverage: 0.92,
    lowConfidenceSeconds: 120,
    pendingSeconds: 300,
    evidenceIds: ["summary-evidence"],
  },
  activityComposition,
  baselines: [
    { kind: "current", range: { startDate: "2026-07-06", endDate: "2026-07-12" }, isValid: true, value: 14_400, absoluteDelta: 0, percentDelta: 0, recordedDayCount: 1, missingDayCount: 6, evidenceIds: ["current"] },
    { kind: "previousEqualLength", range: { startDate: "2026-06-29", endDate: "2026-07-05" }, isValid: true, value: 0, absoluteDelta: 14_400, percentDelta: null, recordedDayCount: 1, missingDayCount: 6, evidenceIds: ["previous"] },
    { kind: "previousMonthSamePeriod", range: { startDate: "2026-06-06", endDate: "2026-06-12" }, isValid: false, value: null, absoluteDelta: null, percentDelta: null, recordedDayCount: 0, missingDayCount: 7, evidenceIds: [] },
  ],
  evidence: [{
    id: "previousEqualLength.summary.activeSeconds",
    scope: "baseline",
    seriesKind: "previousEqualLength",
    bucketId: null,
    metric: "activeSeconds",
    value: 3_000,
  }],
  evidenceHash: "workbench-evidence",
};

const researchAnalysis: TrendResearchAnalysisResult = {
  activityScope: "all",
  status: "ready",
  findings: [{
    observation: "当前活动投入呈上升趋势",
    possibleExplanation: "这可能与更连续的工作安排有关",
    validationMethod: "在下一周期复核同一聚合指标",
    evidenceIds: ["previousEqualLength.summary.activeSeconds"],
    claims: [{
      evidenceId: "previousEqualLength.summary.activeSeconds",
      relation: "increased",
    }],
    confidence: 0.84,
    limitations: ["这只是待验证的相关性假设"],
  }],
  limitations: ["分析只覆盖已提供的聚合证据"],
  source: "openai",
  model: "gpt-test",
  evidenceHash: workbenchPayload.evidenceHash,
};

const callbacks = {
  onPresetChange: () => undefined,
  onCustomStartChange: () => undefined,
  onCustomEndChange: () => undefined,
  onShift: () => undefined,
  onRefresh: () => undefined,
};

function renderWorkbench(overrides: Partial<Parameters<typeof TrendWorkbench>[0]> = {}) {
  return renderToStaticMarkup(<TrendWorkbench
    preset="week"
    range={{ startDate: "2026-07-06", endDate: "2026-07-12" }}
    customStart="2026-07-06"
    customEnd="2026-07-12"
    status="ready"
    payload={payload}
    workbenchStatus="ready"
    workbenchPayload={workbenchPayload}
    error=""
    {...callbacks}
    {...overrides}
  />);
}

async function mountInteractiveTrendsView(options: {
  autoAnalysisEnabled?: boolean;
  analysis?: TrendResearchAnalysisResult;
} = {}) {
  const loadRange = vi.fn().mockResolvedValue(payload);
  const loadWorkbench = vi.fn().mockResolvedValue(workbenchPayload);
  const loadAnalysis = vi.fn().mockResolvedValue(options.analysis ?? researchAnalysis);
  const queueAnalysis = vi.fn().mockResolvedValue("queued-analysis");
  const exportMarkdown = vi.fn().mockResolvedValue("C:\\trend.md");
  const { document, window } = parseHTML('<!doctype html><html><body><div id="root"></div></body></html>');
  vi.stubGlobal("window", window);
  vi.stubGlobal("document", document);
  vi.stubGlobal("navigator", window.navigator);
  vi.stubGlobal("HTMLElement", window.HTMLElement);
  vi.stubGlobal("Node", window.Node);
  vi.stubGlobal("Event", window.Event);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const container = document.getElementById("root") as unknown as HTMLDivElement;
  const root = createRoot(container);
  await act(async () => root.render(<TrendsView
    anchorDate="2026-07-12"
    formatDuration={(seconds) => `${seconds} 秒`}
    loadRange={loadRange}
    loadWorkbench={loadWorkbench}
    loadAnalysis={loadAnalysis}
    queueAnalysis={queueAnalysis}
    autoAnalysisEnabled={options.autoAnalysisEnabled}
    exportMarkdown={exportMarkdown}
    desktopRuntime={() => true}
  />));
  return { container, document, window, root, loadRange, loadWorkbench, loadAnalysis, queueAnalysis, exportMarkdown };
}

async function click(mounted: Awaited<ReturnType<typeof mountInteractiveTrendsView>>, selector: string) {
  const target = mounted.container.querySelector(selector) as unknown as HTMLButtonElement;
  expect(target, `missing element: ${selector}`).toBeTruthy();
  await act(async () => target.dispatchEvent(new mounted.window.Event("click", { bubbles: true })));
}

describe("TrendResearchAnalysis", () => {
  const localAnalysis = buildLocalTrendAnalysis({
    metric: "activeSeconds",
    mean: 1_200,
    median: 1_100,
    sampleStandardDeviation: 200,
    coefficientOfVariation: 1 / 6,
    effectiveActivityDayCount: 5,
    classificationCoverage: 0.84,
  });
  const evidence = [{
    id: "previousEqualLength.summary.activeSeconds",
    scope: "baseline" as const,
    seriesKind: "previousEqualLength" as const,
    bucketId: null,
    metric: "activeSeconds" as const,
    value: 3_000,
  }];

  it("renders observation, hypothesis, validation, evidence, provenance and limitations", () => {
    const html = renderToStaticMarkup(<TrendResearchAnalysis
      localAnalysis={localAnalysis}
      analysis={researchAnalysis}
      analysisStatus="ready"
      evidence={evidence}
      evidenceHash={researchAnalysis.evidenceHash}
      formatDuration={(seconds) => `${seconds} 秒`}
    />);

    expect(html).toContain("AI 趋势洞察");
    expect(html).toContain("方法与数据质量");
    expect(html).toContain("当前活动投入呈上升趋势");
    expect(html).toContain("假设");
    expect(html).toContain("这可能与更连续的工作安排有关");
    expect(html).toContain("建议");
    expect(html).toContain("在下一周期复核同一聚合指标");
    expect(html).toContain('data-evidence-id="previousEqualLength.summary.activeSeconds"');
    expect(html).toContain("3000 秒");
    expect(html).toContain("openai");
    expect(html).toContain("gpt-test");
    expect(html).toContain("高置信度");
    expect(html).toContain("这只是待验证的相关性假设");
    expect(html).toContain("分析只覆盖已提供的聚合证据");
  });

  it("formats non-duration factual metrics with their own units", () => {
    const html = renderToStaticMarkup(<TrendResearchAnalysis
      localAnalysis={buildLocalTrendAnalysis({
        metric: "switchCount",
        mean: 12.5,
        median: 11,
        sampleStandardDeviation: 2.25,
        coefficientOfVariation: 0.18,
        effectiveActivityDayCount: 5,
        classificationCoverage: 0.84,
      })}
      analysis={null}
      analysisStatus="ready"
      evidence={[]}
      evidenceHash={researchAnalysis.evidenceHash}
      formatDuration={(seconds) => `${seconds} 秒`}
    />);

    expect(html).toContain("12.5");
    expect(html).toContain("2.25");
    expect(html).not.toContain("12.5 秒");
    expect(html).not.toContain("2.25 秒");
  });

  it("formats metric-less quality seconds evidence as duration", () => {
    const qualityEvidence = [{
      id: "current.summary.dataQuality.pendingSeconds",
      scope: "summary" as const,
      seriesKind: "current" as const,
      bucketId: null,
      metric: null,
      value: 3_600,
    }];
    const html = renderToStaticMarkup(<TrendResearchAnalysis
      localAnalysis={localAnalysis}
      analysis={{
        ...researchAnalysis,
        findings: [{
          ...researchAnalysis.findings[0],
          evidenceIds: [qualityEvidence[0].id],
          claims: [{ evidenceId: qualityEvidence[0].id, relation: "supports" }],
        }],
      }}
      analysisStatus="ready"
      evidence={qualityEvidence}
      evidenceHash={researchAnalysis.evidenceHash}
      formatDuration={(seconds) => `${seconds} 秒`}
    />);

    expect(html).toContain("3600 秒");
  });

  it("is read only and keeps facts plus limitations visible without findings", () => {
    const html = renderToStaticMarkup(<TrendResearchAnalysis
      localAnalysis={buildLocalTrendAnalysis({
        metric: "activeSeconds",
        mean: 600,
        median: 600,
        sampleStandardDeviation: null,
        coefficientOfVariation: null,
        effectiveActivityDayCount: 2,
        classificationCoverage: 0.4,
      })}
      analysis={{
        ...researchAnalysis,
        status: "limitations_only",
        findings: [],
        limitations: ["有效活动日不足，暂不生成研究结论"],
        source: "local",
        model: "policy-v1",
      }}
      analysisStatus="ready"
      evidence={evidence}
      evidenceHash={researchAnalysis.evidenceHash}
      formatDuration={(seconds) => `${seconds} 秒`}
    />);

    expect(html).toContain("均值");
    expect(html).toContain("中位数");
    expect(html).toContain("有效活动日不足，暂不生成研究结论");
    expect(html).not.toContain("<button");
    expect(html).not.toMatch(/转为任务|修改目标/);
  });

  it("formats switching-load evidence with a per-active-hour unit", () => {
    const switchingEvidence = [{
      id: "current.summary.switchesPerActiveHour",
      scope: "rate" as const,
      seriesKind: "current" as const,
      bucketId: null,
      metric: null,
      value: 2.5,
    }];
    const html = renderToStaticMarkup(<TrendResearchAnalysis
      localAnalysis={localAnalysis}
      analysis={{
        ...researchAnalysis,
        findings: [{
          ...researchAnalysis.findings[0],
          evidenceIds: [switchingEvidence[0].id],
          claims: [{ evidenceId: switchingEvidence[0].id, relation: "supports" }],
        }],
      }}
      analysisStatus="ready"
      evidence={switchingEvidence}
      evidenceHash={researchAnalysis.evidenceHash}
      formatDuration={(seconds) => `${seconds} 秒`}
    />);

    expect(html).toContain("2.5 次/活跃小时");
    expect(html).not.toContain("2.5 秒");
  });
});

describe("TrendWorkbench", () => {
  it("applies activity scope and display-key filters to drilldown evidence", () => {
    const bucket = workbenchPayload.buckets[0];
    const leisureRow = {
      ...bucket.drilldown.rawRows[0],
      rowId: "raw-leisure",
      evidenceId: "activity-leisure",
      app: "VideoPlayer",
      category: "video_input",
      videoPurpose: "leisure" as const,
      meaningful: false,
      meaningfulReason: "excluded" as const,
      taskId: null,
      taskTitle: null,
      projectId: null,
      projectName: null,
    };
    const filteredBucket = {
      ...bucket,
      drilldown: { ...bucket.drilldown, rawRows: [...bucket.drilldown.rawRows, leisureRow] },
    };
    const meaningfulHtml = renderToStaticMarkup(<TrendDrilldown
      bucket={filteredBucket}
      activityScope="meaningful"
      activityFilter="creation_development"
      formatDuration={(seconds) => `${seconds} 秒`}
    />);
    const leisureHtml = renderToStaticMarkup(<TrendDrilldown
      bucket={filteredBucket}
      activityScope="all"
      activityFilter="leisure_video"
      formatDuration={(seconds) => `${seconds} 秒`}
    />);

    expect(meaningfulHtml).toContain("分类筛选：");
    expect(meaningfulHtml).toContain("创作开发");
    expect(meaningfulHtml).toContain("Codex");
    expect(meaningfulHtml).not.toContain("VideoPlayer");
    expect(leisureHtml).toContain("休闲视频");
    expect(leisureHtml).toContain("VideoPlayer");
    expect(leisureHtml).not.toContain("Codex");
  });

  it("renders the reference-aligned dashboard before the expandable detail workbench", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    const labels = ["趋势概览", "总监测", "活动构成", "每日时间", "按星期分布", "AI 趋势洞察", "查看基准比较与明细下钻"];
    labels.reduce((previousIndex, label) => {
      const index = html.indexOf(label);
      expect(index).toBeGreaterThan(previousIndex);
      return index;
    }, -1);
    for (const metric of workbenchPayload.metricAvailability) {
      expect(html).toContain(`value="${metric.metric}"`);
    }
    expect(html).toContain("时间分桶");
    expect(html).toContain("基准比较");
    expect(html).toContain("数据质量");
    expect(html).toContain("样本标准差");
    expect(html).toContain("分类覆盖");
    expect(html).not.toContain("原始数据");
    const { document } = parseHTML(html);
    const topQuality = document.querySelector('[aria-label="数据质量"]')?.textContent ?? "";
    expect(topQuality).not.toContain("样本标准差");
    expect(topQuality).not.toContain("CV");
  });

  it("renders the authoritative learning composition without falling back to excluded activity", () => {
    const html = renderWorkbench({ activityScope: "meaningful" });
    const { document } = parseHTML(html);
    const composition = document.querySelector(".trend-composition-card");

    expect(composition?.querySelector('[aria-pressed="true"]')?.textContent).toBe("学习");
    expect(composition?.textContent).toContain("创作开发");
    expect(composition?.textContent).toContain("1 小时 0 分钟");
    expect(composition?.textContent).not.toContain("不活跃");
    expect(html).toContain("分析口径：学习");
    expect(html).not.toContain("有意义活动");
    expect(html).not.toContain("非娱乐活动");
  });

  it("shows an explicit zero state when meaningful composition is unavailable", () => {
    const html = renderWorkbench({
      activityScope: "meaningful",
      workbenchPayload: { ...workbenchPayload, activityComposition: undefined },
    });
    const { document } = parseHTML(html);
    const composition = document.querySelector(".trend-composition-card");

    expect(composition?.textContent).toContain("当前口径暂无可展示数据");
    expect(composition?.textContent).not.toContain("搜索/调研");
  });

  it("shows zero and unavailable baselines honestly and keeps custom baseline off by default", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    expect(html).toContain("基准为 0，百分比不可用");
    expect(html).toContain("无有效采样");
    expect(html).toContain('aria-label="启用自定义基准"');
    expect(html).toContain('aria-checked="false"');
    expect(html).not.toContain('data-baseline-kind="custom"');
  });

  it("disables unavailable metrics with an explanation", () => {
    const unavailable = {
      ...workbenchPayload,
      metricAvailability: workbenchPayload.metricAvailability.map((item) => item.metric === "linkedTaskSeconds"
        ? { ...item, status: "unavailable" as const, reasonCode: "metricUnavailable" as const }
        : item),
    };
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload: unavailable });
    expect(html).toMatch(/value="linkedTaskSeconds"[^>]*disabled/);
    expect(html).toContain("当前数据不可用");
  });

  it("shows custom baseline dates and the fourth comparison only after enabling it", () => {
    const withCustom = {
      ...workbenchPayload,
      baselines: [...workbenchPayload.baselines, {
        kind: "custom" as const,
        range: { startDate: "2026-05-01", endDate: "2026-05-07" },
        isValid: true,
        value: 10_800,
        absoluteDelta: 3_600,
        percentDelta: 33.3,
        recordedDayCount: 5,
        missingDayCount: 2,
        evidenceIds: ["custom"],
      }],
    };
    const html = renderWorkbench({
      workbenchStatus: "ready",
      workbenchPayload: withCustom,
      customBaselineEnabled: true,
      customBaselineStart: "2026-05-01",
      customBaselineEnd: "2026-05-07",
    });
    expect(html).toContain("基准开始");
    expect(html).toContain("基准结束");
    expect(html).toContain('aria-checked="true"');
    expect(html).toContain('data-baseline-kind="custom"');
  });

  it("uses the accessible time bucket as the drilldown entry point", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const root = createRoot(document.getElementById("root") as unknown as HTMLDivElement);
    try {
      await act(async () => root.render(<TrendWorkbench preset="week" range={{ startDate: "2026-07-06", endDate: "2026-07-12" }} customStart="" customEnd="" status="ready" payload={payload} workbenchStatus="ready" workbenchPayload={workbenchPayload} error="" {...callbacks} />));
      const bucketButton = document.querySelector(".trend-bucket-button") as unknown as HTMLButtonElement;
      expect(bucketButton.className).toContain("selected");
      expect(document.querySelector(".trend-drilldown")?.textContent).toContain("所选周期详情");
      await act(async () => bucketButton.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(bucketButton.className).toContain("selected");
      expect(document.querySelector(".trend-drilldown")?.textContent).toContain("应用分布");
      expect(document.querySelector(".trend-raw-evidence-table")).toBeNull();
    } finally {
      await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("switches drilldown evidence when another time bucket is selected", async () => {
    const secondBucketId = "2026-07-07_2026-07-07";
    const secondBucket = {
      ...workbenchPayload.buckets[0],
      id: secondBucketId,
      startDate: "2026-07-07",
      endDate: "2026-07-07",
      drilldown: {
        ...workbenchPayload.buckets[0].drilldown,
        bucketId: secondBucketId,
        rawRows: workbenchPayload.buckets[0].drilldown.rawRows.map((row) => ({
          ...row,
          rowId: "raw-2",
          bucketId: secondBucketId,
          date: "2026-07-07",
          app: "Chrome",
        })),
      },
    };
    const twoBucketPayload = { ...workbenchPayload, buckets: [...workbenchPayload.buckets, secondBucket] };
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const root = createRoot(document.getElementById("root") as unknown as HTMLDivElement);

    try {
      await act(async () => root.render(<TrendWorkbench preset="week" range={{ startDate: "2026-07-06", endDate: "2026-07-12" }} customStart="" customEnd="" status="ready" payload={payload} workbenchStatus="ready" workbenchPayload={twoBucketPayload} error="" {...callbacks} />));
      const secondBucketButton = document.querySelectorAll(".trend-bucket-button")[1] as HTMLButtonElement;
      await act(async () => secondBucketButton.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(document.querySelectorAll(".trend-bucket-button")[1].className).toContain("selected");
      expect(document.querySelector(".trend-drilldown")?.textContent).toContain("2026-07-07 至 2026-07-07");

      const firstBucketButton = document.querySelectorAll(".trend-bucket-button")[0] as HTMLButtonElement;
      await act(async () => firstBucketButton.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(document.querySelector(".trend-drilldown")?.textContent).toContain("2026-07-06 至 2026-07-06");
      expect(document.querySelector(".trend-raw-evidence-table")).toBeNull();
    } finally {
      await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("renders the evidence analysis before the expandable drilldown details", () => {
    const html = renderWorkbench({ analysisStatus: "ready", analysis: researchAnalysis });
    const labels = ["趋势概览", "活动构成", "每日时间", "按星期分布", "AI 趋势洞察", "方法与数据质量", "查看基准比较与明细下钻", "时间分桶"];
    labels.reduce((previousIndex, label) => {
      const index = html.indexOf(label);
      expect(index).toBeGreaterThan(previousIndex);
      return index;
    }, -1);
    expect(html).toContain('id="trend-comparison-chart-heading">基准比较');
    expect(html).toContain("所选周期详情");

    expect((html.match(/class="trend-baseline-item/g) ?? []).length).toBe(3);
    expect(html).toContain("基准为 0，百分比不可用");
    expect(html).toContain("无有效采样");
    expect(html).not.toMatch(/转为任务|修改目标/);
  });

  it("shows deterministic factual statistics while research analysis is loading", () => {
    const html = renderWorkbench({ analysisStatus: "loading", analysis: null });

    expect(html).toContain("方法与数据质量");
    expect(html).toContain("均值");
    expect(html).toContain("中位数");
    expect(html).toContain("正在读取研究分析");
  });

  it("renders matching evidence-validated research without replacing local facts", () => {
    const html = renderWorkbench({ analysisStatus: "ready", analysis: researchAnalysis });

    expect(html).toContain("方法与数据质量");
    expect(html).toContain("openai");
    expect(html).toContain("gpt-test");
    expect(html).toContain("当前活动投入呈上升趋势");
    expect(html).toContain("假设");
    expect(html).toContain("建议");
  });

  it("ignores research analysis whose evidence hash differs from the workbench", () => {
    const html = renderWorkbench({
      analysisStatus: "ready",
      analysis: { ...researchAnalysis, evidenceHash: "stale-hash", findings: [{ ...researchAnalysis.findings[0], observation: "STALE_PROVIDER_SUMMARY" }] },
    });

    expect(html).toContain("方法与数据质量");
    expect(html).toContain("研究分析与当前证据不匹配，仅显示事实统计");
    expect(html).not.toContain("STALE_PROVIDER_SUMMARY");
    expect(html).not.toContain("gpt-test");
  });

  it("surfaces stale-hash feedback even after the coordinator removes the analysis object", () => {
    const html = renderWorkbench({
      analysisStatus: "ready",
      analysis: null,
      analysisError: "分析证据已更新，已保留本地评价",
    });

    expect(html).toContain("分析证据已更新，已保留本地评价");
    expect(html).toContain("方法与数据质量");
  });

  it("shows research load errors without hiding deterministic local statistics", () => {
    const html = renderWorkbench({
      analysisStatus: "error",
      analysis: null,
      analysisError: "provider unavailable",
    });

    expect(html).toContain("AI 解释暂不可用");
    expect(html).toContain("provider unavailable");
    expect(html).toContain("方法与数据质量");
  });

  it("never publishes an older analysis request after a newer evidence hash", async () => {
    function deferred<T>() {
      let resolve!: (value: T) => void;
      const promise = new Promise<T>((next) => { resolve = next; });
      return { promise, resolve };
    }

    const first = deferred<TrendResearchAnalysisResult>();
    const second = deferred<TrendResearchAnalysisResult>();
    const loader = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const states: TrendAnalysisLoadState[] = [];
    const coordinator = createTrendAnalysisRequestCoordinator(loader, (state) => states.push(state));

    const workbenchRequest: TrendWorkbenchRequest = {
      startDate: "2026-07-06", endDate: "2026-07-12", timezoneOffsetMinutes: -480,
      granularity: "day", metric: "activeSeconds", customBaseline: null,
    };
    const firstRequest = coordinator.load(workbenchPayload, workbenchRequest);
    const newerPayload = { ...workbenchPayload, evidenceHash: "newer-evidence-hash" };
    const secondRequest = coordinator.load(newerPayload, workbenchRequest);
    first.resolve(researchAnalysis);
    await firstRequest;
    expect(states.at(-1)).toMatchObject({ status: "loading", evidenceHash: "newer-evidence-hash", analysis: null });

    const newerAnalysis = { ...researchAnalysis, evidenceHash: "newer-evidence-hash" };
    second.resolve(newerAnalysis);
    await secondRequest;
    expect(states.at(-1)).toMatchObject({ status: "ready", evidenceHash: "newer-evidence-hash", analysis: newerAnalysis });
  });

  it("rejects a trend analysis produced for a different activity scope", async () => {
    const loader = vi.fn().mockResolvedValue(researchAnalysis);
    const states: TrendAnalysisLoadState[] = [];
    const coordinator = createTrendAnalysisRequestCoordinator(loader, (state) => states.push(state));
    const request = {
      startDate: "2026-07-06",
      endDate: "2026-07-12",
      timezoneOffsetMinutes: -480,
      granularity: "day" as const,
      metric: "activeSeconds" as const,
      customBaseline: null,
      activityScope: "meaningful" as const,
    };

    await coordinator.load(workbenchPayload, request);

    expect(states.at(-1)).toMatchObject({
      status: "ready",
      evidenceHash: workbenchPayload.evidenceHash,
      analysis: null,
      error: "分析口径与当前选择不匹配，已保留本地评价",
    });
  });

  it("accepts a meaningful result only against the meaningful evidence hash", async () => {
    const meaningfulHash = "meaningful-evidence-hash";
    const meaningfulAnalysis = {
      ...researchAnalysis,
      activityScope: "meaningful" as const,
      evidenceHash: meaningfulHash,
    };
    const loader = vi.fn().mockResolvedValue(meaningfulAnalysis);
    const states: TrendAnalysisLoadState[] = [];
    const coordinator = createTrendAnalysisRequestCoordinator(loader, (state) => states.push(state));
    const request = {
      startDate: "2026-07-06",
      endDate: "2026-07-12",
      timezoneOffsetMinutes: -480,
      granularity: "day" as const,
      metric: "activeSeconds" as const,
      customBaseline: null,
      activityScope: "meaningful" as const,
    };

    await coordinator.load({
      ...workbenchPayload,
      analysisActivityScope: "meaningful",
      analysisEvidenceHash: meaningfulHash,
    }, request);

    expect(states.at(-1)).toMatchObject({
      status: "ready",
      evidenceHash: meaningfulHash,
      analysis: meaningfulAnalysis,
      error: "",
    });
  });

  it("uses accessible range controls and single-metric interactive period bars", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    expect(html).toContain('role="radiogroup"');
    expect(html).toContain('aria-label="近 7 天"');
    expect(html).toContain('aria-label="上一时间段"');
    expect(html).toContain('aria-label="下一时间段"');
    expect(html).toContain('aria-label="刷新趋势"');
    expect(html).toContain('aria-label="趋势指标"');
    expect(html).toContain('aria-label="活跃时长时间分桶柱状图"');
    expect(html).toContain('class="trend-bucket-button "');
    expect(html).toContain('aria-pressed="false"');
    expect(html).not.toContain('class="trend-learning-segment"');
    expect(html).not.toContain("goal-marker");
  });

  it("exposes accessible reanalysis and Markdown export controls", () => {
    const html = renderWorkbench({ nativeActionsAvailable: true });

    expect(html).toContain('aria-label="重新分析"');
    expect(html).toContain('title="重新分析"');
    expect(html).toContain('aria-label="导出 Markdown"');
    expect(html).toContain("导出 Markdown");
  });

  it("keeps browser preview non-crashing and explains desktop-only actions", () => {
    const html = renderWorkbench({ nativeActionsAvailable: false });

    expect(html).toContain("仅桌面版支持重新分析与 Markdown 导出");
    expect(html).toMatch(/aria-label="导出 Markdown"[^>]*disabled/);
    expect(html).toMatch(/aria-label="重新分析"[^>]*disabled/);
  });

  it("guards action feedback when the evidence hash changes", async () => {
    let resolve!: (value: string | null) => void;
    const pending = new Promise<string | null>((next) => { resolve = next; });
    const states: TrendActionState[] = [];
    const coordinator = createTrendActionRequestCoordinator((state) => states.push(state));

    coordinator.activate("first-hash");
    const request = coordinator.run("first-hash", "正在加入分析队列...", () => pending, () => "已加入分析队列");
    coordinator.activate("newer-hash");
    resolve("job-1");
    await request;

    expect(states.at(-1)).toEqual({ evidenceHash: "newer-hash", status: "idle", message: "" });
  });

  it("queues force reanalysis and exports Markdown with clear native feedback", async () => {
    const loadRange = vi.fn().mockResolvedValue(payload);
    const loadWorkbench = vi.fn().mockResolvedValue(workbenchPayload);
    const loadAnalysis = vi.fn().mockResolvedValue(researchAnalysis);
    const queueAnalysis = vi.fn().mockResolvedValue("job-1");
    const exportMarkdown = vi.fn().mockResolvedValue("D:\\Reports\\trend.md");
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);

    try {
      await act(async () => {
        root.render(<TrendsView
          anchorDate="2026-07-12"
          formatDuration={(seconds) => `${seconds} 秒`}
          loadRange={loadRange}
          loadWorkbench={loadWorkbench}
          loadAnalysis={loadAnalysis}
          queueAnalysis={queueAnalysis}
          exportMarkdown={exportMarkdown}
          desktopRuntime={() => true}
        />);
      });
      await act(async () => undefined);

      const reanalyzeButton = container.querySelector('button[aria-label="重新分析"]') as unknown as HTMLButtonElement;
      await act(async () => { reanalyzeButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
      expect(queueAnalysis).toHaveBeenCalledWith(expect.objectContaining({
        startDate: payload.range.startDate,
        endDate: payload.range.endDate,
        granularity: "day",
        metric: "activeSeconds",
      }), true);
      expect(container.textContent).toContain("已加入分析队列");

      const exportButton = container.querySelector('button[aria-label="导出 Markdown"]') as unknown as HTMLButtonElement;
      await act(async () => { exportButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
      expect(exportMarkdown).toHaveBeenCalledWith(expect.objectContaining({
        startDate: payload.range.startDate,
        endDate: payload.range.endDate,
        granularity: "day",
        metric: "activeSeconds",
        customBaseline: null,
      }));
      expect(container.textContent).toContain("已导出：D:\\Reports\\trend.md");
    } finally {
      await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("reports offline queueing and Markdown export errors", () => {
    expect(renderWorkbench({
      nativeActionsAvailable: true,
      reanalysisFeedback: { evidenceHash: payload.evidenceHash, status: "success", message: "当前离线或未配置可用 AI，继续使用本地统计" },
      exportFeedback: { evidenceHash: payload.evidenceHash, status: "error", message: "导出失败：磁盘不可写" },
    })).toContain("当前离线或未配置可用 AI，继续使用本地统计");
    expect(renderWorkbench({
      nativeActionsAvailable: true,
      exportFeedback: { evidenceHash: payload.evidenceHash, status: "error", message: "导出失败：磁盘不可写" },
    })).toContain("导出失败：磁盘不可写");
  });

  it("keeps trend bucket buttons interactive while breakdown rows stay out of the tab order", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    expect(html).toMatch(/<button[^>]+class="trend-bucket-button/);
    expect(html).toContain('aria-pressed="false"');
    expect(html).toContain('role="listitem"');
    expect(html).toMatch(/<button[^>]+trend-bucket-button/);
    expect(html).not.toMatch(/<button[^>]+trend-breakdown-row/);
    expect((html.match(/<button/g) ?? []).length).toBeGreaterThan(12);
  });

  it("merges current and previous breakdown names in stable order", () => {
    expect(mergeTrendBreakdowns(
      [
        { name: "A", seconds: 300, share: 0.6 },
        { name: "B", seconds: 200, share: 0.4 },
      ],
      [
        { name: "B", seconds: 100, share: 0.2 },
        { name: "C", seconds: 250, share: 0.5 },
        { name: "D", seconds: 150, share: 0.3 },
      ],
    )).toEqual([
      { name: "A", seconds: 300, share: 0.6, previousSeconds: 0 },
      { name: "B", seconds: 200, share: 0.4, previousSeconds: 100 },
      { name: "C", seconds: 0, share: 0, previousSeconds: 250 },
      { name: "D", seconds: 0, share: 0, previousSeconds: 150 },
    ]);
  });

  it("renders loading, error, and empty states explicitly", () => {
    expect(renderWorkbench({ status: "loading", payload: null })).toContain('role="status"');
    expect(renderWorkbench({ status: "loading", payload: null })).toContain("正在读取趋势数据");
    expect(renderWorkbench({ status: "error", payload: null, error: "读取失败" })).toContain('role="alert"');
    expect(renderWorkbench({ status: "error", payload: null, error: "读取失败" })).toContain("读取失败");
    expect(renderWorkbench({ status: "ready", payload: null })).toContain("当前范围暂无活动记录");
    expect(renderWorkbench({ payload: { ...payload, days: [], summary: { ...payload.summary, monitoredSeconds: 0, activeSeconds: 0 } } })).toContain("当前范围暂无活动记录");
    expect(renderWorkbench({ workbenchStatus: "loading" })).toContain("正在读取数据工作台");
    expect(renderWorkbench({ workbenchStatus: "error", workbenchError: "工作台读取失败" })).toContain("工作台读取失败");
    expect(renderWorkbench({ workbenchStatus: "ready", workbenchPayload: { ...workbenchPayload, buckets: [] } })).toContain("当前范围没有可用的时间分桶");
  });

  it("does not crash when the external anchor date is temporarily empty", () => {
    expect(() => renderToStaticMarkup(<TrendsView anchorDate="" formatDuration={(seconds) => `${seconds} 秒`} />)).not.toThrow();
  });

  it("never exposes an old payload across a rapid range change", async () => {
    function deferred<T>() {
      let resolve!: (value: T) => void;
      const promise = new Promise<T>((next) => { resolve = next; });
      return { promise, resolve };
    }

    const first = deferred<TrendPayload>();
    const second = deferred<TrendPayload>();
    const loader = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const states: TrendLoadState[] = [];
    const coordinator = createTrendRangeRequestCoordinator(loader, (state) => states.push(state));
    const firstRange = { startDate: "2026-07-06", endDate: "2026-07-12" };
    const secondRange = { startDate: "2026-06-13", endDate: "2026-07-12" };
    const readyFirst: TrendLoadState = {
      rangeKey: trendRangeKey(firstRange),
      status: "ready",
      payload,
      error: "",
    };

    expect(visibleTrendLoadState(readyFirst, trendRangeKey(secondRange))).toMatchObject({
      rangeKey: trendRangeKey(secondRange),
      status: "loading",
      payload: null,
    });

    const firstRequest = coordinator.load(firstRange);
    const secondRequest = coordinator.load(secondRange);
    expect(states.at(-1)).toMatchObject({ rangeKey: trendRangeKey(secondRange), status: "loading", payload: null });

    first.resolve(payload);
    await firstRequest;
    expect(states.at(-1)).toMatchObject({ rangeKey: trendRangeKey(secondRange), status: "loading", payload: null });

    const secondPayload = {
      ...payload,
      range: { ...payload.range, ...secondRange, dayCount: 30 },
      evidenceHash: "second-range",
    };
    second.resolve(secondPayload);
    await secondRequest;
    expect(states.at(-1)).toMatchObject({
      rangeKey: trendRangeKey(secondRange),
      status: "ready",
      payload: secondPayload,
    });
  });

  it("mounts TrendsView and never renders a stale payload during rapid range changes", async () => {
    function deferred<T>() {
      let resolve!: (value: T) => void;
      const promise = new Promise<T>((next) => { resolve = next; });
      return { promise, resolve };
    }

    function payloadWithMarker(marker: string, startDate: string, endDate: string, dayCount: number): TrendPayload {
      return {
        ...payload,
        range: { ...payload.range, startDate, endDate, dayCount },
        summary: {
          ...payload.summary,
          categoryBreakdown: [{ name: marker, seconds: payload.summary.activeSeconds, share: 1 }],
        },
        comparison: { ...payload.comparison, previousCategoryBreakdown: [] },
        evidenceHash: marker,
      };
    }

    const first = deferred<TrendPayload>();
    const staleSecond = deferred<TrendPayload>();
    const latestThird = deferred<TrendPayload>();
    const loader = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => staleSecond.promise)
      .mockImplementationOnce(() => latestThird.promise);
    const firstPayload = payloadWithMarker("FIRST_READY_PAYLOAD", "2026-07-06", "2026-07-12", 7);
    const stalePayload = payloadWithMarker("STALE_MONTH_PAYLOAD", "2026-06-13", "2026-07-12", 30);
    const latestPayload = payloadWithMarker("LATEST_WEEK_PAYLOAD", "2026-07-06", "2026-07-12", 7);
    const loadWorkbench = vi.fn().mockResolvedValue(workbenchPayload);
    const loadAnalysis = vi.fn().mockResolvedValue(researchAnalysis);
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    let root: Root | null = null;

    try {
      await act(async () => {
        root = createRoot(container);
        root.render(<TrendsView
          anchorDate="2026-07-12"
          formatDuration={(seconds) => `${seconds} 秒`}
          loadRange={loader}
          loadWorkbench={loadWorkbench}
          loadAnalysis={loadAnalysis}
          desktopRuntime={() => true}
        />);
      });
      expect(loader).toHaveBeenCalledTimes(1);

      await act(async () => {
        first.resolve(firstPayload);
        await first.promise;
      });
      expect(container.innerHTML).toContain("趋势统计概览");

      const monthButton = container.querySelector('[data-preset="month"]') as unknown as HTMLButtonElement;
      vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", false);
      flushSync(() => { monthButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
      expect(container.textContent).toContain("正在读取趋势数据");
      expect(container.innerHTML).not.toContain("趋势统计概览");
      expect(loader).toHaveBeenCalledTimes(1);

      const nextRangeButton = container.querySelector('button[aria-label="下一时间段"]') as unknown as HTMLButtonElement;
      flushSync(() => { nextRangeButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
      flushSync(() => { nextRangeButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
      expect(container.textContent).toContain("正在读取趋势数据");
      expect(container.innerHTML).not.toContain("趋势统计概览");
      expect(loader).toHaveBeenCalledTimes(1);

      vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
      await act(async () => undefined);
      expect(loader).toHaveBeenCalledTimes(2);
      expect(loader).toHaveBeenLastCalledWith("2026-08-12", "2026-09-10");
      expect(container.textContent).toContain("正在读取趋势数据");
      expect(container.innerHTML).not.toContain("趋势统计概览");

      const weekButton = container.querySelector('[data-preset="week"]') as unknown as HTMLButtonElement;
      await act(async () => { weekButton.dispatchEvent(new window.Event("click", { bubbles: true })); });
      expect(loader).toHaveBeenCalledTimes(3);
      expect(container.textContent).toContain("正在读取趋势数据");
      expect(container.innerHTML).not.toContain("趋势统计概览");

      await act(async () => {
        staleSecond.resolve(stalePayload);
        await staleSecond.promise;
      });
      expect(container.textContent).toContain("正在读取趋势数据");
      expect(container.innerHTML).not.toContain("趋势统计概览");

      await act(async () => {
        latestThird.resolve(latestPayload);
        await latestThird.promise;
      });
      expect(container.innerHTML).toContain("趋势统计概览");
    } finally {
      vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
      if (root) await act(async () => { root?.unmount(); });
      vi.unstubAllGlobals();
    }
  });

  it("hides the old payload and disables old actions in the render that receives a new anchorDate", async () => {
    const nextPayload = new Promise<TrendPayload>(() => undefined);
    const oldPayload: TrendPayload = {
      ...payload,
      evidenceHash: "OLD_ANCHOR_MARKER",
      summary: {
        ...payload.summary,
        categoryBreakdown: [{ name: "OLD_ANCHOR_MARKER", seconds: payload.summary.activeSeconds, share: 1 }],
      },
    };
    const loader = vi.fn()
      .mockResolvedValueOnce(oldPayload)
      .mockImplementationOnce(() => nextPayload);
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    const commonProps = {
      formatDuration: (seconds: number) => `${seconds} 秒`,
      loadRange: loader,
      loadWorkbench: vi.fn().mockResolvedValue(workbenchPayload),
      loadAnalysis: vi.fn().mockResolvedValue(researchAnalysis),
      desktopRuntime: () => true,
    };

    try {
      await act(async () => {
        root.render(<TrendsView anchorDate="2026-07-12" {...commonProps} />);
      });
      await act(async () => undefined);
      expect(container.innerHTML).toContain("趋势统计概览");

      flushSync(() => {
        root.render(<TrendsView anchorDate="2026-07-19" {...commonProps} />);
      });

      expect(container.textContent).toContain("正在读取趋势数据");
      expect(container.innerHTML).not.toContain("趋势统计概览");
      const exportButton = container.querySelector('button[aria-label="导出 Markdown"]') as unknown as HTMLButtonElement;
      expect(exportButton.disabled).toBe(true);
      expect(container.querySelector('button[aria-label="重新分析"]')).toBeNull();
    } finally {
      await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("cancels a scheduled range load when TrendsView unmounts before its microtask", async () => {
    const pending = new Promise<TrendPayload>(() => undefined);
    const loader = vi.fn(() => pending);
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", false);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    let unmounted = false;

    try {
      flushSync(() => {
        root.render(<TrendsView
          anchorDate="2026-07-12"
          formatDuration={(seconds) => `${seconds} 秒`}
          loadRange={loader}
          desktopRuntime={() => true}
        />);
      });
      expect(loader).not.toHaveBeenCalled();
      flushSync(() => root.unmount());
      unmounted = true;
      vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
      await act(async () => undefined);
      expect(loader).not.toHaveBeenCalled();
    } finally {
      vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
      if (!unmounted) await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("shows the concise dashboard while preserving quality details", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    expect(html).toContain("总监测");
    expect(html).toContain("完整时间口径");
    expect(html).toContain("分类覆盖");
    expect(html).toContain("方法与数据质量");
  });

  it("uses the workbench statistics DTO for effective activity days", () => {
    const html = renderWorkbench({
      workbenchPayload: {
        ...workbenchPayload,
        summary: {
          ...workbenchPayload.summary,
          recordedDayCount: 5,
          effectiveActivityDayCount: 2,
          missingDayCount: 2,
        },
      } as TrendWorkbenchPayload,
    });

    expect(html).toContain("<dt>有效活动日</dt><dd>2</dd>");
  });

  it("renders the six reference KPI cards without a secondary statistics strip", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });

    expect((html.match(/data-trend-overview-card="interval"/g) ?? [])).toHaveLength(6);
    expect((html.match(/data-trend-overview-card="daily-average"/g) ?? [])).toHaveLength(0);
    expect(html).toContain('data-trend-overview-metric="switchCount"');
    expect(html).toContain('data-responsive-columns="6 3 2"');
  });

  it("uses homepage metric colors and semantic time shares", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    const { document } = parseHTML(html);
    const card = (group: string, metric: string) => document.querySelector<HTMLElement>(`[data-trend-overview-card="${group}"][data-trend-overview-metric="${metric}"]`);

    expect(card("interval", "monitoredSeconds")?.dataset.trendAccent).toBe("#0f9f8f");
    expect(card("interval", "activeSeconds")?.dataset.trendAccent).toBe("#2563eb");
    expect(card("interval", "idleSeconds")?.dataset.trendAccent).toBe("#64748b");
    expect(card("interval", "learningSeconds")?.dataset.trendAccent).toBe("#e58a12");
    expect(card("interval", "switchCount")?.dataset.trendAccent).toBe("#8b5cf6");
    expect(card("interval", "longestFocusSeconds")?.dataset.trendAccent).toBe("#0f9f8f");

    expect(card("interval", "monitoredSeconds")?.dataset.trendShare).toBe("100.0%");
    expect(card("interval", "activeSeconds")?.dataset.trendShare).toBe("80.0%");
    expect(card("interval", "idleSeconds")?.dataset.trendShare).toBe("20.0%");
    expect(card("interval", "learningSeconds")?.dataset.trendShare).toBe("40.0%");
    expect(card("interval", "longestFocusSeconds")?.dataset.trendShare).toBe("20.0%");
    expect(card("interval", "switchCount")?.dataset.trendShare).toBeUndefined();
  });

  it("shows totals, all-selected-day averages, and monitored shares for duration metrics", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    const { document } = parseHTML(html);
    const card = (metric: string) => document.querySelector<HTMLElement>(`[data-trend-overview-metric="${metric}"]`);

    expect(card("monitoredSeconds")?.textContent).toContain("日均 42 分钟");
    expect(card("monitoredSeconds")?.textContent).toContain("占日均总监测 100.0%");
    expect(card("activeSeconds")?.textContent).toContain("日均 34 分钟");
    expect(card("activeSeconds")?.textContent).toContain("占日均总监测 80.0%");
    expect(card("learningSeconds")?.textContent).toContain("日均 17 分钟");
    expect(card("idleSeconds")?.textContent).toContain("日均 8 分钟");
    expect(card("longestFocusSeconds")?.textContent).not.toContain("日均");
    expect(card("longestFocusSeconds")?.textContent).toContain("占总监测 20.0%");
    expect(card("switchCount")?.textContent).not.toContain("日均");
    expect(card("switchCount")?.textContent).not.toContain("占");
    expect(card("switchCount")?.textContent).toContain("3.0 次/小时");
    expect(card("switchCount")?.textContent).toContain("共 12 次");
  });

  it("shows composition total, selected-day average, and monitored share while preserving scoped donut shares", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });
    const { document } = parseHTML(html);
    const composition = document.querySelector(".trend-composition-card");

    expect(composition?.textContent).toContain("创作开发");
    expect(composition?.textContent).toContain("总计 1 小时 0 分钟");
    expect(composition?.textContent).toContain("日均 8 分钟");
    expect(composition?.textContent).toContain("占总监测 20.0%");
    expect(composition?.textContent).toContain("构成 50%");
  });

  it("shows unavailable shares when there is no valid time denominator", () => {
    const zeroPayload = {
      ...workbenchPayload,
      summary: {
        ...workbenchPayload.summary,
        totals: {
          ...workbenchPayload.summary.totals,
          monitoredSeconds: 0,
          activeSeconds: 0,
          idleSeconds: 0,
          learningSeconds: 0,
          longestFocusSeconds: 0,
        },
        dailyAverage: { monitoredSeconds: null, activeSeconds: null, idleSeconds: null, learningSeconds: null },
        averageSampleDayCount: 0,
        switchesPerActiveHour: null,
      },
    } as TrendWorkbenchPayload;
    const { document } = parseHTML(renderWorkbench({ workbenchStatus: "ready", workbenchPayload: zeroPayload }));
    const shareCards = [...document.querySelectorAll<HTMLElement>("[data-trend-share]")];

    expect(shareCards).toHaveLength(5);
    expect(shareCards.every((item) => item.dataset.trendShare === "—")).toBe(true);
  });

  it("changes the chart metric when an overview card is clicked", async () => {
    const { document, window, root, container } = await mountInteractiveTrendsView();
    try {
      const chart = container.querySelector("[data-trend-chart-focus]") as HTMLElement;
      const labelsAtFocus: Array<string | null> = [];
      const focus = vi.fn(() => labelsAtFocus.push(chart.getAttribute("aria-label")));
      chart.focus = focus;
      const switchCard = container.querySelector('[data-trend-overview-metric="switchCount"]') as HTMLButtonElement;

      await act(async () => switchCard.dispatchEvent(new window.Event("click", { bubbles: true })));

      expect((container.querySelector('[data-trend-overview-metric="switchCount"]') as HTMLButtonElement).getAttribute("aria-pressed")).toBe("true");
      expect(container.textContent).toContain("切换次数");
      expect(focus).toHaveBeenCalledOnce();
      expect(labelsAtFocus).toEqual(["已聚焦：切换次数时间分桶图表"]);
      const focusedChart = container.querySelector("[data-trend-chart-focus]") as HTMLElement;
      expect(focusedChart.getAttribute("role")).toBe("region");
      expect(focusedChart.getAttribute("aria-label")).toBe("已聚焦：切换次数每日时间图表");
    } finally {
      await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("restores and persists the trends activity scope without touching the today preference", async () => {
    const getItem = vi.fn((key: string) => key === ACTIVITY_SCOPE_STORAGE_KEYS.trends ? "meaningful" : null);
    const setItem = vi.fn();
    vi.stubGlobal("localStorage", { getItem, setItem });
    const mounted = await mountInteractiveTrendsView();
    try {
      const pressed = mounted.container.querySelector('.trend-composition-toggle [aria-pressed="true"]');
      expect(pressed?.textContent).toBe("学习");
      expect(mounted.loadWorkbench).toHaveBeenCalledWith(expect.objectContaining({ activityScope: "meaningful" }));
      expect(getItem).toHaveBeenCalledWith(ACTIVITY_SCOPE_STORAGE_KEYS.trends);
      expect(getItem).not.toHaveBeenCalledWith(ACTIVITY_SCOPE_STORAGE_KEYS.today);
      expect(setItem).toHaveBeenCalledWith(ACTIVITY_SCOPE_STORAGE_KEYS.trends, "meaningful");
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("auto-queues a local meaningful analysis only when deep analysis automation is enabled", async () => {
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => key === ACTIVITY_SCOPE_STORAGE_KEYS.trends ? "meaningful" : null,
      setItem: vi.fn(),
    });
    const mounted = await mountInteractiveTrendsView({
      autoAnalysisEnabled: true,
      analysis: {
        ...researchAnalysis,
        activityScope: "meaningful",
        source: "local",
      },
    });
    try {
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(mounted.queueAnalysis).toHaveBeenCalledTimes(1);
      expect(mounted.queueAnalysis).toHaveBeenCalledWith(
        expect.objectContaining({ activityScope: "meaningful" }),
        false,
      );
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("exposes the requested desktop, medium, and narrow responsive column structures", () => {
    const html = renderWorkbench({ workbenchStatus: "ready", workbenchPayload });

    expect(html).toContain('class="trend-overview-grid interval" data-responsive-columns="6 3 2"');
    expect(html).not.toContain('class="trend-overview-grid daily-average"');
  });

  it("resolves every research evidence chip against the current workbench", () => {
    const html = renderWorkbench({ analysisStatus: "ready", analysis: researchAnalysis });
    const { document } = parseHTML(html);
    const chips = [...document.querySelectorAll<HTMLElement>("[data-evidence-id]")];

    expect(chips).toHaveLength(1);
    for (const chip of chips) {
      expect(workbenchPayload.evidence.some((item) => item.id === chip.dataset.evidenceId)).toBe(true);
    }
    expect(chips[0].textContent).toContain("50 分钟");
  });
});

describe("trend workbench requests", () => {
  it("hides an old workbench payload synchronously before the next effect runs", () => {
    const stale = { requestKey: "old", status: "ready" as const, payload: workbenchPayload, error: "" };
    expect(visibleTrendWorkbenchLoadState(stale, "new")).toEqual({ requestKey: "new", status: "loading", payload: null, error: "" });
  });
  it("drops an older workbench response after metric or granularity changes", async () => {
    let resolveFirst!: (value: TrendWorkbenchPayload) => void;
    let resolveSecond!: (value: TrendWorkbenchPayload) => void;
    const first = new Promise<TrendWorkbenchPayload>((resolve) => { resolveFirst = resolve; });
    const second = new Promise<TrendWorkbenchPayload>((resolve) => { resolveSecond = resolve; });
    const loader = vi.fn().mockReturnValueOnce(first).mockReturnValueOnce(second);
    const states: Array<{ status: string; payload: TrendWorkbenchPayload | null }> = [];
    const coordinator = createTrendWorkbenchRequestCoordinator(loader, (state) => states.push(state));
    const base: TrendWorkbenchRequest = { startDate: "2026-07-06", endDate: "2026-07-12", timezoneOffsetMinutes: -480, granularity: "day", metric: "activeSeconds", customBaseline: null };

    const oldRequest = coordinator.load(base);
    const newRequest = coordinator.load({ ...base, granularity: "week", metric: "learningSeconds" });
    resolveFirst(workbenchPayload);
    await oldRequest;
    expect(states.at(-1)?.status).toBe("loading");
    resolveSecond({ ...workbenchPayload, granularity: "week", metric: "learningSeconds" });
    await newRequest;
    expect(states.at(-1)?.payload).toMatchObject({ granularity: "week", metric: "learningSeconds" });
    expect(loader).toHaveBeenLastCalledWith(expect.objectContaining({ timezoneOffsetMinutes: -480, customBaseline: null }));
  });

  it("uses local timezone, session granularity override, and opt-in custom baseline", async () => {
    const loadRange = vi.fn().mockResolvedValue(payload);
    const loadWorkbench = vi.fn().mockResolvedValue(workbenchPayload);
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const root = createRoot(document.getElementById("root") as unknown as HTMLDivElement);
    try {
      await act(async () => root.render(<TrendsView anchorDate="2026-07-12" formatDuration={(seconds) => `${seconds} 秒`} loadRange={loadRange} loadWorkbench={loadWorkbench} loadAnalysis={vi.fn().mockResolvedValue(researchAnalysis)} desktopRuntime={() => true} />));
      expect(loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({ granularity: "day", metric: "activeSeconds", timezoneOffsetMinutes: new Date().getTimezoneOffset(), customBaseline: null }));
      const weekButton = [...document.querySelectorAll(".trend-granularity button")].find((button) => button.textContent === "周") as unknown as HTMLButtonElement;
      await act(async () => weekButton.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({ granularity: "week" }));
      const baselineToggle = document.querySelector('[role="switch"]') as unknown as HTMLButtonElement;
      await act(async () => baselineToggle.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({ customBaseline: { startDate: "2026-06-29", endDate: "2026-07-05" } }));
    } finally {
      await act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("keeps specific-date clicks as draft until apply, then updates requests and removable chips", async () => {
    const mounted = await mountInteractiveTrendsView();
    try {
      await click(mounted, '[data-preset="custom"]');
      await click(mounted, '[data-custom-mode="specific"]');
      const workbenchCallsBeforeDraft = mounted.loadWorkbench.mock.calls.length;
      const analysisCallsBeforeDraft = mounted.loadAnalysis.mock.calls.length;

      const july8 = mounted.container.querySelector('[data-date="2026-07-08"]') as unknown as HTMLButtonElement;
      expect(july8.getAttribute("aria-pressed")).toBe("true");
      await act(async () => july8.dispatchEvent(new mounted.window.Event("click", { bubbles: true })));
      expect((mounted.container.querySelector('[data-date="2026-07-08"]') as Element).getAttribute("aria-pressed")).toBe("false");
      expect(mounted.loadWorkbench).toHaveBeenCalledTimes(workbenchCallsBeforeDraft);
      expect(mounted.loadAnalysis).toHaveBeenCalledTimes(analysisCallsBeforeDraft);

      await click(mounted, 'button[aria-label="导出 Markdown"]');
      expect(mounted.exportMarkdown).toHaveBeenLastCalledWith(expect.objectContaining({
        selectedDates: ["2026-07-06", "2026-07-07", "2026-07-08", "2026-07-09", "2026-07-10", "2026-07-11", "2026-07-12"],
      }));

      await click(mounted, 'button[data-action="apply-specific-dates"]');
      expect(mounted.loadWorkbench).toHaveBeenCalledTimes(workbenchCallsBeforeDraft + 1);
      expect(mounted.loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({
        startDate: "2026-07-06",
        endDate: "2026-07-12",
        selectedDates: ["2026-07-06", "2026-07-07", "2026-07-09", "2026-07-10", "2026-07-11", "2026-07-12"],
      }));
      expect(mounted.container.textContent).toContain("已选 6 天");
      expect(mounted.container.textContent).toContain("2026-07-06 至 2026-07-12");

      const callsBeforeRemoval = mounted.loadWorkbench.mock.calls.length;
      await click(mounted, 'button[aria-label="移除 2026-07-09"]');
      expect(mounted.loadWorkbench).toHaveBeenCalledTimes(callsBeforeRemoval + 1);
      expect(mounted.loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({
        selectedDates: ["2026-07-06", "2026-07-07", "2026-07-10", "2026-07-11", "2026-07-12"],
      }));
      expect((mounted.container.querySelector('[data-date="2026-07-09"]') as Element).getAttribute("aria-pressed")).toBe("false");

      await click(mounted, 'button[data-action="clear-specific-dates"]');
      const callsBeforeInvalidApply = mounted.loadWorkbench.mock.calls.length;
      await click(mounted, 'button[data-action="apply-specific-dates"]');
      expect(mounted.loadWorkbench).toHaveBeenCalledTimes(callsBeforeInvalidApply);
      expect(mounted.container.querySelector('[role="alert"]')?.textContent).toContain("请至少选择 1 天");
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("never calls the legacy range loader in sparse mode and stays ready from workbench state", async () => {
    const mounted = await mountInteractiveTrendsView();
    try {
      await click(mounted, '[data-preset="custom"]');
      const rangeCallsBeforeSpecific = mounted.loadRange.mock.calls.length;
      mounted.loadRange.mockImplementation(() => new Promise<TrendPayload>(() => undefined));
      await click(mounted, '[data-custom-mode="specific"]');

      expect(mounted.loadRange).toHaveBeenCalledTimes(rangeCallsBeforeSpecific);
      expect(mounted.container.textContent).toContain("趋势概览");
      expect(mounted.container.textContent).not.toContain("正在读取趋势数据");

      await click(mounted, '[data-date="2026-07-08"]');
      await click(mounted, 'button[data-action="apply-specific-dates"]');
      const weekGranularity = [...mounted.container.querySelectorAll(".trend-granularity button")]
        .find((button) => button.textContent === "周") as unknown as HTMLButtonElement;
      await act(async () => weekGranularity.dispatchEvent(new mounted.window.Event("click", { bubbles: true })));
      await click(mounted, 'button[aria-label="启用自定义基准"]');

      expect(mounted.loadRange).toHaveBeenCalledTimes(rangeCallsBeforeSpecific);
      expect(mounted.loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({
        selectedDates: ["2026-07-06", "2026-07-07", "2026-07-09", "2026-07-10", "2026-07-11", "2026-07-12"],
        granularity: "week",
        customBaseline: { startDate: "2026-06-29", endDate: "2026-07-05" },
      }));
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("removing an applied chip preserves unrelated draft additions", async () => {
    const mounted = await mountInteractiveTrendsView();
    try {
      await click(mounted, '[data-preset="custom"]');
      await click(mounted, '[data-custom-mode="specific"]');
      await click(mounted, '[data-date="2026-07-20"]');
      expect((mounted.container.querySelector('[data-date="2026-07-20"]') as Element).getAttribute("aria-pressed")).toBe("true");

      await click(mounted, 'button[aria-label="移除 2026-07-09"]');
      expect(mounted.loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({
        selectedDates: ["2026-07-06", "2026-07-07", "2026-07-08", "2026-07-10", "2026-07-11", "2026-07-12"],
      }));
      expect((mounted.container.querySelector('[data-date="2026-07-20"]') as Element).getAttribute("aria-pressed")).toBe("true");
      expect((mounted.container.querySelector('[data-date="2026-07-09"]') as Element).getAttribute("aria-pressed")).toBe("false");
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("removes the final applied chip and enters an invalid no-request state", async () => {
    const mounted = await mountInteractiveTrendsView();
    try {
      await click(mounted, '[data-preset="custom"]');
      await click(mounted, '[data-custom-mode="specific"]');
      await click(mounted, 'button[data-action="clear-specific-dates"]');
      await click(mounted, '[data-date="2026-07-09"]');
      await click(mounted, 'button[data-action="apply-specific-dates"]');
      const workbenchCallsBeforeRemoval = mounted.loadWorkbench.mock.calls.length;

      await click(mounted, 'button[aria-label="移除 2026-07-09"]');
      expect(mounted.container.querySelector('button[aria-label="移除 2026-07-09"]')).toBeNull();
      expect(mounted.container.textContent).toContain("尚未应用指定日期");
      expect(mounted.container.querySelector('[role="alert"]')?.textContent).toContain("请至少选择 1 天");
      expect(mounted.loadWorkbench).toHaveBeenCalledTimes(workbenchCallsBeforeRemoval);
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("shifts every applied specific date by its envelope span and requests exactly once", async () => {
    const mounted = await mountInteractiveTrendsView();
    try {
      await click(mounted, '[data-preset="custom"]');
      await click(mounted, '[data-custom-mode="specific"]');
      await click(mounted, 'button[data-action="clear-specific-dates"]');
      await click(mounted, '[data-date="2026-07-06"]');
      await click(mounted, '[data-date="2026-07-08"]');
      await click(mounted, '[data-date="2026-07-12"]');
      await click(mounted, 'button[data-action="apply-specific-dates"]');

      const callsBeforeShift = mounted.loadWorkbench.mock.calls.length;
      await click(mounted, 'button[aria-label="下一时间段"]');
      expect(mounted.loadWorkbench).toHaveBeenCalledTimes(callsBeforeShift + 1);
      expect(mounted.loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({
        startDate: "2026-07-13",
        endDate: "2026-07-19",
        selectedDates: ["2026-07-13", "2026-07-15", "2026-07-19"],
      }));
      expect(mounted.container.textContent).toContain("已选 3 天");
      expect(mounted.container.textContent).toContain("2026-07-13 至 2026-07-19");
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("preserves continuous and specific custom selections while omitting selectedDates in continuous mode", async () => {
    const mounted = await mountInteractiveTrendsView();
    try {
      await click(mounted, '[data-preset="custom"]');
      expect((mounted.container.querySelector('input[aria-label="开始日期"]') as unknown as HTMLInputElement).value).toBe("2026-07-06");

      await click(mounted, '[data-custom-mode="specific"]');
      await click(mounted, 'button[data-action="clear-specific-dates"]');
      await click(mounted, '[data-date="2026-07-03"]');
      await click(mounted, '[data-date="2026-07-11"]');
      await click(mounted, 'button[data-action="apply-specific-dates"]');
      expect(mounted.loadWorkbench).toHaveBeenLastCalledWith(expect.objectContaining({ selectedDates: ["2026-07-03", "2026-07-11"] }));

      await click(mounted, '[data-custom-mode="continuous"]');
      expect((mounted.container.querySelector('input[aria-label="开始日期"]') as unknown as HTMLInputElement).value).toBe("2026-07-06");
      expect(mounted.loadWorkbench.mock.calls.at(-1)?.[0]).not.toHaveProperty("selectedDates");

      await click(mounted, '[data-custom-mode="specific"]');
      expect(mounted.container.textContent).toContain("已选 2 天");
      expect((mounted.container.querySelector('[data-date="2026-07-03"]') as Element).getAttribute("aria-pressed")).toBe("true");
      expect((mounted.container.querySelector('[data-date="2026-07-11"]') as Element).getAttribute("aria-pressed")).toBe("true");
    } finally {
      await act(async () => mounted.root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it("keeps browser preview renderable without calling either desktop loader", async () => {
    const loadRange = vi.fn();
    const loadWorkbench = vi.fn();
    const html = renderToStaticMarkup(<TrendsView anchorDate="2026-07-12" formatDuration={(seconds) => `${seconds} 秒`} loadRange={loadRange} loadWorkbench={loadWorkbench} desktopRuntime={() => false} />);
    expect(html).toContain("趋势概览");
    expect(loadRange).not.toHaveBeenCalled();
    expect(loadWorkbench).not.toHaveBeenCalled();
  });

  it("builds preview facts across the requested range and clips week and month buckets", () => {
    const request = { startDate: "2026-07-09", endDate: "2026-07-15", timezoneOffsetMinutes: -480, metric: "activeSeconds" as const, customBaseline: { startDate: "2026-06-01", endDate: "2026-06-07" } };
    const legacy = buildPreviewTrendPayload(request);
    expect(legacy.range.dayCount).toBe(7);
    expect(legacy.days.map((day) => day.date)).toEqual(["2026-07-09", "2026-07-10", "2026-07-11", "2026-07-12", "2026-07-13", "2026-07-14", "2026-07-15"]);
    const weekly = buildPreviewWorkbenchPayload({ ...request, granularity: "week" });
    expect(weekly.buckets.map((bucket) => [bucket.startDate, bucket.endDate])).toEqual([["2026-07-09", "2026-07-12"], ["2026-07-13", "2026-07-15"]]);
    const monthly = buildPreviewWorkbenchPayload({ ...request, granularity: "month" });
    expect(monthly.buckets.map((bucket) => [bucket.startDate, bucket.endDate])).toEqual([["2026-07-09", "2026-07-15"]]);
    expect(weekly.baselines.find((series) => series.kind === "custom")?.range).toEqual(request.customBaseline);
  });

  it("builds browser preview facts and buckets from selected dates instead of their full envelope", () => {
    const request: TrendWorkbenchRequest = {
      startDate: "2026-07-06",
      endDate: "2026-07-12",
      selectedDates: ["2026-07-06", "2026-07-08", "2026-07-12"],
      timezoneOffsetMinutes: -480,
      granularity: "day",
      metric: "activeSeconds",
      customBaseline: null,
    };
    const legacy = buildPreviewTrendPayload(request);
    const preview = buildPreviewWorkbenchPayload(request);
    expect(legacy.days.map((day) => day.date)).toEqual(request.selectedDates);
    expect(preview.buckets.map((bucket) => bucket.startDate)).toEqual(request.selectedDates);
    expect(preview.range).toMatchObject({
      selectionMode: "selectedDates",
      selectedDates: request.selectedDates,
      selectedDateCount: 3,
      envelopeDayCount: 7,
      dayCount: 3,
    });
  });

  it("clips cross-year Monday buckets without omitting request dates", () => {
    const payload = buildPreviewWorkbenchPayload({ startDate: "2025-12-31", endDate: "2026-01-06", timezoneOffsetMinutes: -480, granularity: "week", metric: "activeSeconds", customBaseline: null });
    expect(payload.buckets.map((bucket) => [bucket.startDate, bucket.endDate])).toEqual([["2025-12-31", "2026-01-04"], ["2026-01-05", "2026-01-06"]]);
    expect(payload.range.dayCount).toBe(7);
  });

  it("computes preview workbench bucket and daily statistics from generated facts", () => {
    const request = { startDate: "2026-07-09", endDate: "2026-07-15", timezoneOffsetMinutes: -480, metric: "activeSeconds" as const, customBaseline: null };
    const monthly = buildPreviewWorkbenchPayload({ ...request, granularity: "month" });
    expect(monthly.buckets).toHaveLength(1);
    expect(monthly.summary.meanPerBucket.activeSeconds).toBe(monthly.summary.totals.activeSeconds);
    expect(monthly.summary.dailyMedian.activeSeconds).toBe(13_950);
    expect(monthly.summary.dailyMax.activeSeconds).toBe(16_200);

    const weekly = buildPreviewWorkbenchPayload({ ...request, granularity: "week" });
    expect(weekly.summary.meanPerBucket.activeSeconds).toBe((weekly.buckets[0].values.activeSeconds + weekly.buckets[1].values.activeSeconds) / 2);
    const daily = buildPreviewWorkbenchPayload({ ...request, granularity: "day" });
    expect(daily.summary.meanPerBucket.activeSeconds).toBe(monthly.summary.totals.activeSeconds / 7);
    expect(daily.summary.dailyMedian.activeSeconds).toBe(monthly.summary.dailyMedian.activeSeconds);
    expect(daily.summary.dailyMax.activeSeconds).toBe(monthly.summary.dailyMax.activeSeconds);
  });

  it("renders preview comparison baselines with finite values instead of NaN", () => {
    const request = { startDate: "2026-07-09", endDate: "2026-07-15", timezoneOffsetMinutes: -480, granularity: "day" as const, metric: "activeSeconds" as const, customBaseline: null };
    const preview = buildPreviewWorkbenchPayload(request);
    const priorBaselines = preview.baselines.filter((series) => series.kind !== "current");
    expect(priorBaselines).toHaveLength(2);
    expect(priorBaselines.every((series) => series.value !== null && Number.isFinite(series.value) && series.absoluteDelta !== null && Number.isFinite(series.absoluteDelta))).toBe(true);
    const html = renderWorkbench({ range: { startDate: request.startDate, endDate: request.endDate }, payload: buildPreviewTrendPayload(request), workbenchStatus: "ready", workbenchPayload: preview });
    expect(html).toContain("上一等长区间");
    expect(html).toContain("上月同期");
    expect(html).not.toContain("NaN");
  });
});
