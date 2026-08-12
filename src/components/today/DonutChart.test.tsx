import { StrictMode } from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";

const { init } = vi.hoisted(() => ({ init: vi.fn() }));
vi.mock("echarts/core", () => ({ init, use: vi.fn() }));

import {
  DONUT_FORCE_REFRESH_INTERVAL_MS,
  DonutChart,
  shouldRefreshDonut,
  type DonutItem,
} from "./analysis-shared";

const initialItems: DonutItem[] = [
  { key: "work", name: "工作", value: 50, color: "#2563eb" },
  { key: "idle", name: "不活跃", value: 50, color: "#64748b" },
];

describe("DonutChart refresh policy", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("ignores tiny share changes and refreshes material or five-minute changes", () => {
    expect(shouldRefreshDonut(initialItems, [
      { ...initialItems[0], value: 50.1 },
      { ...initialItems[1], value: 49.9 },
    ], 60_000)).toBe(false);
    expect(shouldRefreshDonut(initialItems, [
      { ...initialItems[0], value: 51 },
      { ...initialItems[1], value: 49 },
    ], 60_000)).toBe(true);
    expect(shouldRefreshDonut(initialItems, initialItems, DONUT_FORCE_REFRESH_INTERVAL_MS)).toBe(true);
  });

  it("initializes once in Strict Mode and throttles insignificant updates", async () => {
    vi.useFakeTimers();
    const { document, window } = parseHTML("<div id='root'></div>");
    Object.defineProperty(document, "defaultView", { value: window });
    Object.assign(window, {
      getComputedStyle: () => ({}),
      setInterval,
      clearInterval,
    });
    vi.stubGlobal("document", document);
    vi.stubGlobal("window", window);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const chart = {
      setOption: vi.fn(),
      on: vi.fn(),
      resize: vi.fn(),
      dispose: vi.fn(),
      dispatchAction: vi.fn(),
    };
    init.mockReturnValue(chart);
    const root = createRoot(document.getElementById("root") as unknown as Element);
    const render = (items: DonutItem[]) => root.render(
      <StrictMode>
        <DonutChart
          items={items}
          ariaLabel="test donut"
          centerLabel="total"
          centerValue="100"
          onSelect={vi.fn()}
        />
      </StrictMode>,
    );

    await act(async () => {
      render(initialItems);
      await vi.runAllTicks();
    });
    expect(init).toHaveBeenCalledOnce();
    expect(chart.setOption).toHaveBeenCalledOnce();

    await act(async () => render([
      { ...initialItems[0], value: 50.1 },
      { ...initialItems[1], value: 49.9 },
    ]));
    expect(chart.setOption).toHaveBeenCalledOnce();

    await act(async () => render([
      { ...initialItems[0], value: 51 },
      { ...initialItems[1], value: 49 },
    ]));
    expect(chart.setOption).toHaveBeenCalledTimes(2);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(DONUT_FORCE_REFRESH_INTERVAL_MS);
    });
    expect(chart.setOption).toHaveBeenCalledTimes(3);

    await act(async () => root.unmount());
    expect(chart.dispose).toHaveBeenCalledOnce();
  });
});
