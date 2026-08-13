import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { buildDashboardMetrics, type Segment } from "../../lib/metrics";
import { appIdentityKey, type AppIdentity } from "../../lib/app-identity";
import { buildFallbackActivityCompositions } from "../../lib/activity-composition";
import { appRankingFilter } from "./AppRankingPanel";
import { TodayAnalysisPanels } from "./TodayAnalysisPanels";
import timeDistributionPanelSource from "./TimeDistributionPanel.tsx?raw";

const segments: Segment[] = [
  {
    id: "research",
    startMs: 8 * 3_600_000,
    endMs: 9 * 3_600_000,
    app: "Chrome",
    title: "Research",
    category: "research",
    videoPurpose: "unknown",
    confidence: 0.95,
    needsReview: false,
  },
  {
    id: "writing",
    startMs: 14 * 3_600_000,
    endMs: 15 * 3_600_000,
    app: "Obsidian",
    title: "Notes",
    category: "text_input",
    videoPurpose: "unknown",
    confidence: 0.95,
    needsReview: false,
  },
];

describe("TodayAnalysisPanels", () => {
  it("renders the approved three-column analysis contract", () => {
    const html = renderToStaticMarkup(
      <TodayAnalysisPanels
        metrics={buildDashboardMetrics(segments)}
        activityCompositions={buildFallbackActivityCompositions(segments)}
        activityScope="all"
        onActivityScopeChange={() => {}}
        selectedSeries={["active", "learning"]}
        onSeriesChange={() => {}}
        onDrill={() => {}}
      />,
    );

    expect(html).toContain('data-analysis-layout="three-column"');
    expect(html).toContain("活动构成");
    expect(html).toContain('aria-label="今日活动构成口径"');
    expect(html).toContain("应用排行");
    expect(html).toContain("时间分布");
    expect((html.match(/data-time-bucket=/g) ?? []).length).toBe(12);
    expect(html).toContain("120 min");
  });

  it("wires the body portal tooltip to pointer and keyboard focus handlers", () => {
    expect(timeDistributionPanelSource).toContain("createPortal(");
    expect(timeDistributionPanelSource).toContain("document.body");
    expect(timeDistributionPanelSource).toContain("onPointerEnter");
    expect(timeDistributionPanelSource).toContain("onPointerMove");
    expect(timeDistributionPanelSource).toContain("onPointerLeave");
    expect(timeDistributionPanelSource).toContain("onFocus");
    expect(timeDistributionPanelSource).toContain("onBlur");
  });

  it("uses the resolved identity and shared native icon in app ranking", () => {
    const appPath = "C:\\Program Files\\Microsoft VS Code\\Code.exe";
    const identity: AppIdentity = {
      rawName: "Code",
      displayName: "Visual Studio Code",
      executablePath: appPath,
      productName: "Visual Studio Code",
      iconDataUrl: "data:image/png;base64,iVBORw0KGgo=",
    };
    const metrics = buildDashboardMetrics([{ ...segments[0], app: "Code", appPath }]);
    const html = renderToStaticMarkup(
      <TodayAnalysisPanels
        metrics={metrics}
        activityCompositions={buildFallbackActivityCompositions(segments)}
        activityScope="all"
        onActivityScopeChange={() => {}}
        identities={new Map([[appIdentityKey("Code", appPath), identity]])}
        selectedSeries={["active", "learning"]}
        onSeriesChange={() => {}}
        onDrill={() => {}}
      />,
    );

    expect(html).toContain("Visual Studio Code");
    expect(html).toContain('data-app-icon="native"');
  });

  it("limits the activity and application breakdown lists to five rows", () => {
    const categories = [
      "research",
      "text_input",
      "creation_development",
      "social",
      "game",
      "file_management",
    ] as const;
    const manySegments = categories.map((category, index) => ({
      ...segments[0],
      id: `segment-${index}`,
      startMs: index * 3_600_000,
      endMs: (index + 1) * 3_600_000,
      app: `Ranked App ${index + 1}`,
      category,
    }));
    const html = renderToStaticMarkup(
      <TodayAnalysisPanels
        metrics={buildDashboardMetrics(manySegments)}
        activityCompositions={buildFallbackActivityCompositions(manySegments)}
        activityScope="all"
        onActivityScopeChange={() => {}}
        selectedSeries={["active", "learning"]}
        onSeriesChange={() => {}}
        onDrill={() => {}}
      />,
    );

    expect((html.match(/class="bar-item"/g) ?? []).length).toBe(5);
    expect((html.match(/class="app-row"/g) ?? []).length).toBe(5);
    expect(html).toContain("Ranked App 5");
    expect(html).not.toContain("Ranked App 6");
  });

  it("renders the learning composition without falling back to all activity", () => {
    const withIdle = [
      ...segments,
      { ...segments[0], id: "idle", startMs: 16 * 3_600_000, endMs: 17 * 3_600_000, category: "idle" as const },
    ];
    const html = renderToStaticMarkup(
      <TodayAnalysisPanels
        metrics={buildDashboardMetrics(withIdle)}
        activityCompositions={buildFallbackActivityCompositions(withIdle)}
        activityScope="meaningful"
        onActivityScopeChange={() => {}}
        selectedSeries={["active", "learning"]}
        onSeriesChange={() => {}}
        onDrill={() => {}}
      />,
    );

    expect(html).toContain('aria-pressed="true">学习</button>');
    expect(html).not.toContain("不活跃</span>");
    expect(html).not.toContain("有意义活动");
    expect(html).not.toContain("非娱乐活动");
  });

  it("drills a ranking identity by both raw app name and executable path", () => {
    expect(appRankingFilter({
      name: "Editor",
      appPath: "D:\\Portable\\Editor.exe",
      seconds: 60,
      share: 1,
    })).toEqual({
      mode: "app",
      app: "Editor",
      appPath: "D:\\Portable\\Editor.exe",
    });
  });
});
