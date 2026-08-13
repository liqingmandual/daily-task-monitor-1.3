import { useEffect, useMemo, useState } from "react";
import { ListFilter } from "lucide-react";
import { AppIcon } from "../AppIcon";
import { appIdentityKey, identityFor, type AppIdentity } from "../../lib/app-identity";
import type { AppShareItem, DashboardMetrics } from "../../lib/metrics";
import type { TimelineFilter } from "../../lib/timeline-filter";
import { formatChartDuration } from "../../lib/presentation";
import { DonutChart, type DonutSelection, formatPreview, PanelHeading } from "./analysis-shared";

const appColors = ["#2563eb", "#0891b2", "#7c3aed", "#d97706", "#059669", "#64748b", "#ec4899"];
const APP_RANKING_LIMIT = 5;
const OTHER_APPS_KEY = "__other_apps__";

export function appRankingFilter(item: AppShareItem): TimelineFilter {
  return item.appPath
    ? { mode: "app", app: item.name, appPath: item.appPath }
    : { mode: "app", app: item.name };
}

export function AppRankingPanel({ metrics, identities = new Map(), onDrill, previewResetKey }: { metrics: DashboardMetrics; identities?: ReadonlyMap<string, AppIdentity>; onDrill: (filter: TimelineFilter) => void; previewResetKey?: string }) {
  const [preview, setPreview] = useState<DonutSelection | null>(null);
  useEffect(() => setPreview(null), [metrics, previewResetKey]);
  const topApps = useMemo(() => metrics.apps.slice(0, APP_RANKING_LIMIT), [metrics.apps]);
  const donutItems = useMemo(() => {
    const visible = topApps.map((item, index) => ({
      key: appIdentityKey(item.name, item.appPath),
      name: identityFor(identities, item.name, item.appPath).displayName,
      value: item.seconds,
      color: appColors[index % appColors.length],
    }));
    const otherValue = metrics.apps
      .slice(APP_RANKING_LIMIT)
      .reduce((total, item) => total + item.seconds, 0);
    return otherValue > 0
      ? [...visible, { key: OTHER_APPS_KEY, name: "其他应用", value: otherValue, color: "#a8a29e" }]
      : visible;
  }, [identities, metrics.apps, topApps]);

  return <article className="panel apps-panel" data-interaction-model="hover-preview-click-select">
    <PanelHeading eyebrow="TOP 5 APPS" title="应用排行" icon={<ListFilter size={18} />} />
    <div className="split-visual apps-split compact">
      <DonutChart
        ariaLabel="应用占活跃时间比例"
        centerLabel="总活跃"
        centerValue={formatChartDuration(metrics.activeSeconds)}
        items={donutItems}
        onPreview={setPreview}
        onSelect={(item) => {
          const selected = metrics.apps.find((app) => appIdentityKey(app.name, app.appPath) === item.key);
          if (selected) onDrill(appRankingFilter(selected));
        }}
      />
      <div className="app-list">
        {topApps.map((item, index) => {
          const key = appIdentityKey(item.name, item.appPath);
          const identity = identityFor(identities, item.name, item.appPath);
          const donutItem = donutItems.find((candidate) => candidate.key === key)!;
          return <button className="app-row" key={key} onPointerEnter={() => setPreview({ ...donutItem, percent: item.share * 100 })} onPointerLeave={() => setPreview(null)} onFocus={() => setPreview({ ...donutItem, percent: item.share * 100 })} onBlur={() => setPreview(null)} onClick={() => onDrill(appRankingFilter(item))}>
            <span><b>{index + 1}</b>{identity.displayName}<AppIcon identity={identity} /></span>
            <strong>{Math.round(item.share * 100)}%</strong>
            <small>{formatChartDuration(item.seconds)}</small>
            <span className="app-share-track"><i style={{ width: `${Math.max(2, item.share * 100)}%` }} /></span>
          </button>;
        })}
      </div>
    </div>
    <p className="chart-preview" aria-live="polite">{formatPreview(preview)}</p>
  </article>;
}
