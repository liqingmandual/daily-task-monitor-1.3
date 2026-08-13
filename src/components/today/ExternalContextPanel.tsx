import { CalendarDays, FolderKanban, ListChecks } from "lucide-react";
import type { ExternalContextItem } from "../../lib/desktop";

function timeLabel(item: ExternalContextItem): string {
  if (item.startAtMs === null) return "";
  const start = new Date(item.startAtMs).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
  if (item.endAtMs === null) return start;
  const end = new Date(item.endAtMs).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
  return `${start}–${end}`;
}

export function ExternalContextPanel({ items }: { items: ExternalContextItem[] }) {
  if (!items?.length) return null;
  return <section className="panel external-context-panel" aria-label="本地日历与项目上下文">
    <header className="external-context-head">
      <div><span>CONTEXT</span><h2>今日上下文</h2></div>
      <small>仅本地 · 不作为活动事实</small>
    </header>
    <div className="external-context-list">
      {items.map((item) => <article key={item.id} data-kind={item.kind}>
        <span className="external-context-icon" aria-hidden="true">
          {item.kind === "calendar_event" ? <CalendarDays size={15} /> : item.kind === "project" ? <FolderKanban size={15} /> : <ListChecks size={15} />}
        </span>
        <div><b>{item.title}</b><small>{[timeLabel(item), item.projectName, item.status, item.sourceName].filter(Boolean).join(" · ")}</small></div>
      </article>)}
    </div>
  </section>;
}
