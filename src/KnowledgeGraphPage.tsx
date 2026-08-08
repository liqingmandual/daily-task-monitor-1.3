import { useEffect, useMemo, useState } from "react";
import { Activity, AppWindow, ArrowLeft, CalendarDays, CircleDot, Eye, EyeOff, Globe2, Network, Search, Sparkles, X } from "lucide-react";
import KnowledgeGraphScene from "./KnowledgeGraphScene";
import { isDesktopRuntime, loadKnowledgeGraph, type KnowledgeGraphNode, type KnowledgeGraphNodeKind, type KnowledgeGraphPayload } from "./lib/desktop";
import type { PositionedKnowledgeNode } from "./lib/knowledge-graph-layout";
import "./knowledge-space.css";

type RangeKey = "today" | "7d" | "30d";

interface KnowledgeGraphPageProps {
  initialPayload?: KnowledgeGraphPayload;
  onBack: () => void;
  onOpenTimeline: (node: KnowledgeGraphNode) => void;
}

const filterOptions: Array<{ kind: KnowledgeGraphNodeKind; label: string; icon: React.ReactNode }> = [
  { kind: "category", label: "分类", icon: <CircleDot size={14} /> },
  { kind: "app", label: "应用", icon: <AppWindow size={14} /> },
  { kind: "domain", label: "网页域名", icon: <Globe2 size={14} /> },
  { kind: "day", label: "日期", icon: <CalendarDays size={14} /> },
  { kind: "activity", label: "活动", icon: <Activity size={14} /> },
  { kind: "browser-visit", label: "网页访问", icon: <Network size={14} /> },
];

