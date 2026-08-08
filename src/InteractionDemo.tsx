import { useEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";
import * as echarts from "echarts/core";
import { PieChart } from "echarts/charts";
import { TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { Activity, ArrowLeft, ChevronRight, Clock3, Headphones, Pause, RotateCcw, Sparkles } from "lucide-react";
import { FaChrome, FaCode } from "react-icons/fa6";
import { SiObsidian, SiWechat } from "react-icons/si";
import "./interaction-demo.css";

echarts.use([PieChart, TooltipComponent, CanvasRenderer]);

type DemoItem = { name: string; percent: number; duration: string; color: string; icon?: React.ReactNode };
type TooltipState = { x: number; y: number; item: DemoItem; pinned: boolean } | null;

const categoryData: DemoItem[] = [
  { name: "创作开发", percent: 28, duration: "2 小时 24 分钟", color: "#496fae" },
  { name: "空闲", percent: 19, duration: "1 小时 39 分钟", color: "#8b9cae" },
  { name: "视频信息输入", percent: 16, duration: "1 小时 24 分钟", color: "#b86e91" },
  { name: "搜索/调研", percent: 15, duration: "1 小时 18 分钟", color: "#ba8a55" },
  { name: "文字信息输入", percent: 9, duration: "45 分钟", color: "#4d96a8" },
  { name: "游戏", percent: 7, duration: "36 分钟", color: "#a85b65" },
  { name: "文件整理", percent: 4, duration: "21 分钟", color: "#4b8d83" },
  { name: "社交软件", percent: 2, duration: "15 分钟", color: "#7a69a8" },
];

const appData: DemoItem[] = [
  { name: "Chrome", percent: 38, duration: "2 小时 42 分钟", color: "#496fae", icon: <FaChrome /> },
  { name: "Codex", percent: 17, duration: "1 小时 12 分钟", color: "#4d96a8", icon: <FaCode /> },
  { name: "Code", percent: 17, duration: "1 小时 12 分钟", color: "#6d6fae", icon: <FaCode /> },
  { name: "Obsidian", percent: 11, duration: "45 分钟", color: "#7c67a1", icon: <SiObsidian /> },
  { name: "League Client", percent: 9, duration: "36 分钟", color: "#a88051", icon: <Activity /> },
  { name: "Explorer", percent: 5, duration: "21 分钟", color: "#4d8b7d", icon: <Activity /> },
  { name: "WeChat", percent: 3, duration: "15 分钟", color: "#5d956f", icon: <SiWechat /> },
];

const periods = [
  { label: "凌晨", range: "00-06", buckets: [{ range: "00-02", learning: 0, other: 0 }, { range: "02-04", learning: 0, other: 0 }, { range: "04-06", learning: 0, other: 0 }] },
  { label: "上午", range: "06-12", buckets: [{ range: "06-08", learning: 15, other: 2 }, { range: "08-10", learning: 88, other: 12 }, { range: "10-12", learning: 62, other: 3 }] },
  { label: "下午", range: "12-18", buckets: [{ range: "12-14", learning: 50, other: 1 }, { range: "14-16", learning: 52, other: 48 }, { range: "16-18", learning: 1, other: 25 }] },
  { label: "晚上", range: "18-24", buckets: [{ range: "18-20", learning: 8, other: 18 }, { range: "20-22", learning: 32, other: 42 }, { range: "22-24", learning: 12, other: 8 }] },
];

export default function InteractionDemo() {
  const [tooltip, setTooltip] = useState<TooltipState>(null);
  const [soundEnabled, setSoundEnabled] = useState(false);
  const [animationKey, setAnimationKey] = useState(0);
  const playSound = useSubtleSound(soundEnabled);
  const metrics = useMemo(() => [
    ["总监测", "8 小时 42 分钟", "#4b8d83"], ["活跃", "7 小时 3 分钟", "#496fae"], ["学习", "5 小时 21 分钟", "#ba8a55"],
    ["空闲", "1 小时 39 分钟", "#8b9cae"], ["切换负荷", "11 次", "#7a69a8"], ["最长专注", "1 小时 12 分钟", "#4d8b7d"],
  ], []);

  const showItem = (item: DemoItem, x: number, y: number, pinned = false) => {
    if (!tooltip?.pinned || pinned) setTooltip({ item, x, y, pinned });
  };

  return <div className="interaction-demo" key={animationKey} onPointerDown={() => playSound("click")}>
    <header className="demo-header">
      <a href="/" className="demo-back"><ArrowLeft size={16} />返回当前表盘</a>
      <div><span>MINERAL INTERFACE STUDY</span><h1>雾蓝矿物分析台</h1></div>
      <div className="demo-controls">
        <button className={soundEnabled ? "sound-on" : ""} onClick={() => setSoundEnabled((value) => !value)}>{soundEnabled ? <Headphones size={16} /> : <Pause size={16} />}声音反馈</button>
        <button onClick={() => { setTooltip(null); setAnimationKey((value) => value + 1); }}><RotateCcw size={16} />重播生成</button>
      </div>
    </header>

    <main className="demo-main">
      <section className="demo-intro"><div><span>CALM MATERIAL / PRECISE DATA</span><h2>层次来自切面、留白与微弱矿物光泽</h2></div><p>无厚重塑料、无持续炫光。悬停查看，点击固定。</p></section>
      <section className="demo-metrics">
        {metrics.map(([label, value, color], index) => <InteractiveSurface key={label} color={color} onHover={() => playSound("hover")}><span>{label}</span><strong>{value}</strong><small>查看对应记录</small><ChevronRight size={15} /><i style={{ animationDelay: `${index * 55}ms` }} /></InteractiveSurface>)}
      </section>

      <section className="demo-analysis">
        <DataPanel title="任务分布" eyebrow="DISTRIBUTION" icon={<Sparkles size={17} />} items={categoryData} total="8 小时 42 分钟" tooltip={tooltip} playSound={playSound} showItem={showItem} clearTooltip={() => !tooltip?.pinned && setTooltip(null)} />
        <DataPanel title="应用排行" eyebrow="APPLICATIONS" icon={<Activity size={17} />} items={appData} total="7 小时 3 分钟" tooltip={tooltip} playSound={playSound} showItem={showItem} clearTooltip={() => !tooltip?.pinned && setTooltip(null)} />
      </section>

      <section className="demo-time-panel">
        <PanelHead eyebrow="HOURLY" title="两小时节律矩阵" icon={<div className="demo-legend"><span className="learn" />学习 <span className="other" />其他活跃</div>} />
        <div className="demo-period-grid">
          {periods.map((period, periodIndex) => <article className="period-quadrant" key={period.label} style={{ animationDelay: `${periodIndex * 75}ms` }}>
            <header><div><strong>{period.label}</strong><span>{period.range}</span></div><Clock3 size={15} /></header>
            <div className="tight-bars">
              {period.buckets.map((bucket) => <div className="demo-bucket" key={bucket.range}>
                <div className="demo-column">
                  <button className="column-part learning" aria-label={`${bucket.range} 学习 ${durationFromPercent(bucket.learning)}`} style={{ height: `${bucket.learning}%` }} disabled={!bucket.learning} onPointerEnter={(event) => { playSound("hover"); showItem({ name: `${bucket.range} 学习`, duration: durationFromPercent(bucket.learning), percent: bucket.learning, color: "#4f968d" }, event.clientX, event.clientY); }} onPointerLeave={() => !tooltip?.pinned && setTooltip(null)} onClick={(event) => showItem({ name: `${bucket.range} 学习`, duration: durationFromPercent(bucket.learning), percent: bucket.learning, color: "#4f968d" }, event.clientX, event.clientY, true)} />
                  <button className="column-part other" aria-label={`${bucket.range} 其他活跃 ${durationFromPercent(bucket.other)}`} style={{ height: `${bucket.other}%` }} disabled={!bucket.other} onPointerEnter={(event) => { playSound("hover"); showItem({ name: `${bucket.range} 其他活跃`, duration: durationFromPercent(bucket.other), percent: bucket.other, color: "#6179aa" }, event.clientX, event.clientY); }} onPointerLeave={() => !tooltip?.pinned && setTooltip(null)} onClick={(event) => showItem({ name: `${bucket.range} 其他活跃`, duration: durationFromPercent(bucket.other), percent: bucket.other, color: "#6179aa" }, event.clientX, event.clientY, true)} />
                </div><span>{bucket.range}</span>
              </div>)}
            </div>
          </article>)}
        </div>
      </section>
    </main>
    <FloatingTooltip tooltip={tooltip} onClose={() => setTooltip(null)} />
  </div>;
}

function DataPanel({ title, eyebrow, icon, items, total, tooltip, playSound, showItem, clearTooltip }: { title: string; eyebrow: string; icon: React.ReactNode; items: DemoItem[]; total: string; tooltip: TooltipState; playSound: (kind: "hover" | "click") => void; showItem: (item: DemoItem, x: number, y: number, pinned?: boolean) => void; clearTooltip: () => void }) {
  return <article className="demo-panel">
    <PanelHead eyebrow={eyebrow} title={title} icon={icon} />
    <div className="demo-split">
      <MineralDonut items={items} total={total} onHover={(item, x, y) => { playSound("hover"); showItem(item, x, y); }} onLeave={clearTooltip} onSelect={(item, x, y) => showItem(item, x, y, true)} />
      <div className="demo-list">
        {items.map((item, index) => <InteractiveSurface key={item.name} color={item.color} className="demo-data-row" onHover={(event) => { playSound("hover"); showItem(item, event.clientX, event.clientY); }} onLeave={clearTooltip} onClick={(event) => showItem(item, event.clientX, event.clientY, true)}><span>{item.icon && <i className="demo-app-icon">{item.icon}</i>}<i className="demo-dot" />{item.name}</span><b>{item.percent}%</b><small>{item.duration}</small><em><i style={{ width: `${item.percent * 2.1}%`, animationDelay: `${index * 45}ms` }} /></em></InteractiveSurface>)}
      </div>
    </div>
    {tooltip?.pinned && items.some((item) => item.name === tooltip.item.name) && <p className="demo-panel-note">已固定：{tooltip.item.name} · {tooltip.item.duration}</p>}
  </article>;
}

function MineralDonut({ items, total, onHover, onLeave, onSelect }: { items: DemoItem[]; total: string; onHover: (item: DemoItem, x: number, y: number) => void; onLeave: () => void; onSelect: (item: DemoItem, x: number, y: number) => void }) {
  const chartRef = useRef<HTMLDivElement>(null);
  const callbacksRef = useRef({ onHover, onLeave, onSelect });
  callbacksRef.current = { onHover, onLeave, onSelect };
  useEffect(() => {
    if (!chartRef.current) return;
    const chart = echarts.init(chartRef.current, undefined, { renderer: "canvas" });
    chart.setOption({
      animationDuration: 650,
      animationEasing: "cubicOut",
      tooltip: { show: false },
      series: [{ type: "pie", radius: ["54%", "76%"], center: ["50%", "48%"], startAngle: 110, clockwise: true, avoidLabelOverlap: true, label: { show: false }, itemStyle: { borderColor: "rgba(235,244,247,.9)", borderWidth: 3, borderRadius: 4, shadowBlur: 12, shadowColor: "rgba(36,63,78,.12)", shadowOffsetY: 5 }, emphasis: { scale: true, scaleSize: 7, itemStyle: { shadowBlur: 20, shadowColor: "rgba(43,76,94,.22)" } }, data: items.map((item) => ({ name: item.name, value: item.percent, itemStyle: { color: item.color } })) }],
    });
    const locate = (params: { event?: { event?: MouseEvent }; dataIndex?: number }) => {
      const item = items[params.dataIndex ?? -1];
      const event = params.event?.event;
      return item && event ? { item, x: event.clientX, y: event.clientY } : null;
    };
    const hover = (params: unknown) => { const hit = locate(params as never); if (hit) callbacksRef.current.onHover(hit.item, hit.x, hit.y); };
    const leave = () => callbacksRef.current.onLeave();
    const click = (params: unknown) => { const hit = locate(params as never); if (hit) callbacksRef.current.onSelect(hit.item, hit.x, hit.y); };
    chart.on("mouseover", hover);
    chart.on("mouseout", leave);
    chart.on("click", click);
    const observer = new ResizeObserver(() => chart.resize());
    observer.observe(chartRef.current);
    return () => { observer.disconnect(); chart.dispose(); };
  }, [items]);
  return <div className="mineral-donut"><div ref={chartRef} /><span>总时长<strong>{total}</strong><small>点击扇区固定</small></span></div>;
}

function InteractiveSurface({ children, color, className = "", onHover, onLeave, onClick }: { children: React.ReactNode; color: string; className?: string; onHover?: (event: ReactPointerEvent<HTMLButtonElement>) => void; onLeave?: () => void; onClick?: (event: ReactPointerEvent<HTMLButtonElement>) => void }) {
  return <button className={`demo-interactive ${className}`} style={{ "--feedback": color } as CSSProperties} onPointerEnter={onHover} onPointerLeave={onLeave} onPointerMove={(event) => { const rect = event.currentTarget.getBoundingClientRect(); event.currentTarget.style.setProperty("--mx", `${event.clientX - rect.left}px`); event.currentTarget.style.setProperty("--my", `${event.clientY - rect.top}px`); }} onClick={onClick}>{children}</button>;
}

function PanelHead({ eyebrow, title, icon }: { eyebrow: string; title: string; icon: React.ReactNode }) {
  return <div className="demo-panel-head"><div><span>{eyebrow}</span><h2>{title}</h2></div>{icon}</div>;
}

function FloatingTooltip({ tooltip, onClose }: { tooltip: TooltipState; onClose: () => void }) {
  if (!tooltip) return null;
  const left = Math.min(window.innerWidth - 282, Math.max(14, tooltip.x + 16));
  const top = Math.min(window.innerHeight - 160, Math.max(14, tooltip.y + 14));
  return createPortal(<aside className={`demo-tooltip ${tooltip.pinned ? "pinned" : ""}`} style={{ left, top, "--tooltip-color": tooltip.item.color } as CSSProperties}><span>{tooltip.pinned ? "已固定" : "实时数据"}</span><strong>{tooltip.item.name}</strong><div><b>{tooltip.item.duration}</b><em>{tooltip.item.percent}%</em></div>{tooltip.pinned && <button onClick={onClose}>关闭详情</button>}</aside>, document.body);
}

function useSubtleSound(enabled: boolean) {
  const contextRef = useRef<AudioContext | null>(null);
  const lastHover = useRef(0);
  return (kind: "hover" | "click") => {
    if (!enabled || (kind === "hover" && performance.now() - lastHover.current < 100)) return;
    if (kind === "hover") lastHover.current = performance.now();
    const context = contextRef.current ?? new AudioContext();
    contextRef.current = context;
    void context.resume();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.type = "sine";
    oscillator.frequency.value = kind === "hover" ? 520 : 340;
    gain.gain.setValueAtTime(.0001, context.currentTime);
    gain.gain.exponentialRampToValueAtTime(kind === "hover" ? .012 : .022, context.currentTime + .01);
    gain.gain.exponentialRampToValueAtTime(.0001, context.currentTime + .09);
    oscillator.connect(gain).connect(context.destination);
    oscillator.start();
    oscillator.stop(context.currentTime + .1);
  };
}

function durationFromPercent(percent: number) {
  const minutes = Math.round(percent * 1.2);
  return minutes >= 60 ? `${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分钟` : `${minutes} 分钟`;
}
