import { useEffect, useRef } from "react";

import type {
  ProjectDailyPoint,
  ProjectTaskContribution,
} from "../../lib/desktop";

export const workflowChartPalette = [
  "#2563eb",
  "#0f9f8f",
  "#e58a12",
  "#8b5cf6",
  "#e24976",
  "#64748b",
];

export function ProjectDailyStackedChart({
  points,
  tasks,
  formatDuration,
  onOpenDate,
}: {
  points: ProjectDailyPoint[];
  tasks: ProjectTaskContribution[];
  formatDuration: (seconds: number) => string;
  onOpenDate?: (date: string) => void;
}) {
  const chartRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = chartRef.current;
    if (!element || typeof element.ownerDocument.defaultView?.getComputedStyle !== "function") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void import("echarts").then((echarts) => {
      if (disposed) return;
      const chart = echarts.init(element);
      const visibleTasks = tasks.filter((task) => (
        task.investedSeconds > 0
        || points.some((point) => (point.taskSeconds[task.taskId] ?? 0) > 0)
      ));
      const taskSeries = visibleTasks.map((task, index) => ({
        name: task.taskTitle,
        type: "bar" as const,
        stack: "workflow",
        barMaxWidth: 22,
        emphasis: { focus: "series" as const },
        itemStyle: { color: workflowChartPalette[index % workflowChartPalette.length] },
        data: points.map((point) => {
          const rawTotal = Object.values(point.taskSeconds).reduce((sum, value) => sum + value, 0);
          const exclusiveTotal = Math.max(0, point.investedSeconds - point.sharedSeconds);
          const rawValue = point.taskSeconds[task.taskId] ?? 0;
          return rawTotal > 0 ? rawValue / rawTotal * exclusiveTotal / 3_600 : 0;
        }),
      }));
      const sharedSeries = points.some((point) => point.sharedSeconds > 0)
        ? [{
          name: "共享证据",
          type: "bar" as const,
          stack: "workflow",
          barMaxWidth: 22,
          itemStyle: { color: "#94a3b8" },
          data: points.map((point) => point.sharedSeconds / 3_600),
        }]
        : [];
      chart.setOption({
        animationDuration: 260,
        grid: { left: 44, right: 14, top: 24, bottom: 54 },
        legend: {
          type: "scroll",
          bottom: 2,
          left: "center",
          itemWidth: 8,
          itemHeight: 8,
          textStyle: { color: "#64748b", fontSize: 10 },
        },
        tooltip: {
          trigger: "axis",
          formatter: (items: Array<{ axisValue: string; value: number; marker: string; seriesName: string }>) => {
            const total = items.reduce((sum, item) => sum + Number(item.value || 0), 0) * 3_600;
            const rows = items
              .filter((item) => Number(item.value) > 0)
              .map((item) => `${item.marker}${item.seriesName}：${formatDuration(Number(item.value) * 3_600)}`)
              .join("<br/>");
            return `<b>${items[0]?.axisValue ?? ""}</b><br/>总投入：${formatDuration(total)}${rows ? `<br/>${rows}` : ""}`;
          },
        },
        xAxis: {
          type: "category",
          data: points.map((point) => point.date),
          axisTick: { show: false },
          axisLine: { lineStyle: { color: "#dce5f1" } },
          axisLabel: {
            color: "#64748b",
            fontSize: 9,
            interval: points.length > 14 ? Math.max(0, Math.floor(points.length / 6) - 1) : 0,
            formatter: (value: string) => value.slice(5),
          },
        },
        yAxis: {
          type: "value",
          min: 0,
          axisLabel: { color: "#64748b", fontSize: 9, formatter: (value: number) => `${value}h` },
          splitLine: { lineStyle: { color: "#e7edf5", type: "dashed" } },
        },
        series: [...taskSeries, ...sharedSeries],
      });
      chart.on("click", (event: { name?: string }) => {
        if (event.name) onOpenDate?.(event.name);
      });
      const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => chart.resize());
      resize?.observe(element);
      cleanup = () => {
        resize?.disconnect();
        chart.dispose();
      };
    });
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [formatDuration, onOpenDate, points, tasks]);

  return (
    <div
      ref={chartRef}
      className="workflow-stacked-chart"
      role="img"
      aria-label="工作流每日投入堆叠柱状图"
    />
  );
}
