import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import * as echarts from "echarts/core";
import { PieChart } from "echarts/charts";
import { TooltipComponent } from "echarts/components";
import { SVGRenderer } from "echarts/renderers";
import { AppIcon } from "../AppIcon";
import { fallbackAppIdentity } from "../../lib/app-identity";
import { formatChartDuration, formatDonutTooltip } from "../../lib/presentation";
import { categoryMeta } from "./analysis-meta";

export { categoryMeta } from "./analysis-meta";

echarts.use([PieChart, TooltipComponent, SVGRenderer]);

export type DonutItem = { key: string; name: string; value: number; color: string };
export type DonutSelection = DonutItem & { percent: number };

export const DONUT_SHARE_REFRESH_THRESHOLD = 0.005;
export const DONUT_FORCE_REFRESH_INTERVAL_MS = 5 * 60_000;

function donutShares(items: DonutItem[]): Map<string, number> {
  const total = items.reduce((sum, item) => sum + Math.max(0, item.value), 0);
  return new Map(items.map((item) => [item.key, total ? Math.max(0, item.value) / total : 0]));
}

export function shouldRefreshDonut(
  previous: DonutItem[],
  next: DonutItem[],
  elapsedMs: number,
): boolean {
  if (elapsedMs >= DONUT_FORCE_REFRESH_INTERVAL_MS) return true;
  if (previous.length !== next.length) return true;
  const previousByKey = new Map(previous.map((item) => [item.key, item]));
  if (next.some((item) => {
    const prior = previousByKey.get(item.key);
    return !prior || prior.name !== item.name || prior.color !== item.color;
  })) return true;
  const previousShares = donutShares(previous);
  const nextShares = donutShares(next);
  const totalVariation = next.reduce((difference, item) => (
    difference
      + Math.abs((nextShares.get(item.key) ?? 0) - (previousShares.get(item.key) ?? 0))
  ), 0) / 2;
  return totalVariation >= DONUT_SHARE_REFRESH_THRESHOLD;
}

function donutSeriesData(items: DonutItem[]) {
  return items.map((item) => ({
    id: item.key,
    name: item.name,
    value: item.value,
    itemStyle: { color: item.color },
  }));
}

export function PanelHeading({ eyebrow, title, icon }: { eyebrow: string; title: string; icon: ReactNode }) {
  return <div className="panel-heading"><div><span>{eyebrow}</span><h2>{title}</h2></div><i>{icon}</i></div>;
}

