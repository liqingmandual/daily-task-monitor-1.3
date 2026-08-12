import { useEffect, useMemo, useState } from "react";
import { Activity } from "lucide-react";
import {
  compositionToDonutItems,
  type ActivityCompositions,
  type ActivityDisplayKey,
  type ActivityScope,
} from "../../lib/activity-composition";
import type { TimelineFilter } from "../../lib/timeline-filter";
import { formatChartDuration } from "../../lib/presentation";
import { DonutChart, type DonutSelection, formatPreview, PanelHeading } from "./analysis-shared";

const BREAKDOWN_LIMIT = 5;
const OTHER_ACTIVITY_KEY = "__other_activity__";

export function DistributionPanel({
  activityCompositions,
  activityScope,
  onActivityScopeChange,
  onDrill,
  previewResetKey,
}: {
  activityCompositions: ActivityCompositions;
  activityScope: ActivityScope;
  onActivityScopeChange: (scope: ActivityScope) => void;
  onDrill: (filter: TimelineFilter) => void;
  previewResetKey?: string;
}) {
  const [preview, setPreview] = useState<DonutSelection | null>(null);
  const composition = activityCompositions[activityScope];
  useEffect(() => setPreview(null), [activityScope, composition, previewResetKey]);
  const rankedItems = useMemo(
    () => compositionToDonutItems(composition),
    [composition],
  );
  const visibleItems = rankedItems.slice(0, BREAKDOWN_LIMIT);
  const donutItems = useMemo(() => {
    const remaining = rankedItems.slice(BREAKDOWN_LIMIT);
    const otherValue = remaining.reduce((total, item) => total + item.value, 0);
    return otherValue > 0
      ? [...visibleItems, { key: OTHER_ACTIVITY_KEY, name: "其他活动", value: otherValue, color: "#a8a29e" }]
      : visibleItems;
  }, [rankedItems, visibleItems]);

  const drill = (key: ActivityDisplayKey) => {
    onDrill({ mode: "displayCategory", key });
  };

  return <article className="panel distribution-panel" data-interaction-model="hover-preview-click-select">
    <div className="composition-panel-heading">
      <PanelHeading eyebrow="COMPOSITION" title="活动构成" icon={<Activity size={18} />} />
      <div className="trend-composition-toggle" role="group" aria-label="今日活动构成口径">
        <button type="button" aria-pressed={activityScope === "all"} onClick={() => onActivityScopeChange("all")}>全部</button>
        <button type="button" aria-pressed={activityScope === "meaningful"} onClick={() => onActivityScopeChange("meaningful")}>学习</button>
      </div>
    </div>
    {donutItems.length ? <>
      <div className="split-visual compact">
        <DonutChart
          ariaLabel={`今日活动构成（${activityScope === "all" ? "完整时间口径" : "学习"}）`}
          centerLabel={activityScope === "all" ? "全部活动" : "学习"}
          centerValue={formatChartDuration(composition.totalSeconds)}
          items={donutItems}
          onPreview={setPreview}
          onSelect={(item) => {
            if (item.key !== OTHER_ACTIVITY_KEY) drill(item.key as ActivityDisplayKey);
          }}
        />
        <div className="category-list">
          {visibleItems.map((donutItem) => {
            return <button
              className="bar-item"
              key={donutItem.key}
              style={{ "--bar-color": donutItem.color } as React.CSSProperties}
              onPointerEnter={() => setPreview({ ...donutItem, percent: donutItem.share * 100 })}
              onPointerLeave={() => setPreview(null)}
              onFocus={() => setPreview({ ...donutItem, percent: donutItem.share * 100 })}
              onBlur={() => setPreview(null)}
              onClick={() => drill(donutItem.key)}
            >
              <div><span><i />{donutItem.name}</span><b>{Math.round(donutItem.share * 100)}%</b></div>
              <div className="bar-track"><span style={{ width: `${Math.max(2, donutItem.share * 100)}%` }} /></div>
              <small>{formatChartDuration(donutItem.value)}</small>
            </button>;
          })}
        </div>
      </div>
      <p className="chart-preview" aria-live="polite">{formatPreview(preview)}</p>
    </> : <div className="composition-empty" role="status">
      {activityScope === "meaningful"
        ? "当前日期没有符合规则的学习活动。"
        : "当前日期暂无活动记录。"}
    </div>}
  </article>;
}
