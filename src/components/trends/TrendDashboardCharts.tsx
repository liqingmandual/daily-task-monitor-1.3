import { useEffect, useMemo, useRef } from "react";

import {
  activityDisplayRegistry,
  displayMetaForKey,
  type ActivityScope,
} from "../../lib/activity-composition";
import type { TrendBucket, TrendDay } from "../../lib/desktop";

const displayMetaByKey = new Map<string, (typeof activityDisplayRegistry)[number]>(
  activityDisplayRegistry.map((item) => [item.key, item]),
);

type ActivityDisplaySlice = {
  key: string;
  label: string;
  color: string;
  seconds: number;
};

function legacyDisplayMeta(key: string) {
  if (key === "unknown_video") return displayMetaForKey("unknown_video");
  return displayMetaByKey.get(key)
    ?? activityDisplayRegistry.find((item) => item.category === key
      && (item.category !== "video_input" || item.key === "unknown_video"));
}

function scopedCategoryDistribution(bucket: TrendBucket, activityScope: ActivityScope): ActivityDisplaySlice[] {
  const composition = bucket.activityComposition?.[activityScope];
  if (composition) {
    return composition.items.map((item) => {
      const meta = displayMetaByKey.get(item.key);
      return {
        key: item.key,
        label: meta?.label ?? item.key,
        color: meta?.color ?? "#64748b",
        seconds: item.seconds,
      };
    });
  }
  if (activityScope === "meaningful") return [];
  return bucket.drilldown.categoryDistribution.map((item) => {
    const meta = legacyDisplayMeta(item.key);
    return {
      key: meta?.key ?? item.key,
      label: meta?.label ?? item.label,
      color: meta?.color ?? "#64748b",
      seconds: item.seconds,
    };
  });
}

export function TrendStackedTimeChart({
  buckets,
  activityScope = "all",
  selectedBucketId,
  formatDuration,
  onSelectBucket,
}: {
  buckets: TrendBucket[];
  activityScope?: ActivityScope;
  selectedBucketId: string | null;
  formatDuration: (seconds: number) => string;
  onSelectBucket: (bucketId: string) => void;
}) {
  const chartRef = useRef<HTMLDivElement>(null);
  const categories = useMemo(() => {
    const totals = new Map<string, ActivityDisplaySlice>();
    for (const bucket of buckets) {
      for (const item of scopedCategoryDistribution(bucket, activityScope)) {
        const current = totals.get(item.key);
        totals.set(item.key, current
          ? { ...current, seconds: current.seconds + item.seconds }
          : { ...item });
      }
    }
    return [...totals]
      .map(([, item]) => item)
      .filter((item) => item.seconds > 0)
      .sort((left, right) => right.seconds - left.seconds)
      .slice(0, 7)
  }, [activityScope, buckets]);

  useEffect(() => {
    const element = chartRef.current;
    if (!element || typeof element.ownerDocument.defaultView?.getComputedStyle !== "function") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void import("echarts").then((echarts) => {
      if (disposed) return;
      const chart = echarts.init(element);
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
            return `<b>${items[0]?.axisValue ?? ""}</b><br/>总计：${formatDuration(total)}${rows ? `<br/>${rows}` : ""}`;
          },
        },
        xAxis: {
          type: "category",
          data: buckets.map((bucket) => bucket.startDate === bucket.endDate
            ? bucket.startDate
            : `${bucket.startDate}~${bucket.endDate}`),
          axisTick: { show: false },
          axisLine: { lineStyle: { color: "#dce5f1" } },
          axisLabel: {
            color: "#64748b",
            fontSize: 9,
            interval: buckets.length > 14 ? Math.max(0, Math.floor(buckets.length / 6) - 1) : 0,
            formatter: (value: string) => value.slice(5),
          },
        },
        yAxis: {
          type: "value",
          min: 0,
          axisLabel: { color: "#64748b", fontSize: 9, formatter: (value: number) => `${value}h` },
          splitLine: { lineStyle: { color: "#e7edf5", type: "dashed" } },
        },
        series: categories.map((category) => ({
          name: category.label,
          type: "bar",
          stack: "activity",
          barMaxWidth: 22,
          emphasis: { focus: "series" },
          itemStyle: { color: category.color },
          data: buckets.map((bucket) => ({
            value: (scopedCategoryDistribution(bucket, activityScope).find((item) => item.key === category.key)?.seconds ?? 0) / 3_600,
            itemStyle: bucket.id === selectedBucketId
              ? { borderColor: "#153f9f", borderWidth: 1 }
              : undefined,
          })),
        })),
      });
      chart.on("click", (event: { dataIndex?: number }) => {
        if (typeof event.dataIndex === "number") onSelectBucket(buckets[event.dataIndex].id);
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
  }, [activityScope, buckets, categories, formatDuration, onSelectBucket, selectedBucketId]);

  return <div ref={chartRef} className="trend-dashboard-echart" role="img" aria-label="活动构成每日堆叠柱状图" />;
}

export function TrendWeekdayChart({
  days,
  formatDuration,
}: {
  days: TrendDay[];
  formatDuration: (seconds: number) => string;
}) {
  const chartRef = useRef<HTMLDivElement>(null);
  const weekdayTotals = useMemo(() => {
    const totals = Array.from({ length: 7 }, () => 0);
    for (const day of days) {
      const weekday = new Date(`${day.date}T00:00:00Z`).getUTCDay();
      const mondayIndex = (weekday + 6) % 7;
      totals[mondayIndex] += day.activeSeconds;
    }
    return totals;
  }, [days]);

  useEffect(() => {
    const element = chartRef.current;
    if (!element || typeof element.ownerDocument.defaultView?.getComputedStyle !== "function") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void import("echarts").then((echarts) => {
      if (disposed) return;
      const chart = echarts.init(element);
      chart.setOption({
        animationDuration: 240,
        grid: { left: 42, right: 12, top: 20, bottom: 28 },
        tooltip: {
          trigger: "axis",
          formatter: (items: Array<{ axisValue: string; value: number }>) => (
            `<b>${items[0]?.axisValue ?? ""}</b><br/>活跃时间：${formatDuration(Number(items[0]?.value ?? 0) * 3_600)}`
          ),
        },
        xAxis: {
          type: "category",
          data: ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
          axisTick: { show: false },
          axisLine: { lineStyle: { color: "#dce5f1" } },
          axisLabel: { color: "#64748b", fontSize: 10 },
        },
        yAxis: {
          type: "value",
          min: 0,
          axisLabel: { color: "#64748b", fontSize: 9, formatter: (value: number) => `${value}h` },
          splitLine: { lineStyle: { color: "#e7edf5", type: "dashed" } },
        },
        series: [{
          name: "活跃时间",
          type: "bar",
          barMaxWidth: 28,
          itemStyle: { color: "#2563eb", borderRadius: [3, 3, 0, 0] },
          data: weekdayTotals.map((seconds) => seconds / 3_600),
        }],
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
  }, [formatDuration, weekdayTotals]);

  return <div ref={chartRef} className="trend-weekday-echart" role="img" aria-label="星期活跃时间柱状图" />;
}