export function DonutChart({
  items,
  ariaLabel,
  centerLabel,
  centerValue,
  onSelect,
  onPreview,
}: {
  items: DonutItem[];
  ariaLabel: string;
  centerLabel: string;
  centerValue: string;
  onSelect: (item: DonutSelection) => void;
  onPreview?: (item: DonutSelection | null) => void;
}) {
  const chartRef = useRef<HTMLDivElement>(null);
  const chartInstanceRef = useRef<echarts.ECharts | null>(null);
  const itemsRef = useRef(items);
  const renderedItemsRef = useRef<DonutItem[]>([]);
  const lastChartRefreshAtRef = useRef(0);
  const keyboardIndexRef = useRef(0);
  const onSelectRef = useRef(onSelect);
  const onPreviewRef = useRef(onPreview);
  const [keyboardIndex, setKeyboardIndex] = useState(0);
  onSelectRef.current = onSelect;
  onPreviewRef.current = onPreview;
  itemsRef.current = items;

  const selectionFor = (item: DonutItem, percent: number): DonutSelection => ({ ...item, percent });

  useEffect(() => {
    const nextIndex = items.length ? Math.min(keyboardIndexRef.current, items.length - 1) : 0;
    keyboardIndexRef.current = nextIndex;
    setKeyboardIndex(nextIndex);
  }, [items.length]);

  useEffect(() => {
    if (!chartRef.current) return;
    const element = chartRef.current;
    const chartWindow = element.ownerDocument.defaultView;
    if (!chartWindow || typeof chartWindow.getComputedStyle !== "function") return;
    let cancelled = false;
    let chart: echarts.ECharts | null = null;
    let forceRefreshTimer: number | undefined;
    const resize = () => chart?.resize();
    queueMicrotask(() => {
      if (cancelled) return;
      chart = echarts.init(element, undefined, { renderer: "svg" });
      chartInstanceRef.current = chart;
      const initialItems = itemsRef.current;
      renderedItemsRef.current = initialItems;
      lastChartRefreshAtRef.current = Date.now();
      chart.setOption({
        animationDuration: 280,
        animationDurationUpdate: 280,
        tooltip: {
          trigger: "item",
          appendTo: "body",
          className: "chart-hover-card",
          confine: true,
          backgroundColor: "rgba(19, 30, 49, .96)",
          borderColor: "rgba(129, 164, 222, .42)",
          textStyle: { color: "#eaf4ff", fontSize: 12 },
          extraCssText: "pointer-events:none;border-radius:8px;box-shadow:0 18px 42px rgba(15,23,42,.32);backdrop-filter:blur(12px);",
          formatter: (params: { name: string; value: number; percent: number }) =>
            formatDonutTooltip(params.name, params.value, params.percent).replace("\n", "<br/>") ,
        },
        series: [{
          type: "pie",
          radius: ["62%", "84%"],
          center: ["50%", "50%"],
          padAngle: 2,
          itemStyle: { borderRadius: 4, borderColor: "#fff", borderWidth: 2 },
          label: { show: false },
          data: donutSeriesData(initialItems),
        }],
      });
      const itemFor = (params: unknown) => {
        const event = params as { data?: { id?: string } | null; percent?: number };
        const item = renderedItemsRef.current.find((candidate) => candidate.key === event.data?.id);
        return item ? selectionFor(item, event.percent ?? 0) : null;
      };
      chart.on("click", (params) => {
        const item = itemFor(params);
        if (item) onSelectRef.current(item);
      });
      chart.on("mouseover", (params) => onPreviewRef.current?.(itemFor(params)));
      chart.on("globalout", () => onPreviewRef.current?.(null));
      chartWindow.addEventListener("resize", resize);
      forceRefreshTimer = chartWindow.setInterval(() => {
        const latestItems = itemsRef.current;
        chart?.setOption({ series: [{ data: donutSeriesData(latestItems) }] });
        renderedItemsRef.current = latestItems;
        lastChartRefreshAtRef.current = Date.now();
      }, DONUT_FORCE_REFRESH_INTERVAL_MS);
    });
    return () => {
      cancelled = true;
      chartWindow.removeEventListener("resize", resize);
      if (forceRefreshTimer !== undefined) chartWindow.clearInterval(forceRefreshTimer);
      chartInstanceRef.current = null;
      chart?.dispose();
    };
  }, []);

  useEffect(() => {
    const chart = chartInstanceRef.current;
    if (!chart || !shouldRefreshDonut(
      renderedItemsRef.current,
      items,
      Date.now() - lastChartRefreshAtRef.current,
    )) return;
    chart.setOption({ series: [{ data: donutSeriesData(items) }] });
    renderedItemsRef.current = items;
    lastChartRefreshAtRef.current = Date.now();
  }, [items]);

  const selectByKeyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!items.length || !["ArrowLeft", "ArrowRight", "Enter", " "].includes(event.key)) return;
    event.preventDefault();
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      const direction = event.key === "ArrowRight" ? 1 : -1;
      keyboardIndexRef.current = (keyboardIndexRef.current + direction + items.length) % items.length;
      setKeyboardIndex(keyboardIndexRef.current);
      chartInstanceRef.current?.dispatchAction({ type: "downplay", seriesIndex: 0 });
      chartInstanceRef.current?.dispatchAction({ type: "highlight", seriesIndex: 0, dataIndex: keyboardIndexRef.current });
      const item = items[keyboardIndexRef.current];
      const total = items.reduce((sum, candidate) => sum + candidate.value, 0);
      onPreviewRef.current?.(selectionFor(item, total ? item.value / total * 100 : 0));
      return;
    }
    const item = items[keyboardIndexRef.current];
    const total = items.reduce((sum, candidate) => sum + candidate.value, 0);
    onSelectRef.current(selectionFor(item, total ? item.value / total * 100 : 0));
  };

  return <div className="donut-interactive" role="group" tabIndex={0} aria-label={`${ariaLabel}；当前选择 ${items[keyboardIndex]?.name ?? "无数据"}；使用左右方向键选择，按回车查看详情`} onKeyDown={selectByKeyboard}>
    <div ref={chartRef} className="donut-chart" aria-hidden="true" />
    <div className="donut-center"><span>{centerLabel}</span><strong>{centerValue}</strong></div>
    <span className="sr-only" aria-live="polite">当前扇区：{items[keyboardIndex]?.name ?? "无数据"}</span>
  </div>;
}

export function AppLogo({ name }: { name: string }) {
  return <AppIcon identity={fallbackAppIdentity(name)} />;
}

export function formatPreview(item: DonutSelection | null) {
  return item ? `${item.name} · ${formatChartDuration(item.value)} · ${item.percent.toFixed(1)}%` : "";
}
