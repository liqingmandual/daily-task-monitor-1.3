import { useState, type FocusEvent, type PointerEvent } from "react";
import { createPortal } from "react-dom";
import { Clock3 } from "lucide-react";
import type { TimeBucketMetric, TimeSeriesKey } from "../../lib/metrics";
import { TWO_HOUR_BUCKET_MAX_SECONDS, getTimeBucketSeriesSeconds } from "../../lib/metrics";
import { activityDisplayRegistry } from "../../lib/activity-composition";
import type { TimelineFilter } from "../../lib/timeline-filter";
import { PanelHeading } from "./analysis-shared";
import {
  DEFAULT_TIME_SERIES,
  getTimeBarTooltip,
  getTimeChartMinimumWidth,
  timeBucketFilter,
  timeSeriesLabel,
  timeSeriesColor,
  toggleTimeSeries,
} from "./time-distribution-controller";

const atomicSeries = activityDisplayRegistry.map((item) => item.key);

function SeriesPicker({ value, onChange }: { value: TimeSeriesKey[]; onChange: (series: TimeSeriesKey[]) => void }) {
  const seriesOptions: TimeSeriesKey[] = ["active", "learning", ...atomicSeries];
  const toggle = (series: TimeSeriesKey) => {
    onChange(toggleTimeSeries(value, series));
  };
  return <div className="series-picker" role="group" aria-label="时间分布序列">
    {seriesOptions.map((series) => <button key={series} type="button" className={value.includes(series) ? "selected" : ""} aria-pressed={value.includes(series)} onClick={() => toggle(series)}>{timeSeriesLabel(series)}</button>)}
  </div>;
}

export function TimeDistributionPanel({ buckets, selectedSeries, onSeriesChange, onDrill }: {
  buckets: TimeBucketMetric[];
  selectedSeries: TimeSeriesKey[];
  onSeriesChange: (series: TimeSeriesKey[]) => void;
  onDrill: (filter: TimelineFilter) => void;
}) {
  const [hovered, setHovered] = useState<ReturnType<typeof getTimeBarTooltip> & { x: number; y: number } | null>(null);
  const minimumWidth = getTimeChartMinimumWidth(buckets.length, selectedSeries.length);
  const showTooltip = (event: PointerEvent<HTMLButtonElement> | FocusEvent<HTMLButtonElement>, bucket: TimeBucketMetric, series: TimeSeriesKey) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const pointer = "clientX" in event && event.clientX > 0
      ? { x: event.clientX, y: event.clientY }
      : { x: rect.left + rect.width / 2, y: rect.top };
    setHovered({ ...getTimeBarTooltip(bucket, series), ...pointer });
  };
  const tooltipStyle = hovered && typeof window !== "undefined"
    ? {
        left: Math.max(12, Math.min(window.innerWidth - 274, hovered.x + 14)),
        top: hovered.y > window.innerHeight - 210 ? hovered.y - 194 : hovered.y + 14,
      }
    : undefined;

  return <>
    <article className="panel time-panel" data-interaction-model="hover-preview-click-select">
      <PanelHeading eyebrow="HOURLY" title="时间分布" icon={<Clock3 size={18} />} />
      <SeriesPicker value={selectedSeries} onChange={onSeriesChange} />
      <div className="two-hour-chart-scroll">
        <div className="two-hour-chart" style={{ "--series-count": Math.max(selectedSeries.length, 1), minWidth: `${minimumWidth}px` } as React.CSSProperties} aria-label="每两小时的时间分布">
          <div className="time-axis" aria-hidden="true"><span>120 min</span><span>60 min</span><span>0</span></div>
          {buckets.map((bucket) => <div key={bucket.key} data-time-bucket={bucket.key} className="time-bucket-group">
            <div className="time-bucket-bars">
              {selectedSeries.map((series) => {
                const seconds = getTimeBucketSeriesSeconds(bucket, series);
                return <button
                  key={series}
                  type="button"
                  className={`time-series-bar series-${series}`}
                  style={{
                    height: `${seconds / TWO_HOUR_BUCKET_MAX_SECONDS * 100}%`,
                    background: timeSeriesColor(series),
                  }}
                  aria-controls="activity-timeline"
                  aria-describedby="time-bar-tooltip"
                  aria-label={`${bucket.range} ${timeSeriesLabel(series)} ${getTimeBarTooltip(bucket, series).duration}`}
                  onPointerEnter={(event) => showTooltip(event, bucket, series)}
                  onPointerMove={(event) => showTooltip(event, bucket, series)}
                  onPointerLeave={() => setHovered(null)}
                  onFocus={(event) => showTooltip(event, bucket, series)}
                  onBlur={() => setHovered(null)}
                  onClick={() => onDrill(timeBucketFilter(bucket, series))}
                />;
              })}
            </div>
            <span>{bucket.range}</span>
          </div>)}
        </div>
      </div>
    </article>
    {hovered && tooltipStyle && typeof document !== "undefined" && createPortal(
      <aside id="time-bar-tooltip" className="time-hover-card" role="tooltip" style={tooltipStyle}>
        <span>{hovered.range} · {hovered.seriesLabel}</span>
        <strong>{hovered.duration}</strong>
        <dl>
          <div><dt>占两小时</dt><dd>{hovered.percentOfWindow.toFixed(1)}%</dd></div>
          <div><dt>{hovered.denominatorLabel}</dt><dd>{hovered.percentOfDenominator.toFixed(1)}%</dd></div>
        </dl>
      </aside>,
      document.body,
    )}
  </>;
}
