import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TrendBaselineSeries, TrendBucket } from "../../lib/desktop";

const init = vi.fn();
vi.mock("echarts", () => ({ init }));

import { TrendComparisonChart } from "./TrendComparisonChart";
import { TrendTimelineChart } from "./TrendTimelineChart";

const values = { monitoredSeconds: 7200, activeSeconds: 5400, learningSeconds: 3000, idleSeconds: 1800, switchCount: 4, longestFocusSeconds: 2400, classificationCoverage: .9, completedTaskCount: 1, linkedTaskSeconds: 2400 };
const bucket = (id: string, date: string): TrendBucket => ({ id, startDate: date, endDate: date, values, recordedDayCount: 1, missingDayCount: 0, evidenceIds: [], drilldown: { bucketId: id, rawRows: [], applicationDistribution: [], categoryDistribution: [], completedTasks: [], linkedTaskRollups: [], linkedProjectRollups: [], workflowOwnership: [], dataQuality: { recordedDayCount: 1, missingDayCount: 0, classifiedSeconds: 6480, classificationCoverage: .9, lowConfidenceSeconds: 0, pendingSeconds: 0 } } });
const baselines: TrendBaselineSeries[] = [
  { kind: "current", range: { startDate: "2026-07-09", endDate: "2026-07-15" }, isValid: true, value: 5400, absoluteDelta: 0, percentDelta: 0, recordedDayCount: 7, missingDayCount: 0, evidenceIds: [] },
  { kind: "previousEqualLength", range: { startDate: "2026-07-02", endDate: "2026-07-08" }, isValid: true, value: 0, absoluteDelta: 5400, percentDelta: null, recordedDayCount: 7, missingDayCount: 0, evidenceIds: [] },
  { kind: "previousMonthSamePeriod", range: { startDate: "2026-06-09", endDate: "2026-06-15" }, isValid: false, value: null, absoluteDelta: null, percentDelta: null, recordedDayCount: 0, missingDayCount: 7, evidenceIds: [] },
  { kind: "custom", range: { startDate: "2026-05-09", endDate: "2026-05-15" }, isValid: true, value: 3600, absoluteDelta: 1800, percentDelta: 50, recordedDayCount: 7, missingDayCount: 0, evidenceIds: [] },
];

