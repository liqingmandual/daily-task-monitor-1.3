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
  const keyboardIndexRef = useRef(0);
  const onSelectRef = useRef(onSelect);
  const onPreviewRef = useRef(onPreview);
  const [keyboardIndex, setKeyboardIndex] = useState(0);
  onSelectRef.current = onSelect;
  onPreviewRef.current = onPreview;

  const selectionFor = (item: DonutItem, percent: number): DonutSelection => ({ ...item, percent });

  useEffect(() => {
    const nextIndex = items.length ? Math.min(keyboardIndexRef.current, items.length - 1) : 0;
    keyboardIndexRef.current = nextIndex;
    setKeyboardIndex(nextIndex);
  }, [items.length]);

  useEffect(() => {
    if (!chartRef.current) return;
    const chartWindow = chartRef.current.ownerDocument.defaultView;
    if (!chartWindow || typeof chartWindow.getComputedStyle !== "function") return;
    const chart = echarts.init(chartRef.current, undefined, { renderer: "svg" });
    chartInstanceRef.current = chart;
    chart.setOption({
      animationDuration: 280,
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
        data: items.map((item) => ({ id: item.key, name: item.name, value: item.value, itemStyle: { color: item.color } })),
      }],
    });
    const itemFor = (params: unknown) => {
      const event = params as { data?: { id?: string } | null; percent?: number };
      const item = items.find((candidate) => candidate.key === event.data?.id);
      return item ? selectionFor(item, event.percent ?? 0) : null;
    };
    chart.on("click", (params) => {
      const item = itemFor(params);
      if (item) onSelectRef.current(item);
    });
    chart.on("mouseover", (params) => onPreviewRef.current?.(itemFor(params)));
    chart.on("globalout", () => onPreviewRef.current?.(null));
    const resize = () => chart.resize();
    window.addEventListener("resize", resize);
    return () => {
      window.removeEventListener("resize", resize);
      chartInstanceRef.current = null;
      chart.dispose();
    };
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
