import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";

import { activityDisplayRegistry, type ActivityCompositions } from "../../lib/activity-composition";
import type { TrendBucket } from "../../lib/desktop";

const init = vi.fn();
vi.mock("echarts", () => ({ init }));

import { buildTrendChartPoints, TrendStackedTimeChart } from "./TrendDashboardCharts";

const values = {
  monitoredSeconds: 7_200,
  activeSeconds: 5_400,
  learningSeconds: 3_000,
  idleSeconds: 1_800,
  switchCount: 4,
  longestFocusSeconds: 2_400,
  classificationCoverage: 0.9,
  completedTaskCount: 1,
  linkedTaskSeconds: 2_400,
};

function bucket(activityComposition?: ActivityCompositions): TrendBucket {
  return {
    id: "bucket-1",
    startDate: "2026-08-01",
    endDate: "2026-08-01",
    values,
    recordedDayCount: 1,
    missingDayCount: 0,
    evidenceIds: [],
    activityComposition,
    drilldown: {
      bucketId: "bucket-1",
      rawRows: [],
      applicationDistribution: [],
      categoryDistribution: [{ key: "idle", label: "legacy idle", seconds: 1_800 }],
      completedTasks: [],
      linkedTaskRollups: [],
      linkedProjectRollups: [],
      workflowOwnership: [],
      dataQuality: { recordedDayCount: 1, missingDayCount: 0, classifiedSeconds: 7_200, classificationCoverage: 1, lowConfidenceSeconds: 0, pendingSeconds: 0 },
    },
  };
}

function compositions(allSeconds: number, meaningfulSeconds: number): ActivityCompositions {
  return {
    all: {
      totalSeconds: allSeconds,
      items: allSeconds ? [{ key: "idle", category: "idle", videoPurpose: null, seconds: allSeconds, share: 1, meaningfulReason: "excluded" }] : [],
    },
    meaningful: {
      totalSeconds: meaningfulSeconds,
      items: meaningfulSeconds ? [{ key: "research", category: "research", videoPurpose: null, seconds: meaningfulSeconds, share: 1, meaningfulReason: "core" }] : [],
    },
  };
}

describe("TrendStackedTimeChart activity scope", () => {
  const originalResizeObserver = globalThis.ResizeObserver;

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    globalThis.ResizeObserver = originalResizeObserver;
  });

  function mount() {
    const { document, window } = parseHTML("<div id='root'></div>");
    Object.defineProperty(document, "defaultView", { value: window });
    Object.assign(window, { getComputedStyle: () => ({}) });
    vi.stubGlobal("document", document);
    vi.stubGlobal("window", window);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const chart = { setOption: vi.fn(), on: vi.fn(), resize: vi.fn(), dispose: vi.fn() };
    init.mockReturnValue(chart);
    return { root: createRoot(document.getElementById("root") as unknown as Element), chart };
  }

  it("keeps an empty meaningful slice at zero instead of falling back to all activity", async () => {
    const { root, chart } = mount();
    await act(async () => {
      root.render(<TrendStackedTimeChart buckets={[bucket(compositions(1_800, 0))]} activityScope="meaningful" selectedBucketId={null} formatDuration={(seconds) => `${seconds}s`} onSelectBucket={vi.fn()} />);
      await Promise.resolve();
    });

    expect(chart.setOption.mock.calls[0][0].series).toEqual([]);
    await act(async () => root.unmount());
  });

  it("uses the composition display registry for scoped labels and colors", async () => {
    const { root, chart } = mount();
    await act(async () => {
      root.render(<TrendStackedTimeChart buckets={[bucket(compositions(1_800, 7_200))]} activityScope="meaningful" selectedBucketId={null} formatDuration={(seconds) => `${seconds}s`} onSelectBucket={vi.fn()} />);
      await Promise.resolve();
    });

    const research = activityDisplayRegistry.find((item) => item.key === "research")!;
    const series = chart.setOption.mock.calls[0][0].series;
    expect(series).toHaveLength(1);
    expect(series[0].name).toBe(research.label);
    expect(series[0].itemStyle.color).toBe(research.color);
    expect(series[0].data[0].value).toBe(2);
    await act(async () => root.unmount());
  });

  it("keeps legacy category distribution for all scope when no composition exists", async () => {
    const { root, chart } = mount();
    await act(async () => {
      root.render(<TrendStackedTimeChart buckets={[bucket()]} activityScope="all" selectedBucketId={null} formatDuration={(seconds) => `${seconds}s`} onSelectBucket={vi.fn()} />);
      await Promise.resolve();
    });

    const series = chart.setOption.mock.calls[0][0].series;
    expect(series).toHaveLength(1);
    expect(series[0].name).toBe(activityDisplayRegistry.find((item) => item.key === "idle")!.label);
    expect(series[0].data[0].value).toBe(0.5);
    await act(async () => root.unmount());
  });

  it("falls back to authoritative active and idle totals when category slices are absent", async () => {
    const { root, chart } = mount();
    const withoutSlices = bucket();
    withoutSlices.drilldown.categoryDistribution = [];
    await act(async () => {
      root.render(<TrendStackedTimeChart buckets={[withoutSlices]} activityScope="all" selectedBucketId={null} formatDuration={(seconds) => `${seconds}s`} onSelectBucket={vi.fn()} />);
      await Promise.resolve();
    });

    const series = chart.setOption.mock.calls[0][0].series;
    expect(series.map((item: { name: string }) => item.name)).toEqual(["活跃", "不活跃"]);
    expect(series.map((item: { data: Array<{ value: number }> }) => item.data[0].value)).toEqual([1.5, 0.5]);
    await act(async () => root.unmount());
  });
});

describe("buildTrendChartPoints", () => {
  it("pads a short daily history at the beginning to seven calendar days", () => {
    const first = { ...bucket(), id: "day-12", startDate: "2026-08-12", endDate: "2026-08-12", values: { ...values, activeSeconds: 3_600 } };
    const second = { ...bucket(), id: "day-13", startDate: "2026-08-13", endDate: "2026-08-13", values: { ...values, activeSeconds: 7_200 } };
    const points = buildTrendChartPoints([first, second], "2026-08-12", "2026-08-13", "day");

    expect(points).toHaveLength(7);
    expect(points.map((point) => point.date)).toEqual([
      "2026-08-07", "2026-08-08", "2026-08-09", "2026-08-10",
      "2026-08-11", "2026-08-12", "2026-08-13",
    ]);
    expect(points.map((point) => point.activeSeconds)).toEqual([0, 0, 0, 0, 0, 3_600, 7_200]);
  });

  it("preserves non-daily buckets without inserting artificial ranges", () => {
    const weekly = { ...bucket(), startDate: "2026-08-10", endDate: "2026-08-13", values: { ...values, activeSeconds: 3_600 } };
    const points = buildTrendChartPoints([weekly], "2026-08-10", "2026-08-13", "week");
    expect(points).toHaveLength(1);
    expect(points[0]).toMatchObject({ label: "2026-08-10~2026-08-13", activeSeconds: 3_600 });
  });
});