describe("trend ECharts integrations", () => {
  const originalResizeObserver = globalThis.ResizeObserver;
  afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); globalThis.ResizeObserver = originalResizeObserver; });

  function mount() {
    const { document, window } = parseHTML("<div id='root'></div>");
    Object.defineProperty(document, "defaultView", { value: window });
    Object.assign(window, { getComputedStyle: () => ({}) });
    vi.stubGlobal("document", document);
    vi.stubGlobal("window", window);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const observers: Array<{ observe: ReturnType<typeof vi.fn>; disconnect: ReturnType<typeof vi.fn>; callback: ResizeObserverCallback }> = [];
    vi.stubGlobal("ResizeObserver", class { observe = vi.fn(); disconnect = vi.fn(); constructor(callback: ResizeObserverCallback) { observers.push({ observe: this.observe, disconnect: this.disconnect, callback }); } });
    const chart = { setOption: vi.fn(), on: vi.fn(), resize: vi.fn(), dispose: vi.fn() };
    init.mockReturnValue(chart);
    return { root: createRoot(document.getElementById("root") as unknown as Element), chart, observers };
  }

  it("renders grouped comparison series, gaps null values, and cleans up ECharts", async () => {
    const { root, chart, observers } = mount();
    await act(async () => { root.render(<TrendComparisonChart baselines={baselines} metric="activeSeconds" formatDuration={(value) => `${value}s`} />); await Promise.resolve(); });
    const option = chart.setOption.mock.calls[0][0];
    expect(option.xAxis.data).toEqual(["活跃时长"]);
    expect(option.legend.data).toEqual(["当前区间", "上一等长区间", "上月同期", "自定义基准"]);
    expect(option.series).toHaveLength(4);
    expect(option.series.map((series: { name: string }) => series.name)).toEqual(option.legend.data);
    expect(option.series[2].data).toEqual(["-"]);
    observers[0].callback([], {} as ResizeObserver);
    expect(chart.resize).toHaveBeenCalledOnce();
    await act(async () => root.unmount());
    expect(observers[0].disconnect).toHaveBeenCalledOnce();
    expect(chart.dispose).toHaveBeenCalledOnce();
  });

  it("updates the chart click mapping when timeline buckets change", async () => {
    const { root, chart, observers } = mount();
    const select = vi.fn();
    await act(async () => { root.render(<TrendTimelineChart buckets={[bucket("old", "2026-07-09")]} metric="activeSeconds" selectedBucketId={null} formatDuration={(value) => `${value}s`} onSelectBucket={select} />); await Promise.resolve(); });
    await act(async () => { root.render(<TrendTimelineChart buckets={[bucket("new", "2026-07-10")]} metric="learningSeconds" selectedBucketId="new" formatDuration={(value) => `${value}s`} onSelectBucket={select} />); await Promise.resolve(); });
    const latestOn = chart.on.mock.calls.at(-1);
    expect(latestOn).toBeDefined();
    const handler = latestOn![1] as (event: { dataIndex: number }) => void;
    handler({ dataIndex: 0 });
    expect(select).toHaveBeenLastCalledWith("new");
    const latestOption = chart.setOption.mock.calls.at(-1);
    expect(latestOption).toBeDefined();
    expect(latestOption![0].series[0].data[0].value).toBeCloseTo(3000 / 3600);
    observers.at(-1)!.callback([], {} as ResizeObserver);
    expect(chart.resize).toHaveBeenCalled();
    await act(async () => root.unmount());
    expect(chart.dispose).toHaveBeenCalled();
  });

  it("plots duration metrics in hours while keeping timeline tooltip durations raw", async () => {
    const { root, chart } = mount();
    await act(async () => { root.render(<TrendTimelineChart buckets={[bucket("duration", "2026-07-09")]} metric="activeSeconds" selectedBucketId={null} formatDuration={(value) => `${value}s`} onSelectBucket={vi.fn()} />); await Promise.resolve(); });
    const option = chart.setOption.mock.calls[0][0];
    expect(option.yAxis.name).toBe("小时");
    expect(option.yAxis.axisLabel.formatter(1.234)).toBe("1.23");
    expect(option.series[0].data[0].value).toBe(1.5);
    expect(option.tooltip.formatter([{ dataIndex: 0, value: 1.5 }])).toContain("5400s");
    await act(async () => root.unmount());
  });

  it("uses metric-specific chart units for comparisons and keeps invalid values as gaps", async () => {
    const { root, chart } = mount();
    await act(async () => { root.render(<TrendComparisonChart baselines={baselines} metric="activeSeconds" formatDuration={(value) => `${value}s`} />); await Promise.resolve(); });
    const durationOption = chart.setOption.mock.calls[0][0];
    expect(durationOption.yAxis.name).toBe("小时");
    expect(durationOption.yAxis.axisLabel.formatter(1.234)).toBe("1.23");
    expect(durationOption.series[0].data).toEqual([1.5]);
    expect(durationOption.series[2].data).toEqual(["-"]);
    const coverageBaselines = baselines.map((baseline, index) => ({ ...baseline, value: index === 2 ? null : [.9, .5, 0, .75][index] }));
    await act(async () => { root.render(<TrendComparisonChart baselines={coverageBaselines} metric="classificationCoverage" formatDuration={(value) => `${value}s`} />); await Promise.resolve(); });
    const coverageOption = chart.setOption.mock.calls.at(-1)![0];
    expect(coverageOption.yAxis.name).toBe("百分比");
    expect(coverageOption.yAxis.axisLabel.formatter(90)).toBe("90%");
    expect(coverageOption.series[0].data).toEqual([90]);
    await act(async () => root.unmount());
  });

  it("labels count axes with their natural units", async () => {
    const { root, chart } = mount();
    await act(async () => { root.render(<TrendTimelineChart buckets={[bucket("count", "2026-07-09")]} metric="switchCount" selectedBucketId={null} formatDuration={(value) => `${value}s`} onSelectBucket={vi.fn()} />); await Promise.resolve(); });
    expect(chart.setOption.mock.calls[0][0].yAxis.name).toBe("次");
    await act(async () => { root.render(<TrendComparisonChart baselines={baselines} metric="completedTaskCount" formatDuration={(value) => `${value}s`} />); await Promise.resolve(); });
    expect(chart.setOption.mock.calls.at(-1)![0].yAxis.name).toBe("个");
    await act(async () => root.unmount());
  });
});