export default function KnowledgeGraphPage({ initialPayload, onBack, onOpenTimeline }: KnowledgeGraphPageProps) {
  const [range, setRange] = useState<RangeKey>("30d");
  const [payload, setPayload] = useState<KnowledgeGraphPayload>(() => initialPayload ?? createPreviewKnowledgeGraph());
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [enabledKinds, setEnabledKinds] = useState<Set<KnowledgeGraphNodeKind>>(() => new Set(filterOptions.map((item) => item.kind)));
  const [bloomMode, setBloomMode] = useState<"strong" | "readable">("strong");
  const [selected, setSelected] = useState<PositionedKnowledgeNode | null>(null);
  const [hovered, setHovered] = useState<{ node: PositionedKnowledgeNode; x: number; y: number } | null>(null);
  const [reducedQuality, setReducedQuality] = useState(false);

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    const { startMs, endMs } = graphRange(range);
    setLoading(true);
    setSelected(null);
    void loadKnowledgeGraph(startMs, endMs)
      .then(setPayload)
      .finally(() => setLoading(false));
  }, [range]);

  const visiblePayload = useMemo(() => filterGraphPayload(payload, enabledKinds, search), [payload, enabledKinds, search]);
  const selectedNode = selected && visiblePayload.nodes.some((node) => node.id === selected.id) ? selected : null;

  const toggleKind = (kind: KnowledgeGraphNodeKind) => {
    setEnabledKinds((current) => {
      const next = new Set(current);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
  };

  return <div className="knowledge-space-shell">
    <header className="knowledge-topbar">
      <button className="knowledge-icon-button" aria-label="返回数据表盘" onClick={onBack}><ArrowLeft size={18} /></button>
      <div className="knowledge-brand"><i><Network size={17} /></i><span>DAILY TASK MONITOR</span><strong>知识空间</strong></div>
      <div className="knowledge-stats"><span>{visiblePayload.nodes.length.toLocaleString()} 个节点</span><span>{visiblePayload.links.length.toLocaleString()} 条关系</span>{reducedQuality && <em>已自动降低光效质量</em>}</div>
      <div className="knowledge-range" role="group" aria-label="图谱时间范围">
        <button className={range === "30d" ? "active" : ""} onClick={() => setRange("30d")}>近 30 天</button>
        <button className={range === "7d" ? "active" : ""} onClick={() => setRange("7d")}>近 7 天</button>
        <button className={range === "today" ? "active" : ""} onClick={() => setRange("today")}>今日</button>
      </div>
    </header>

    <aside className="knowledge-filter-rail">
      <div className="knowledge-rail-title"><span>FILTERS</span><strong>关系层</strong></div>
      <label className="knowledge-search"><Search size={15} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索节点或标题" />{search && <button aria-label="清除搜索" onClick={() => setSearch("")}><X size={14} /></button>}</label>
      <div className="knowledge-filter-list">
        {filterOptions.map((option) => <button key={option.kind} className={enabledKinds.has(option.kind) ? "active" : ""} aria-pressed={enabledKinds.has(option.kind)} onClick={() => toggleKind(option.kind)}>{option.icon}<span>{option.label}</span><b>{payload.counts[option.kind] ?? 0}</b></button>)}
      </div>
      <div className="knowledge-mode">
        <span>RENDER</span><strong>光效模式</strong>
        <div><button className={bloomMode === "strong" ? "active" : ""} onClick={() => setBloomMode("strong")}><Sparkles size={14} />强光</button><button className={bloomMode === "readable" ? "active" : ""} onClick={() => setBloomMode("readable")}><Eye size={14} />可读</button></div>
      </div>
      <p>拖动旋转 · 滚轮缩放<br />点击节点固定关系</p>
    </aside>

    <main className="knowledge-stage">
      {loading && <div className="knowledge-loading"><Sparkles size={19} />正在重建关系空间</div>}
      <KnowledgeGraphScene payload={visiblePayload} selectedId={selectedNode?.id ?? null} bloomMode={bloomMode} onHover={(node, x, y) => setHovered(node ? { node, x, y } : null)} onSelect={(node) => setSelected(node)} onQualityChange={setReducedQuality} />
      <div className="knowledge-stage-label"><span>KNOWLEDGE FIELD</span><strong>{range === "today" ? "今日活动关系" : range === "7d" ? "近七日活动关系" : "三十日活动关系"}</strong></div>
      {hovered && <div className="knowledge-tooltip" style={{ left: Math.min(window.innerWidth - 250, hovered.x + 16), top: Math.min(window.innerHeight - 110, hovered.y + 16) }}><span>{nodeKindLabel(hovered.node.kind)}</span><strong>{hovered.node.label}</strong><small>{formatDuration(hovered.node.durationSeconds)} · 置信度 {Math.round(hovered.node.confidence * 100)}%</small></div>}
    </main>

    <aside className="knowledge-inspector">
      <div className="knowledge-rail-title"><span>INSPECTOR</span><strong>节点详情</strong></div>
      {selectedNode ? <>
        <div className="knowledge-node-mark" style={{ "--node-color": selectedNode.color } as React.CSSProperties}><i /><span>{nodeKindLabel(selectedNode.kind)}</span></div>
        <h2>{selectedNode.label}</h2>
        <dl><div><dt>累计时间</dt><dd>{formatDuration(selectedNode.durationSeconds)}</dd></div><div><dt>分类置信度</dt><dd>{Math.round(selectedNode.confidence * 100)}%</dd></div><div><dt>节点类型</dt><dd>{nodeKindLabel(selectedNode.kind)}</dd></div></dl>
        {Object.entries(selectedNode.metadata).slice(0, 4).map(([key, value]) => <p key={key}><span>{key}</span><b>{value}</b></p>)}
        <button className="knowledge-primary" onClick={() => onOpenTimeline(selectedNode)}>查看对应时间线</button>
        <button className="knowledge-secondary" onClick={() => setSelected(null)}><EyeOff size={15} />取消固定</button>
      </> : <div className="knowledge-empty"><Network size={30} /><strong>选择一个节点</strong><p>点击图谱中的活动、应用或网页域名，查看它与当天工作的关系。</p></div>}
    </aside>
  </div>;
}

function graphRange(range: RangeKey) {
  const now = new Date();
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime();
  const days = range === "today" ? 1 : range === "7d" ? 7 : 30;
  return { startMs: end - days * 86_400_000, endMs: end };
}

function filterGraphPayload(payload: KnowledgeGraphPayload, enabledKinds: Set<KnowledgeGraphNodeKind>, search: string): KnowledgeGraphPayload {
  const query = search.trim().toLocaleLowerCase();
  const directMatches = new Set(payload.nodes.filter((node) => enabledKinds.has(node.kind) && (!query || [node.label, ...Object.values(node.metadata)].some((value) => value.toLocaleLowerCase().includes(query)))).map((node) => node.id));
  const visible = new Set(directMatches);
  if (query) for (const link of payload.links) {
    if (directMatches.has(link.source)) visible.add(link.target);
    if (directMatches.has(link.target)) visible.add(link.source);
  }
  const nodes = payload.nodes.filter((node) => visible.has(node.id) && enabledKinds.has(node.kind));
  const ids = new Set(nodes.map((node) => node.id));
  const links = payload.links.filter((link) => ids.has(link.source) && ids.has(link.target));
  return { ...payload, nodes, links };
}

function nodeKindLabel(kind: KnowledgeGraphNodeKind) {
  return filterOptions.find((option) => option.kind === kind)?.label ?? kind;
}

function formatDuration(seconds: number) {
  if (seconds < 60) return seconds > 0 ? "不足 1 分钟" : "即时访问";
  const minutes = Math.round(seconds / 60);
  return minutes >= 60 ? `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟` : `${minutes} 分钟`;
}

function createPreviewKnowledgeGraph(): KnowledgeGraphPayload {
  const categories = ["research", "creation_development", "text_input", "video_input", "social", "game", "file_management", "idle"];
  const categoryLabels = ["搜索/调研", "创作开发", "文字输入", "视频输入", "社交", "游戏", "文件整理", "空闲"];
  const apps = ["Chrome", "Codex", "VS Code", "Obsidian", "WeChat", "Explorer", "League Client"];
  const domains = ["github.com", "openai.com", "youtube.com", "zhihu.com", "bilibili.com", "docs.rs", "developer.mozilla.org", "google.com"];
  const nodes: KnowledgeGraphPayload["nodes"] = [];
  const links: KnowledgeGraphPayload["links"] = [];
  categories.forEach((category, index) => nodes.push({ id: `category:${category}`, kind: "category", label: categoryLabels[index], durationSeconds: 18_000 - index * 1_100, category, confidence: .98, occurredAtMs: null, metadata: {} }));
  apps.forEach((app, index) => nodes.push({ id: `app:${app.toLowerCase().replaceAll(" ", "-")}`, kind: "app", label: app, durationSeconds: 14_000 - index * 1_000, category: "", confidence: .9, occurredAtMs: null, metadata: {} }));
  domains.forEach((domain, index) => nodes.push({ id: `domain:${domain}`, kind: "domain", label: domain, durationSeconds: 5_000 - index * 310, category: "", confidence: .76, occurredAtMs: null, metadata: {} }));
  for (let day = 0; day < 30; day += 1) nodes.push({ id: `day:2026-06-${String(day + 1).padStart(2, "0")}`, kind: "day", label: `06/${String(day + 1).padStart(2, "0")}`, durationSeconds: 9_000, category: "", confidence: 1, occurredAtMs: null, metadata: {} });
  for (let index = 0; index < 720; index += 1) {
    const category = categories[index % categories.length];
    const app = apps[(index * 5 + 2) % apps.length];
    const day = index % 30;
    const id = `activity:preview-${index}`;
    const durationSeconds = 60 + (index * 47) % 1_500;
    nodes.push({ id, kind: "activity", label: `${app} · ${categoryLabels[index % categoryLabels.length]} ${index + 1}`, durationSeconds, category, confidence: .64 + (index % 34) / 100, occurredAtMs: Date.now() - day * 86_400_000, metadata: { app } });
    links.push({ source: id, target: `category:${category}`, kind: "classified-as", weightSeconds: durationSeconds });
    links.push({ source: id, target: `app:${app.toLowerCase().replaceAll(" ", "-")}`, kind: "used-app", weightSeconds: durationSeconds });
    links.push({ source: id, target: `day:2026-06-${String(day + 1).padStart(2, "0")}`, kind: "occurred-on", weightSeconds: durationSeconds });
  }
  for (let index = 0; index < 280; index += 1) {
    const domain = domains[index % domains.length];
    const id = `visit:preview-${index}`;
    nodes.push({ id, kind: "browser-visit", label: `${domain} 页面 ${index + 1}`, durationSeconds: 0, category: "research", confidence: .72, occurredAtMs: Date.now() - (index % 30) * 86_400_000, metadata: { domain } });
    links.push({ source: id, target: `domain:${domain}`, kind: "visited-domain", weightSeconds: 1 });
    links.push({ source: id, target: `activity:preview-${(index * 7) % 720}`, kind: "browser-context", weightSeconds: 1 });
  }
  const counts = nodes.reduce<Record<string, number>>((total, node) => ({ ...total, [node.kind]: (total[node.kind] ?? 0) + 1 }), {});
  return { nodes, links, counts, totalSeconds: 168_000, startMs: Date.now() - 30 * 86_400_000, endMs: Date.now() };
}
