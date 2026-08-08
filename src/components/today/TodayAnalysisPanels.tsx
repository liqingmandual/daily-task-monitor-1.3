import type { AppIdentity } from "../../lib/app-identity";
import type { DashboardMetrics, TimeSeriesKey } from "../../lib/metrics";
import type { ActivityCompositions, ActivityScope } from "../../lib/activity-composition";
import type { TimelineFilter } from "../../lib/timeline-filter";
import { AppRankingPanel } from "./AppRankingPanel";
import { DistributionPanel } from "./DistributionPanel";
import { TimeDistributionPanel } from "./TimeDistributionPanel";

export function TodayAnalysisPanels({
  metrics,
  activityCompositions,
  activityScope,
  onActivityScopeChange,
  identities,
  selectedSeries,
  onSeriesChange,
  onDrill,
  previewResetKey,
}: {
  metrics: DashboardMetrics;
  activityCompositions: ActivityCompositions;
  activityScope: ActivityScope;
  onActivityScopeChange: (scope: ActivityScope) => void;
  identities?: ReadonlyMap<string, AppIdentity>;
  selectedSeries: TimeSeriesKey[];
  onSeriesChange: (series: TimeSeriesKey[]) => void;
  onDrill: (filter: TimelineFilter) => void;
  previewResetKey?: string;
}) {
  return <section className="analysis-grid" data-analysis-layout="three-column">
    <DistributionPanel
      activityCompositions={activityCompositions}
      activityScope={activityScope}
      onActivityScopeChange={onActivityScopeChange}
      onDrill={onDrill}
      previewResetKey={previewResetKey}
    />
    <AppRankingPanel metrics={metrics} identities={identities} onDrill={onDrill} previewResetKey={previewResetKey} />
    <TimeDistributionPanel buckets={metrics.timeBuckets} selectedSeries={selectedSeries} onSeriesChange={onSeriesChange} onDrill={onDrill} />
  </section>;
}
