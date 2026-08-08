import type { CSSProperties } from "react";
import { AppIcon } from "../AppIcon";
import { fallbackAppIdentity } from "../../lib/app-identity";
import type { TrendBreakdownItem } from "../../lib/desktop";
import { categoryMeta } from "../today/analysis-shared";

interface TrendBreakdownPanelProps {
  kind: "category" | "app";
  title: string;
  eyebrow: string;
  items: TrendBreakdownItem[];
  previousItems: TrendBreakdownItem[];
  formatDuration: (seconds: number) => string;
}

function signedDelta(current: number, previous: number): string {
  if (previous === 0) return current === 0 ? "持平" : "上期无可比数据";
  const value = (current - previous) / previous * 100;
  return `${value > 0 ? "+" : ""}${value.toFixed(1)}%`;
}

function categoryPresentation(name: string) {
  const metadata = categoryMeta[name as keyof typeof categoryMeta];
  return metadata ?? { label: name, color: "#64748b" };
}

export interface MergedTrendBreakdownItem extends TrendBreakdownItem {
  previousSeconds: number;
}

export function mergeTrendBreakdowns(
  items: TrendBreakdownItem[],
  previousItems: TrendBreakdownItem[],
): MergedTrendBreakdownItem[] {
  const previousByName = new Map(previousItems.map((item) => [item.name, item]));
  const currentNames = new Set(items.map((item) => item.name));
  return [
    ...items.map((item) => ({
      ...item,
      previousSeconds: previousByName.get(item.name)?.seconds ?? 0,
    })),
    ...previousItems
      .filter((item) => !currentNames.has(item.name))
      .map((item) => ({ name: item.name, seconds: 0, share: 0, previousSeconds: item.seconds })),
  ];
}

export function TrendBreakdownPanel({
  kind,
  title,
  eyebrow,
  items,
  previousItems,
  formatDuration,
}: TrendBreakdownPanelProps) {
  const mergedItems = mergeTrendBreakdowns(items, previousItems);
  return (
    <section className="panel trend-breakdown-panel" aria-labelledby={`trend-${kind}-heading`}>
      <header className="trend-section-heading">
        <div><span>{eyebrow}</span><h2 id={`trend-${kind}-heading`}>{title}</h2></div>
      </header>
      <div className="trend-breakdown-list" role="list">
        {mergedItems.length ? mergedItems.map((item) => {
          const metadata = categoryPresentation(item.name);
          const identity = fallbackAppIdentity(item.name);
          return (
            <div
              className="trend-breakdown-row"
              key={item.name}
              role="listitem"
              style={{ "--breakdown-color": kind === "category" ? metadata.color : "var(--primary)" } as CSSProperties}
              aria-label={`${kind === "category" ? metadata.label : item.name}，${formatDuration(item.seconds)}，当前占比 ${Math.round(item.share * 100)}%`}
              title={`${formatDuration(item.seconds)} · 当前占比 ${Math.round(item.share * 100)}% · 较上期 ${signedDelta(item.seconds, item.previousSeconds)}`}
            >
              <span className="trend-breakdown-name">
                {kind === "category" ? <i /> : <AppIcon identity={identity} />}
                <b>{kind === "category" ? metadata.label : identity.displayName}</b>
              </span>
              <span className="trend-breakdown-values">
                <strong>{formatDuration(item.seconds)}</strong>
                <small>当前占比 {Math.round(item.share * 100)}% · 较上期 {signedDelta(item.seconds, item.previousSeconds)}</small>
              </span>
              <span className="trend-breakdown-track"><i style={{ width: item.share > 0 ? `${Math.max(2, item.share * 100)}%` : "0%" }} /></span>
            </div>
          );
        }) : <p className="trend-empty-copy">暂无可比较项目</p>}
      </div>
    </section>
  );
}
