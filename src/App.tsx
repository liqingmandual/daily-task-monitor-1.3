import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  Bot,
  CalendarDays,
  ChevronRight,
  CircleCheck,
  CircleX,
  ClipboardCheck,
  FileText,
  Focus,
  Gauge,
  ListChecks,
  Network,
  RefreshCw,
  Settings,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  ChartNoAxesCombined,
  Timer,
  TriangleAlert,
  X,
} from "lucide-react";
import {
  buildDashboardMetrics,
  type ActivityCategory,
  type Segment,
  type TimeSeriesKey,
  type VideoPurpose,
} from "./lib/metrics";
import {
  consumePendingTimelineScroll,
  formatChartDuration,
} from "./lib/presentation";
import type { TimelineFilter } from "./lib/timeline-filter";
import { appIdentityKey, fallbackAppIdentity, type AppIdentity } from "./lib/app-identity";
import { TodayAnalysisPanels } from "./components/today/TodayAnalysisPanels";
import { CompactFocusPopover } from "./components/today/CompactFocusPopover";
import { DailyGoalPanel } from "./components/today/DailyGoalPanel";
import { DailyMarkdownExportButton } from "./components/today/DailyMarkdownExportButton";
import { AiAnalysisPanel } from "./components/today/AiAnalysisPanel";
import { TrendsView } from "./components/trends/TrendsView";
import { WorkflowPage } from "./components/workflow/WorkflowPage";
import { AiReviewPage, previewAiReviewRecords } from "./components/ai-review/AiReviewPage";
import { scrollIntoViewWithHeaderOffset, TimelinePanel } from "./components/today/TimelinePanel";
import { buildDailyAnalysisEvidence, buildLocalDailyAnalysis } from "./lib/daily-analysis";
import {
  shouldAutoQueueDailyAnalysis,
  type DailyAnalysisQueueCheckpoint,
} from "./lib/analysis-auto-queue";
import { buildAiReviewFilter, defaultAiReviewFilters, pendingReviewSubjectIds } from "./lib/ai-review";
import {
  ACTIVITY_SCOPE_STORAGE_KEYS,
  buildFallbackActivityCompositions,
  getCompositionLearningSeconds,
  parsePersistedActivityScope,
  type ActivityCompositions,
  type ActivityScope,
} from "./lib/activity-composition";
import {
  beginFocus,
  classifySegment,
  dayBounds,
  exportDailyReport,
  finishFocus,
  getAiConnectionHealth,
  getAppSettings,
  getCollectionHealth,
  getCodexHealth,
  importLegacyActivity,
  isDesktopRuntime,
  listAiProviders,
  listAiReviews,
  listBrowserSources,
  listSystemFonts,
  listenActivityChanged,
  listenAiConnectionHealthChanged,
  listenCollectionHealthChanged,
  listenDailyAnalysisChanged,
  listenWorkflowChanged,
  loadDashboardSnapshot,
  loadDailyAnalysis,
  enqueueDailyAnalysis,
  revealDataFolder,
  resolveAppIdentities,
  refreshAiConnectionHealth,
  saveAiKey,
  saveCustomProvider,
  scanBrowserHistory,
  testCodexCli,
  testAiProvider,
  toUiSegments,
  type AiProvider,
  type AiConnectionHealth,
  type AiConnectionHealthStatus,
  type AiReviewRecord,
  type AiExecutionMode,
  type BrowserSource,
  type CodexHealth,
  type CodexHealthStatus,
  type CollectionChannelHealth,
  type CollectionHealth,
  type DailyGoalRecord,
  type ScopedDailyAnalysisResult,
  type UiFont,
  type UiTheme,
  type WorkLedgerEvidence,
  type WorkLedgerRangeRollup,
  type WorkLedgerSnapshot,
  updateAiBackfill,
  updateAiAutomation,
  updateSettings,
  updateIdleThreshold,
  updateMonitoring,
  updatePrivacyExclusions,
  updateKnowledgeGraphExperiment,
  updateUiFont,
  updateUiTheme,
} from "./lib/desktop";
import type { KnowledgeGraphNode } from "./lib/desktop";

const KnowledgeGraphPage = lazy(() => import("./KnowledgeGraphPage"));

type Tab = "today" | "trends" | "workflow" | "ai-review" | "health";
type HeaderLayout = "mobile" | "compact" | "wide";

const tabOptions: Array<{ id: Tab; label: string; icon: typeof CalendarDays }> = [
  { id: "today", label: "今日", icon: CalendarDays },
  { id: "trends", label: "趋势", icon: ChartNoAxesCombined },
  { id: "workflow", label: "工作流", icon: ListChecks },
  { id: "ai-review", label: "AI 审核", icon: ClipboardCheck },
  { id: "health", label: "健康诊断", icon: Activity },
];

function resolveHeaderLayout(viewportWidth: number): HeaderLayout {
  if (viewportWidth <= 760) return "mobile";
  if (viewportWidth <= 1500) return "compact";
  return "wide";
}

const themeOptions: Array<{ id: UiTheme; name: string; note: string }> = [
  { id: "classic-workbench", name: "经典工作台", note: "清晰、克制，接近旧版体验" },
  { id: "moon-glass", name: "月白玻璃", note: "冰蓝玻璃与柔和高光" },
  { id: "soft-paper", name: "柔彩纸张", note: "低饱和色块与编辑式层级" },
  { id: "blueprint-data", name: "清透蓝图", note: "精细技术线条与数据动效" },
  { id: "knowledge-space", name: "知识空间", note: "深色关系控制台与高密度图谱" },
];

function normalizeStoredFont(value: string | null): UiFont {
  const normalized = value ? normalizeFontFamilyName(value) : "";
  return normalized && isUsableFontFamily(normalized) ? normalized : "Ubuntu";
}

function normalizeFontFamilyName(value: string): string {
  return value.trim().replace(/\.(?:ttf|tff|otf|ttc|dfont)$/i, "").trim();
}

function isUsableFontFamily(value: string): boolean {
  const normalized = value.trim();
  return normalized.length > 0
    && normalized.length <= 120
    && !normalized.startsWith(".")
    && !/[\u0000-\u001f\u007f]/.test(normalized);
}

function cssFontFamily(value: string): string {
  return `"${value.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

function normalizeStoredTheme(value: string | null): UiTheme {
  const legacyMap: Record<string, UiTheme> = {
    "precision-paper": "classic-workbench",
    "signal-console": "blueprint-data",
    "studio-blocks": "soft-paper",
  };
  if (value && themeOptions.some((item) => item.id === value)) return value as UiTheme;
  return value && legacyMap[value] ? legacyMap[value] : "classic-workbench";
}


const sampleSegments: Segment[] = [
  makeSegment("1", 7.7, 8.15, "research", "Chrome", "AI vocabulary research"),
  makeSegment("2", 8.15, 9.35, "creation_development", "Codex", "Daily Task Monitor desktop rewrite"),
  makeSegment("3", 9.35, 9.6, "social", "WeChat", "微信"),
  makeSegment("4", 9.6, 10.35, "text_input", "Obsidian", "English grammar notes"),
  makeSegment("5", 10.35, 11.25, "video_input", "Chrome", "Lecture: Memory and Learning", "learning"),
  makeSegment("6", 11.25, 12.0, "idle", "Idle", "Away"),
  makeSegment("7", 13.0, 14.2, "creation_development", "Code", "src-tauri classifier"),
  makeSegment("8", 14.2, 14.55, "file_management", "Explorer", "Project files"),
  makeSegment("9", 14.55, 15.4, "research", "Chrome", "Tauri Windows API documentation"),
  makeSegment("10", 15.4, 16.0, "game", "LeagueClientUx", "League of Legends"),
  makeSegment("11", 16.0, 16.5, "video_input", "Chrome", "Comedy video", "leisure"),
  makeSegment("12", 16.5, 17.4, "idle", "Idle", "Away"),
];

function makeSegment(
  id: string,
  startHour: number,
  endHour: number,
  category: ActivityCategory,
  app: string,
  title: string,
  videoPurpose: Segment["videoPurpose"] = "unknown",
): Segment {
  return {
    id,
    startMs: startHour * 3_600_000,
    endMs: endHour * 3_600_000,
    app,
    title,
    category,
    videoPurpose,
    confidence: category === "pending" ? 0.45 : 0.9,
    needsReview: category === "pending",
  };
}

function formatDuration(seconds: number): string {
  const rounded = Math.max(0, Math.round(seconds));
  const hours = Math.floor(rounded / 3_600);
  const minutes = Math.floor((rounded % 3_600) / 60);
  if (hours) return `${hours} 小时 ${minutes} 分钟`;
  if (minutes) return `${minutes} 分钟`;
  return `${rounded} 秒`;
}

function monitoredShare(seconds: number, monitoredSeconds: number): string {
  return monitoredSeconds > 0 ? `${(seconds / monitoredSeconds * 100).toFixed(1)}%` : "—";
}

const collectionStatusLabel: Record<CollectionChannelHealth["status"], string> = {
  healthy: "正常",
  degraded: "需关注",
  paused: "已暂停",
  unavailable: "不可用",
  "permission-denied": "缺少权限",
};

function CollectionHealthPanel({ health }: { health: CollectionHealth | null }) {
  if (!health) return <section className="panel collection-health-loading" aria-live="polite"><RefreshCw className="spinning" size={20} /><div><b>正在检查采集状态</b><small>正在读取本机权限、采集器和浏览器通道…</small></div></section>;
  const channels: Array<[string, CollectionChannelHealth]> = [
    ["桌面应用", health.desktop],
    ["窗口标题", health.windowTitle],
    ["空闲检测", health.idle],
    ["连续性", health.continuity],
    ["屏幕录制权限", health.screenRecording],
    ["浏览器实时 watcher", health.browserWatcher],
    ["浏览器历史证据", health.browserHistory],
  ];
  const issueCount = channels.filter(([, channel]) => channel.status !== "healthy").length;
  const overallStatus = !health.monitoringEnabled ? "采集已暂停" : issueCount ? `${issueCount} 项需关注` : "全部正常";
  const overallAccent = !health.monitoringEnabled ? "#d97706" : issueCount ? "#d97706" : "#059669";
  const formatLastSuccess = (value: number | null) => value
    ? `最近成功 ${new Date(value).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" })}`
    : "尚无成功记录";
  return <div className="collection-health" aria-label="采集健康诊断">
    <section className="metric-grid health-metric-grid" aria-label="采集健康概览">
      <article className="metric-card" style={{ "--accent": overallAccent } as React.CSSProperties}><span>整体状态</span><strong>{overallStatus}</strong><small>{health.monitoringEnabled ? `${channels.length - issueCount}/${channels.length} 个通道正常` : "可在设置中恢复桌面监测"}</small></article>
      <article className="metric-card" style={{ "--accent": "#2563eb" } as React.CSSProperties}><span>浏览器实时来源</span><strong>{health.watcherSourceCount}</strong><small>当前活跃的扩展连接</small></article>
      <article className="metric-card" style={{ "--accent": "#7c3aed" } as React.CSSProperties}><span>最近 24 小时网页计量</span><strong>{formatDuration(health.measuredBrowserSeconds)}</strong><small>{health.measuredBrowserSliceCount} 个可信心跳切片</small></article>
    </section>

    <section className="panel collection-channel-panel">
      <div className="panel-heading"><div><span>CHANNEL STATUS</span><h2>采集通道</h2></div><i><Activity size={18} /></i></div>
      <div className="collection-health-grid">
        {channels.map(([label, channel]) => <article className="collection-health-row" data-status={channel.status} key={label}>
          <div className="collection-health-row-head"><span><i aria-hidden="true" /><b>{label}</b></span><strong>{collectionStatusLabel[channel.status]}</strong></div>
          <p>{channel.detail}</p>
          <small>{formatLastSuccess(channel.lastSuccessAtMs)}</small>
        </article>)}
      </div>
    </section>

    <section className="panel watcher-config-panel">
      <div className="panel-heading"><div><span>BROWSER WATCHER</span><h2>浏览器扩展连接</h2></div><i><Network size={18} /></i></div>
      <p>扩展通过本机回环地址发送活动标签页心跳。连接码仅用于这台电脑，不会发送到外部服务。</p>
      <div className="watcher-config-grid">
        <label htmlFor="browser-watcher-endpoint"><span>扩展服务地址</span><input id="browser-watcher-endpoint" readOnly value={health.watcherEndpoint} /></label>
        <label htmlFor="browser-watcher-token"><span>本机连接码</span><input id="browser-watcher-token" readOnly value={health.watcherToken} /></label>
      </div>
    </section>
  </div>;
}

const DASHBOARD_SYNC_INTERVAL_MS = 60_000;

function MetricCard({ label, value, note, accent, onActivate }: { label: string; value: string; note: string; accent: string; onActivate?: () => void }) {
  const content = <>
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{note}</small>
      {onActivate && <ChevronRight className="metric-link-icon" size={17} />}
    </>;
  return onActivate ? (
    <button className="metric-card interactive" style={{ "--accent": accent } as React.CSSProperties} onClick={onActivate} aria-controls="activity-timeline">
      {content}
    </button>
  ) : <article className="metric-card" style={{ "--accent": accent } as React.CSSProperties}>{content}</article>;
}

type AiHealthTone = "preview" | "checking" | "healthy" | "unconfigured" | "failure";

const AI_DIAGNOSTIC_MAX_LENGTH = 240;

function sanitizeAiDiagnostic(value: unknown): string {
  const normalized = String(value).replace(/\s+/g, " ").trim();
  if (!normalized) return "";
  if (/https?:\/\/|(?:^|\s)bearer(?:\s|$)|api[\s_-]?key|authorization|(?:^|[\s"'=])sk-[a-z0-9_-]{6,}/i.test(normalized)) {
    return "诊断详情已脱敏";
  }
  return normalized.length > AI_DIAGNOSTIC_MAX_LENGTH
    ? `${normalized.slice(0, AI_DIAGNOSTIC_MAX_LENGTH - 3)}...`
    : normalized;
}

function sanitizeAiConnectionHealth(health: AiConnectionHealth): AiConnectionHealth {
  return {
    ...health,
    diagnostic: health.diagnostic ? sanitizeAiDiagnostic(health.diagnostic) : null,
  };
}

function selectNewerAiConnectionHealth(
  current: AiConnectionHealth | null,
  incoming: AiConnectionHealth,
): AiConnectionHealth {
  if (!current || incoming.checkedAtMs > current.checkedAtMs) return incoming;
  if (incoming.checkedAtMs < current.checkedAtMs) return current;
  if (current.status === "checking" && incoming.status !== "checking") return incoming;
  if (current.status !== "checking" && incoming.status === "checking") return current;
  return incoming;
}

function aiHealthPresentation(health: AiConnectionHealth | null, desktopRuntime: boolean): {
  ariaLabel: string;
  label: string;
  status: AiConnectionHealthStatus | "preview";
  title: string;
  tone: AiHealthTone;
} {
  if (!desktopRuntime) {
    const label = "AI · 桌面检测不可用";
    return { ariaLabel: label, label, status: "preview", title: label, tone: "preview" };
  }

  if (!health) {
    const label = "AI · 检测中";
    const title = "执行方式：正在读取\n状态：checking\n上次检查：尚未检查";
    return { ariaLabel: `${label}。${title.replaceAll("\n", "；")}`, label, status: "checking", title, tone: "checking" };
  }

  const executionModeLabel = health.executionMode === "codex" ? "本机 Codex" : "API Key";
  const unconfiguredLabel = health.executionMode === "codex"
    ? (health.executorLabel || "Codex")
    : "API";
  const executorLabel = health.executorLabel || executionModeLabel;
  const label = health.status === "healthy"
    ? `AI · ${executorLabel} 可用`
    : health.status === "reachable"
      ? `AI · ${executorLabel} 可达`
      : health.status === "rate-limited"
        ? `AI · ${executorLabel} 限流`
    : health.status === "checking"
      ? "AI · 检测中"
      : health.status === "unconfigured"
        ? `AI · ${unconfiguredLabel} 未配置`
        : health.status === "timed-out"
          ? `AI · ${executorLabel} 超时`
          : health.status === "permission-denied"
            ? `AI · ${executorLabel} 权限异常`
            : health.status === "unavailable"
              ? `AI · ${executorLabel} 不可用`
              : `AI · ${executorLabel} 错误`;
  const tone: AiHealthTone = health.status === "healthy" || health.status === "reachable"
    ? "healthy"
    : health.status === "checking"
      ? "checking"
      : health.status === "unconfigured"
        ? "unconfigured"
        : "failure";
  const diagnostic = health.diagnostic ? sanitizeAiDiagnostic(health.diagnostic) : "";
  const details = [
    `执行方式：${executionModeLabel}`,
    health.executorLabel || health.executorId
      ? `执行器：${health.executorLabel || health.executorId}${health.executorId && health.executorId !== health.executorLabel ? ` (${health.executorId})` : ""}`
      : null,
    health.model ? `模型：${health.model}` : null,
    `状态：${health.status}`,
    `验证层级：${health.verificationLevel === "inference" ? "真实推理" : "连接与登录"}`,
    `状态来源：${health.source === "manual" ? "手动测试" : health.source === "queue" ? "执行队列" : "后台检测"}`,
    `上次检查：${health.checkedAtMs > 0 ? new Date(health.checkedAtMs).toLocaleString("zh-CN", { hour12: false }) : "尚未检查"}`,
    health.verifiedAtMs ? `推理验证：${new Date(health.verifiedAtMs).toLocaleString("zh-CN", { hour12: false })}` : null,
    diagnostic ? `诊断：${diagnostic}` : null,
  ].filter((detail): detail is string => Boolean(detail));
  const title = details.join("\n");
  return { ariaLabel: `${label}。${details.join("；")}`, label, status: health.status, title, tone };
}

export default function App({ initialSegments }: { initialSegments?: Segment[] }) {
  const [headerLayout, setHeaderLayout] = useState<HeaderLayout>(() => (
    typeof window === "undefined" ? "wide" : resolveHeaderLayout(window.innerWidth)
  ));
  const [tab, setTab] = useState<Tab>("today");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [graphOpen, setGraphOpen] = useState(false);
  const [monitoring, setMonitoring] = useState(true);
  const [segments, setSegments] = useState(() => initialSegments ?? (isDesktopRuntime() ? [] : sampleSegments));
  const [authoritativeActivityCompositions, setAuthoritativeActivityCompositions] = useState<ActivityCompositions | null>(null);
  const [todayActivityScope, setTodayActivityScope] = useState<ActivityScope>(() => (
    typeof window === "undefined"
      ? "all"
      : parsePersistedActivityScope(window.localStorage.getItem(ACTIVITY_SCOPE_STORAGE_KEYS.today))
  ));
  const [appIdentities, setAppIdentities] = useState<Map<string, AppIdentity>>(() => new Map(
    (initialSegments ?? sampleSegments).map((segment) => {
      const path = segment.appPath ?? "";
      return [appIdentityKey(segment.app, path), fallbackAppIdentity(segment.app, path)];
    }),
  ));
  const [selectedDate, setSelectedDate] = useState(() => new Date().toLocaleDateString("sv-SE"));
  const [desktopMessage, setDesktopMessage] = useState("本地预览数据");
  const [providers, setProviders] = useState<AiProvider[]>([]);
  const [browserSources, setBrowserSources] = useState<BrowserSource[]>([]);
  const [collectionHealth, setCollectionHealth] = useState<CollectionHealth | null>(null);
  const [settingsMessage, setSettingsMessage] = useState("");
  const [idleThresholdMinutes, setIdleThresholdMinutes] = useState(6);
  const [aiBackfillEnabled, setAiBackfillEnabled] = useState(false);
  const [aiExecutionMode, setAiExecutionMode] = useState<AiExecutionMode>("api-key");
  const [selectedApiProviderId, setSelectedApiProviderId] = useState<string | null>(null);
  const [aiConnectionHealth, setAiConnectionHealth] = useState<AiConnectionHealth | null>(null);
  const [aiSettingsFocusRequest, setAiSettingsFocusRequest] = useState(0);
  const [codexExecutable, setCodexExecutable] = useState("codex");
  const [codexModel, setCodexModel] = useState("");
  const [codexHealth, setCodexHealth] = useState<CodexHealth | null>(null);
  const [codexHealthPending, setCodexHealthPending] = useState(false);
  const [aiAutomation, setAiAutomation] = useState({ trend: false, classification: false, workflow: false });
  const [aiAutomationNoticeVersion, setAiAutomationNoticeVersion] = useState<number | null>(
    () => isDesktopRuntime() ? null : 0,
  );
  const [excludedApps, setExcludedApps] = useState("");
  const [excludedDomains, setExcludedDomains] = useState("");
  const [drilldown, setDrilldown] = useState<TimelineFilter>({ mode: "all" });
  const [selectedSeries, setSelectedSeries] = useState<TimeSeriesKey[]>(["active", "learning"]);
  const [timelinePulse, setTimelinePulse] = useState(false);
  const [pendingTimelineScroll, setPendingTimelineScroll] = useState<number | null>(null);
  const [dashboardRefsReady, setDashboardRefsReady] = useState(false);
  const [uiTheme, setUiTheme] = useState<UiTheme>(() => {
    if (typeof window === "undefined") return "classic-workbench";
    return normalizeStoredTheme(window.localStorage.getItem("daily-task-monitor-ui-theme"));
  });
  const [uiFont, setUiFont] = useState<UiFont>(() => {
    if (typeof window === "undefined") return "Ubuntu";
    return normalizeStoredFont(window.localStorage.getItem("daily-task-monitor-ui-font"));
  });
  const [experimentalKnowledgeGraphEnabled, setExperimentalKnowledgeGraphEnabled] = useState(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem("daily-task-monitor-knowledge-graph") === "true";
  });
  const [systemFonts, setSystemFonts] = useState<string[]>(["Ubuntu"]);
  const [focusMinutes, setFocusMinutes] = useState(45);
  const [focusRunning, setFocusRunning] = useState(false);
  const [focusSessionId, setFocusSessionId] = useState<string | null>(null);
  const [focusOpen, setFocusOpen] = useState(false);
  const [dailyGoal, setDailyGoal] = useState<DailyGoalRecord>(() => ({
    date: selectedDate,
    goals: "继续完善桌面版任务监测系统",
    expectedOutput: "完成分类、采集和首页数据结构",
    actualOutput: "",
  }));
  const [dailyLedger, setDailyLedger] = useState<WorkLedgerSnapshot | null>(null);
  const [dailyWorkLedgerRollup, setDailyWorkLedgerRollup] = useState<WorkLedgerRangeRollup | null>(null);
  const [focusTaskId, setFocusTaskId] = useState("");
  const [workflowTaskId, setWorkflowTaskId] = useState<string | null>(null);
  const [workflowRefreshKey, setWorkflowRefreshKey] = useState(0);
  const [aiReviewSubjectId, setAiReviewSubjectId] = useState<string | null>(null);
  const [previewReviews] = useState<AiReviewRecord[]>(() => isDesktopRuntime() ? [] : previewAiReviewRecords());
  const previewReviewsRef = useRef(previewReviews);
  const [pendingReviewSubjects, setPendingReviewSubjects] = useState(() => pendingReviewSubjectIds(previewReviews));
  const [dailyAnalysis, setDailyAnalysis] = useState<ScopedDailyAnalysisResult | null>(null);
  const appFrameRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLElement>(null);
  const timelineRef = useRef<HTMLElement>(null);
  const timelineScrollRequestRef = useRef(0);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const settingsDrawerRef = useRef<HTMLElement>(null);
  const aiProviderSettingsRef = useRef<HTMLElement>(null);
  const aiHealthMountedRef = useRef(false);
  const aiConnectionHealthRef = useRef<AiConnectionHealth | null>(null);
  const aiNoticeDialogRef = useRef<HTMLElement>(null);
  const aiNoticeReturnFocusRef = useRef<HTMLElement | null>(null);
  const themeSelectionVersionRef = useRef(0);
  const idleThresholdSelectionVersionRef = useRef(0);
  const aiReviewMarkersMountedRef = useRef(false);
  const aiReviewMarkersRequestRef = useRef(0);
  const dashboardRequestRef = useRef(0);
  const dailyAutoQueueRef = useRef(new Map<string, DailyAnalysisQueueCheckpoint>());
  const metrics = useMemo(() => buildDashboardMetrics(segments), [segments]);
  const activityCompositions = useMemo(
    () => authoritativeActivityCompositions ?? buildFallbackActivityCompositions(segments),
    [authoritativeActivityCompositions, segments],
  );
  const aiStatus = aiHealthPresentation(aiConnectionHealth, isDesktopRuntime());
  const totalSwitches = Math.max(0, segments.length - 1);
  const switchesPerActiveHour = metrics.activeSeconds > 0
    ? totalSwitches * 3_600 / metrics.activeSeconds
    : null;
  const longestFocusSegment = segments
    .filter((item) => item.category !== "idle")
    .reduce<Segment | null>((longest, item) => !longest || item.endMs - item.startMs > longest.endMs - longest.startMs ? item : longest, null);
  const longestFocus = longestFocusSegment ? (longestFocusSegment.endMs - longestFocusSegment.startMs) / 1_000 : 0;
  const pendingSegments = segments.filter((item) => item.needsReview || item.category === "pending");
  const pendingSeconds = activityCompositions.all.items.find((item) => item.key === "pending")?.seconds ?? 0;
  const classificationCoverage = Math.round(
    ((metrics.monitoredSeconds - pendingSeconds) / Math.max(metrics.monitoredSeconds, 1)) * 100,
  );
  const localAnalysisEvidence = useMemo(() => {
    const composition = activityCompositions[todayActivityScope];
    const categorySeconds = Object.fromEntries(composition.items.map((item) => [item.key, item.seconds]));
    const idle = categorySeconds.idle ?? 0;
    const learning = getCompositionLearningSeconds(composition);
    const evidence = buildDailyAnalysisEvidence({
      date: selectedDate,
      goals: dailyGoal.goals,
      expectedOutput: dailyGoal.expectedOutput,
      actualOutput: dailyGoal.actualOutput,
      monitoredSeconds: composition.totalSeconds,
      activeSeconds: Math.max(0, composition.totalSeconds - idle),
      learningSeconds: learning,
      idleSeconds: idle,
      switchCount: todayActivityScope === "all" ? totalSwitches : 0,
      longestFocusSeconds: todayActivityScope === "all" ? longestFocus : 0,
      categorySeconds,
      topApps: todayActivityScope === "all"
        ? metrics.apps.slice(0, 5).map((item) => ({ name: item.name, seconds: item.seconds }))
        : [],
      browserVisitCount: 0,
      classificationCoverage: todayActivityScope === "all" ? classificationCoverage / 100 : 1,
    });
    return { ...evidence, evidenceHash: `${todayActivityScope}:${evidence.evidenceHash}` };
  }, [activityCompositions, classificationCoverage, dailyGoal, longestFocus, metrics.apps, selectedDate, todayActivityScope, totalSwitches]);
  const localDailyAnalysis = useMemo<ScopedDailyAnalysisResult>(() => {
    const local = buildLocalDailyAnalysis(localAnalysisEvidence);
    if (todayActivityScope === "all") return { ...local, activityScope: "all" };
    return {
      ...local,
      activityScope: "meaningful",
      recommendation: "本地事实已按学习口径计算；应用、切换和因果解释需等待同口径 AI 证据，当前可先核对目标与实际产出。",
      findings: local.findings?.slice(0, 1).map((finding) => ({
        ...finding,
        limitations: [...finding.limitations, "学习口径的本地回退不推断应用排行或切换原因。"],
      })),
    };
  }, [localAnalysisEvidence, todayActivityScope]);
  const visibleDailyAnalysis = dailyAnalysis ?? localDailyAnalysis;

  const setAppFrame = useCallback((node: HTMLDivElement | null) => {
    appFrameRef.current = node;
    setDashboardRefsReady(Boolean(node && timelineRef.current));
  }, []);

  const setTimeline = useCallback((node: HTMLElement | null) => {
    timelineRef.current = node;
    setDashboardRefsReady(Boolean(node && appFrameRef.current));
  }, []);

  const commitAiConnectionHealth = useCallback((health: AiConnectionHealth) => {
    if (!aiHealthMountedRef.current) return;
    const sanitized = sanitizeAiConnectionHealth(health);
    setAiConnectionHealth((current) => {
      const selected = selectNewerAiConnectionHealth(current, sanitized);
      aiConnectionHealthRef.current = selected;
      return selected;
    });
  }, []);

  const refreshUnifiedAiHealth = useCallback(async () => {
    if (!isDesktopRuntime()) return;
    try {
      const health = await refreshAiConnectionHealth();
      commitAiConnectionHealth(health);
    } catch (error) {
      if (!aiHealthMountedRef.current) return;
      const current = aiConnectionHealthRef.current;
      commitAiConnectionHealth({
        executionMode: current?.executionMode ?? aiExecutionMode,
        executorId: current?.executorId ?? (aiExecutionMode === "codex" ? "codex" : ""),
        executorLabel: current?.executorLabel ?? (aiExecutionMode === "codex" ? "Codex" : "API"),
        model: current?.model ?? "",
        status: "error",
        verificationLevel: "connectivity",
        source: "background",
        checkedAtMs: Date.now(),
        verifiedAtMs: current?.verifiedAtMs ?? null,
        diagnostic: sanitizeAiDiagnostic(error),
      });
    }
  }, [aiExecutionMode, commitAiConnectionHealth]);

  useEffect(() => {
    const updateHeaderLayout = () => setHeaderLayout(resolveHeaderLayout(window.innerWidth));
    window.addEventListener("resize", updateHeaderLayout);
    return () => window.removeEventListener("resize", updateHeaderLayout);
  }, []);

  useEffect(() => {
    aiHealthMountedRef.current = true;
    if (!isDesktopRuntime()) {
      return () => { aiHealthMountedRef.current = false; };
    }

    let active = true;
    let unlisten: (() => void) | undefined;
    void getAiConnectionHealth().then((health) => {
      if (active && health) commitAiConnectionHealth(health);
    }).catch(() => undefined);
    void listenAiConnectionHealthChanged((health) => {
      if (active) commitAiConnectionHealth(health);
    }).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    }).catch(() => undefined);

    return () => {
      active = false;
      aiHealthMountedRef.current = false;
      unlisten?.();
    };
  }, [commitAiConnectionHealth]);

  const drillToTimeline = (next: TimelineFilter) => {
    setDrilldown(next);
    setTimelinePulse(true);
    timelineScrollRequestRef.current += 1;
    setPendingTimelineScroll(timelineScrollRequestRef.current);
  };

  const openAiProviderSettings = () => {
    void refreshUnifiedAiHealth();
    setAiSettingsFocusRequest((request) => request + 1);
    setSettingsOpen(true);
  };

  useEffect(() => {
    const transition = consumePendingTimelineScroll(pendingTimelineScroll, dashboardRefsReady);
    if (!transition.shouldScroll) return;
    const timeline = timelineRef.current;
    const scrollContainer = appFrameRef.current;
    if (!timeline || !scrollContainer || !headerRef.current) return;
    scrollIntoViewWithHeaderOffset(timeline, headerRef.current, scrollContainer);
    setPendingTimelineScroll(transition.pendingRequest);
  }, [dashboardRefsReady, pendingTimelineScroll]);

  const chooseTheme = async (theme: UiTheme) => {
    const previousTheme = uiTheme;
    themeSelectionVersionRef.current += 1;
    const selectionVersion = themeSelectionVersionRef.current;
    setUiTheme(theme);
    window.localStorage.setItem("daily-task-monitor-ui-theme", theme);
    if (!isDesktopRuntime()) return;
    try {
      await updateUiTheme(theme);
      if (selectionVersion === themeSelectionVersionRef.current) setSettingsMessage("界面主题已保存");
    } catch (error) {
      if (selectionVersion !== themeSelectionVersionRef.current) return;
      setUiTheme(previousTheme);
      window.localStorage.setItem("daily-task-monitor-ui-theme", previousTheme);
      setSettingsMessage(`主题保存失败：${String(error)}`);
    }
  };

  const chooseFont = async (font: UiFont) => {
    const previousFont = uiFont;
    setUiFont(font);
    window.localStorage.setItem("daily-task-monitor-ui-font", font);
    if (!isDesktopRuntime()) return;
    try {
      await updateUiFont(font);
      setSettingsMessage("界面字体已保存");
    } catch (error) {
      setUiFont(previousFont);
      window.localStorage.setItem("daily-task-monitor-ui-font", previousFont);
      setSettingsMessage(`字体保存失败：${String(error)}`);
    }
  };

  const chooseIdleThreshold = async (minutes: number) => {
    const previousMinutes = idleThresholdMinutes;
    const selectionVersion = ++idleThresholdSelectionVersionRef.current;
    setIdleThresholdMinutes(minutes);
    try {
      await updateIdleThreshold(minutes);
      if (selectionVersion === idleThresholdSelectionVersionRef.current) {
        setSettingsMessage("不活跃阈值已保存");
      }
    } catch (error) {
      if (selectionVersion !== idleThresholdSelectionVersionRef.current) return;
      setIdleThresholdMinutes(previousMinutes);
      setSettingsMessage(`不活跃阈值保存失败：${String(error)}`);
    }
  };

  const handleThemePickerKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
    const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="radio"]'));
    const currentIndex = buttons.indexOf(event.target as HTMLButtonElement);
    if (currentIndex < 0) return;
    event.preventDefault();
    const direction = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1 : -1;
    const nextButton = buttons[(currentIndex + direction + buttons.length) % buttons.length];
    nextButton.focus();
    nextButton.click();
  };

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    const requestVersion = themeSelectionVersionRef.current;
    void getAppSettings().then((nextSettings) => {
      if (requestVersion !== themeSelectionVersionRef.current) return;
      setExperimentalKnowledgeGraphEnabled(nextSettings.experimentalKnowledgeGraphEnabled);
      setUiFont(nextSettings.uiFont || "Ubuntu");
      window.localStorage.setItem("daily-task-monitor-ui-font", nextSettings.uiFont || "Ubuntu");
      setIdleThresholdMinutes(nextSettings.idleThresholdMinutes);
      setAiExecutionMode(nextSettings.aiExecutionMode ?? "api-key");
      setSelectedApiProviderId(nextSettings.selectedApiProviderId ?? null);
      setAiAutomation({
        trend: nextSettings.aiAutoResearchAnalysisEnabled ?? nextSettings.aiAutoTrendAnalysisEnabled ?? true,
        classification: nextSettings.aiAutoClassificationEnabled ?? true,
        workflow: nextSettings.aiAutoWorkflowAssignmentEnabled ?? true,
      });
      setAiAutomationNoticeVersion(nextSettings.aiAutomationNoticeVersion ?? 0);
      window.localStorage.setItem("daily-task-monitor-knowledge-graph", String(nextSettings.experimentalKnowledgeGraphEnabled));
      if (!themeOptions.some((item) => item.id === nextSettings.uiTheme)) return;
      setUiTheme(nextSettings.uiTheme);
      window.localStorage.setItem("daily-task-monitor-ui-theme", nextSettings.uiTheme);
    }).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!settingsOpen) return;
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    window.requestAnimationFrame(() => settingsDrawerRef.current?.focus());
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setSettingsOpen(false);
        return;
      }
      if (event.key !== "Tab" || !settingsDrawerRef.current) return;
      const focusable = Array.from(settingsDrawerRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex="0"]',
      ));
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      (previouslyFocused ?? settingsButtonRef.current)?.focus();
    };
  }, [settingsOpen]);

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    void listSystemFonts().then((fonts) => {
      const normalizedFonts = fonts.map(normalizeFontFamilyName).filter(isUsableFontFamily);
      setSystemFonts(Array.from(new Set(["Ubuntu", ...normalizedFonts])).sort((left, right) => left.localeCompare(right)));
    }).catch((error) => setSettingsMessage(`字体列表读取失败：${String(error)}`));
  }, []);

  useEffect(() => {
    if (!settingsOpen || aiSettingsFocusRequest === 0) return;
    const frame = window.requestAnimationFrame(() => {
      const section = aiProviderSettingsRef.current;
      if (!section) return;
      section.scrollIntoView({ behavior: "smooth", block: "start" });
      section.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [aiSettingsFocusRequest, settingsOpen]);

  const aiAutomationNoticeOpen = aiAutomationNoticeVersion === 0;

  useEffect(() => {
    if (!aiAutomationNoticeOpen) return;
    aiNoticeReturnFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    window.requestAnimationFrame(() => aiNoticeDialogRef.current?.focus());
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        void acknowledgeAiAutomationNotice();
        return;
      }
      if (event.key !== "Tab" || !aiNoticeDialogRef.current) return;
      const focusable = Array.from(aiNoticeDialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex="0"]',
      ));
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      (aiNoticeReturnFocusRef.current ?? settingsButtonRef.current)?.focus();
      aiNoticeReturnFocusRef.current = null;
    };
  }, [aiAutomationNoticeOpen]);

  const acknowledgeAiAutomationNotice = async () => {
    setAiAutomationNoticeVersion(1);
    if (!isDesktopRuntime()) return;
    try {
      await updateSettings({ aiAutomationNoticeVersion: 1 });
    } catch (error) {
      setSettingsMessage(`AI 自动化说明保存失败：${String(error)}`);
    }
  };

  const refreshDashboard = async (): Promise<number | null> => {
    if (!isDesktopRuntime()) return null;
    const requestId = ++dashboardRequestRef.current;
    try {
      const dashboard = await loadDashboardSnapshot(selectedDate);
      if (requestId !== dashboardRequestRef.current) return null;
      const { startMs, endMs } = dayBounds(selectedDate);
      const nextSegments = toUiSegments(dashboard.timeline, startMs, endMs);
      setSegments(nextSegments);
      setAuthoritativeActivityCompositions(
        dashboard.activityComposition ?? buildFallbackActivityCompositions(nextSegments),
      );
      setDailyWorkLedgerRollup(dashboard.workLedger);
      try {
        const nextAppIdentities = await resolveAppIdentities(nextSegments);
        if (requestId === dashboardRequestRef.current) {
          setAppIdentities(nextAppIdentities);
        }
      } catch {
        if (requestId === dashboardRequestRef.current) {
          setAppIdentities(new Map(nextSegments.map((segment) => {
            const path = segment.appPath ?? "";
            return [appIdentityKey(segment.app, path), fallbackAppIdentity(segment.app, path)];
          })));
        }
      }
      if (requestId === dashboardRequestRef.current) {
        setDesktopMessage("SQLite 数据已同步");
      }
      return nextSegments.length;
    } catch (error) {
      if (requestId === dashboardRequestRef.current) {
        setDesktopMessage(`读取失败：${String(error)}`);
      }
      return null;
    }
  };

  const refreshAiReviewMarkers = useCallback(async (resolvedRecord?: AiReviewRecord) => {
    const requestId = ++aiReviewMarkersRequestRef.current;
    if (!isDesktopRuntime()) {
      if (!aiReviewMarkersMountedRef.current || requestId !== aiReviewMarkersRequestRef.current) return;
      if (resolvedRecord) {
        previewReviewsRef.current = previewReviewsRef.current.map((record) => (
          record.id === resolvedRecord.id ? resolvedRecord : record
        ));
      }
      setPendingReviewSubjects(pendingReviewSubjectIds(previewReviewsRef.current));
      return;
    }
    try {
      const reviews = await listAiReviews(buildAiReviewFilter("pending", defaultAiReviewFilters));
      if (!aiReviewMarkersMountedRef.current || requestId !== aiReviewMarkersRequestRef.current) return;
      setPendingReviewSubjects(pendingReviewSubjectIds(Array.isArray(reviews) ? reviews : []));
    } catch {
      // Preserve the last known markers while the review index is unavailable.
    }
  }, []);

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    const today = new Date().toLocaleDateString("sv-SE");
    const { startMs, endMs } = dayBounds(selectedDate);
    let active = true;
    let unlisten: (() => void) | undefined;
    let emptyRetryTimer: ReturnType<typeof setTimeout> | undefined;
    let activityRefreshTimer: ReturnType<typeof setTimeout> | undefined;
    let dashboardSyncTimer: ReturnType<typeof setInterval> | undefined;
    let lastActivityRefreshAt = Date.now();

    const refreshCurrentDay = () => {
      if (!active || selectedDate !== today || document.visibilityState === "hidden") return;
      if (emptyRetryTimer) {
        clearTimeout(emptyRetryTimer);
        emptyRetryTimer = undefined;
      }
      if (activityRefreshTimer) {
        clearTimeout(activityRefreshTimer);
        activityRefreshTimer = undefined;
      }
      lastActivityRefreshAt = Date.now();
      void refreshDashboard();
    };

    const refreshWhenVisible = () => {
      if (document.visibilityState !== "hidden") refreshCurrentDay();
    };

    void refreshDashboard().then((segmentCount) => {
      if (!active || segmentCount !== 0 || selectedDate !== today) return;
      emptyRetryTimer = setTimeout(() => {
        emptyRetryTimer = undefined;
        if (active) void refreshDashboard();
      }, DASHBOARD_SYNC_INTERVAL_MS);
    });

    void listenActivityChanged((event) => {
      if (!active || event.observedAtMs < startMs || event.observedAtMs >= endMs) return;
      if (emptyRetryTimer) {
        clearTimeout(emptyRetryTimer);
        emptyRetryTimer = undefined;
      }
      if (activityRefreshTimer) return;
      const elapsed = Date.now() - lastActivityRefreshAt;
      const delay = Math.max(500, DASHBOARD_SYNC_INTERVAL_MS - elapsed);
      activityRefreshTimer = setTimeout(() => {
        activityRefreshTimer = undefined;
        lastActivityRefreshAt = Date.now();
        if (active) void refreshDashboard();
      }, delay);
    }).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    }).catch(() => undefined);

    if (selectedDate === today) {
      dashboardSyncTimer = setInterval(refreshCurrentDay, DASHBOARD_SYNC_INTERVAL_MS);
      window.addEventListener("focus", refreshCurrentDay);
      document.addEventListener("visibilitychange", refreshWhenVisible);
    }

    return () => {
      active = false;
      if (emptyRetryTimer) clearTimeout(emptyRetryTimer);
      if (activityRefreshTimer) clearTimeout(activityRefreshTimer);
      if (dashboardSyncTimer) clearInterval(dashboardSyncTimer);
      window.removeEventListener("focus", refreshCurrentDay);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
      unlisten?.();
    };
  }, [selectedDate]);

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listenWorkflowChanged(() => {
      if (!active) return;
      setDailyAnalysis(null);
      void refreshDashboard();
    }).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    }).catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [selectedDate]);

  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(ACTIVITY_SCOPE_STORAGE_KEYS.today, todayActivityScope);
    }
  }, [todayActivityScope]);

  useEffect(() => {
    aiReviewMarkersMountedRef.current = true;
    return () => {
      aiReviewMarkersMountedRef.current = false;
      aiReviewMarkersRequestRef.current += 1;
    };
  }, []);

  useEffect(() => {
    void refreshAiReviewMarkers();
  }, [refreshAiReviewMarkers]);

  useEffect(() => {
    if (!isDesktopRuntime()) {
      setDailyAnalysis(null);
      return;
    }
    let cancelled = false;
    const autoQueue = async () => {
      if (!aiAutomation.trend) return false;
      const subject = `${selectedDate}:${todayActivityScope}`;
      const previous = dailyAutoQueueRef.current.get(subject);
      const checkpoint: DailyAnalysisQueueCheckpoint = {
        evidenceHash: localAnalysisEvidence.evidenceHash,
        monitoredSeconds: localAnalysisEvidence.monitoredSeconds,
        goalSignature: JSON.stringify([
          dailyGoal.goals,
          dailyGoal.expectedOutput,
          dailyGoal.actualOutput,
        ]),
        queuedAtMs: Date.now(),
      };
      if (!shouldAutoQueueDailyAnalysis(previous, checkpoint)) return false;
      dailyAutoQueueRef.current.set(subject, checkpoint);
      try {
        await enqueueDailyAnalysis(selectedDate, todayActivityScope);
        return true;
      } catch (error) {
        if (dailyAutoQueueRef.current.get(subject) === checkpoint) {
          if (previous) dailyAutoQueueRef.current.set(subject, previous);
          else dailyAutoQueueRef.current.delete(subject);
        }
        throw error;
      }
    };
    void loadDailyAnalysis(selectedDate, todayActivityScope).then(async (analysis) => {
      if (cancelled) return;
      if (analysis.activityScope !== todayActivityScope) {
        await autoQueue();
        if (!cancelled) setDailyAnalysis(null);
        return;
      }
      if (analysis.source !== "ai" && await autoQueue()) {
        if (!cancelled) setDailyAnalysis({ ...analysis, source: "queued" });
      } else {
        setDailyAnalysis(analysis);
      }
    }).catch(() => {
      if (!cancelled) setDailyAnalysis(null);
    });
    return () => { cancelled = true; };
  }, [
    aiAutomation.trend,
    dailyGoal.actualOutput,
    dailyGoal.expectedOutput,
    dailyGoal.goals,
    localAnalysisEvidence.evidenceHash,
    localAnalysisEvidence.monitoredSeconds,
    selectedDate,
    todayActivityScope,
  ]);

  const refreshCompletedDailyAnalysis = useCallback(async () => {
    try {
      const analysis = await loadDailyAnalysis(selectedDate, todayActivityScope);
      if (analysis.activityScope === todayActivityScope) setDailyAnalysis(analysis);
    } catch {
      // Keep the current result while a completion event races with persistence.
    }
  }, [selectedDate, todayActivityScope]);

  useEffect(() => {
    if (!isDesktopRuntime()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void listenDailyAnalysisChanged((event) => {
      const matchingDate = !event.date || event.date === selectedDate;
      const matchingRange = (!event.rangeStart || event.rangeStart === selectedDate)
        && (!event.rangeEnd || event.rangeEnd === selectedDate);
      if (!active || event.page !== "daily" || event.scope !== todayActivityScope || !matchingDate || !matchingRange) return;
      void refreshCompletedDailyAnalysis();
    }).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    }).catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [refreshCompletedDailyAnalysis, selectedDate, todayActivityScope]);

  const reanalyzeDaily = async () => {
    if (!isDesktopRuntime()) return;
    try {
      await enqueueDailyAnalysis(selectedDate, todayActivityScope);
      setDailyAnalysis({ ...localDailyAnalysis, source: "queued" });
    } catch (error) {
      setDesktopMessage(`重新分析排队失败：${String(error)}`);
    }
  };

  const toggleMonitoring = async () => {
    const next = !monitoring;
    setMonitoring(next);
    if (isDesktopRuntime()) await updateMonitoring(next);
  };

  const toggleFocus = async () => {
    if (!isDesktopRuntime()) {
      setFocusRunning((value) => !value);
      return;
    }
    const today = new Date().toLocaleDateString("sv-SE");
    if (!focusRunning && selectedDate !== today) return;
    if (!focusRunning) {
      const id = await beginFocus(selectedDate, dailyGoal.goals, focusMinutes, focusTaskId || null);
      setFocusSessionId(id);
      setFocusRunning(true);
    } else if (focusSessionId) {
      const completed = await finishFocus(focusSessionId, dailyGoal.actualOutput);
      setFocusRunning(false);
      setFocusSessionId(null);
      if (completed && focusTaskId) {
        setWorkflowTaskId(focusTaskId);
        setWorkflowRefreshKey((value) => value + 1);
        setTab("workflow");
      }
    }
  };

  const changeClassification = async (
    segmentId: string,
    category: ActivityCategory,
    videoPurpose: VideoPurpose,
  ) => {
    setSegments((items) => items.map((item) => item.id === segmentId
      ? {
        ...item,
        category,
        videoPurpose: category === "video_input" ? videoPurpose : "unknown",
        confidence: 1,
        needsReview: false,
      }
      : item));
    if (isDesktopRuntime()) {
      await classifySegment(segmentId, category, videoPurpose);
      await refreshDashboard();
    }
  };

  useEffect(() => {
    if (!settingsOpen || !isDesktopRuntime()) return;
    void Promise.all([listAiProviders(), listBrowserSources(), getAppSettings(), getCodexHealth()]).then(([nextProviders, nextSources, nextSettings, nextCodexHealth]) => {
      setProviders(nextProviders);
      setBrowserSources(nextSources);
      setMonitoring(nextSettings.monitoringEnabled);
      setIdleThresholdMinutes(nextSettings.idleThresholdMinutes);
      setAiBackfillEnabled(nextSettings.aiBackfillEnabled);
      setAiExecutionMode(nextSettings.aiExecutionMode ?? "api-key");
      setSelectedApiProviderId(nextSettings.selectedApiProviderId ?? null);
      setCodexExecutable(nextSettings.codexExecutable || "codex");
      setCodexModel(nextSettings.codexModel ?? "");
      setCodexHealth(nextCodexHealth);
      setAiAutomation({
        trend: nextSettings.aiAutoResearchAnalysisEnabled ?? nextSettings.aiAutoTrendAnalysisEnabled ?? true,
        classification: nextSettings.aiAutoClassificationEnabled ?? true,
        workflow: nextSettings.aiAutoWorkflowAssignmentEnabled ?? true,
      });
      setAiAutomationNoticeVersion(nextSettings.aiAutomationNoticeVersion ?? 0);
      setExcludedApps(nextSettings.excludedApps.join("\n"));
      setExcludedDomains(nextSettings.excludedDomains.join("\n"));
      setExperimentalKnowledgeGraphEnabled(nextSettings.experimentalKnowledgeGraphEnabled);
      setUiFont(nextSettings.uiFont || "Ubuntu");
      window.localStorage.setItem("daily-task-monitor-ui-font", nextSettings.uiFont || "Ubuntu");
      if (themeOptions.some((item) => item.id === nextSettings.uiTheme)) {
        setUiTheme(nextSettings.uiTheme);
        window.localStorage.setItem("daily-task-monitor-ui-theme", nextSettings.uiTheme);
      }
    }).catch((error) => setSettingsMessage(String(error)));
  }, [settingsOpen]);

  useEffect(() => {
    if (tab !== "health" || !isDesktopRuntime()) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      void getCollectionHealth().then((health) => {
        if (active) setCollectionHealth(health);
      }).catch((error) => {
        if (active) setDesktopMessage(`采集健康检查失败：${String(error)}`);
      });
    };
    refresh();
    const timer = setInterval(refresh, 15_000);
    void listenCollectionHealthChanged(refresh).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    }).catch(() => undefined);
    return () => {
      active = false;
      clearInterval(timer);
      unlisten?.();
    };
  }, [tab]);

  const configureProvider = async (provider: AiProvider) => {
    let persisted = false;
    try {
      if (provider.id === "custom") {
        const baseUrl = window.prompt("OpenAI 兼容接口地址", provider.baseUrl);
        if (baseUrl === null) return;
        const model = window.prompt("模型名称", provider.model);
        if (model === null) return;
        await saveCustomProvider(baseUrl, model);
        persisted = true;
      }
      const key = window.prompt(`输入 ${provider.name} API Key（留空会移除）`, "");
      if (key === null) return;
      await saveAiKey(provider.id, key);
      persisted = true;
      const [nextProviders, nextSettings] = await Promise.all([
        listAiProviders(),
        isDesktopRuntime() ? getAppSettings() : Promise.resolve(null),
      ]);
      setProviders(nextProviders);
      if (nextSettings) setSelectedApiProviderId(nextSettings.selectedApiProviderId ?? null);
      setSettingsMessage(`${provider.name} 凭据已保存到 Windows 凭据库`);
    } finally {
      if (persisted) await refreshUnifiedAiHealth();
    }
  };

  const verifyProvider = async (provider: AiProvider) => {
    try {
      setSettingsMessage(await testAiProvider(provider.id));
    } catch (error) {
      setSettingsMessage(`测试失败：${String(error)}`);
    } finally {
      await refreshUnifiedAiHealth();
    }
  };

  const scanBrowsers = async () => {
    const result = await scanBrowserHistory(selectedDate);
    setSettingsMessage(`扫描完成：发现 ${result.visitsFound} 条访问${result.errors.length ? `，${result.errors.length} 个数据源失败` : ""}`);
  };

  const importLegacy = async () => {
    if (!isDesktopRuntime()) return setSettingsMessage("旧数据导入仅在桌面安装版可用");
    const path = window.prompt("旧版 activity-log.jsonl 的完整路径", "");
    if (!path) return;
    const result = await importLegacyActivity(path);
    setSettingsMessage(`导入完成：新增 ${result.imported} 条，跳过 ${result.skipped} 条`);
    await refreshDashboard();
  };

  const openDataFolder = async () => {
    if (!isDesktopRuntime()) return setSettingsMessage("桌面安装版会打开本地数据目录");
    setSettingsMessage(`已打开：${await revealDataFolder()}`);
  };

  const toggleAiBackfill = async () => {
    const next = !aiBackfillEnabled;
    if (next && !window.confirm("开启后会把脱敏的应用名、窗口标题、域名和有限摘要发送给你已配置的 AI 提供商。确认开启吗？")) return;
    setAiBackfillEnabled(next);
    if (isDesktopRuntime()) await updateAiBackfill(next);
  };

  const verifyCodexCli = async () => {
    setCodexHealthPending(true);
    try {
      if (isDesktopRuntime()) {
        await updateSettings({
          codexExecutable: codexExecutable.trim() || "codex",
          codexModel: codexModel.trim(),
        });
      }
      setCodexHealth(await testCodexCli());
    } catch (error) {
      setSettingsMessage(`Codex 检查失败：${String(error)}`);
    } finally {
      await refreshUnifiedAiHealth();
      setCodexHealthPending(false);
    }
  };

  const saveCodexConfiguration = async () => {
    if (!isDesktopRuntime()) return;
    try {
      const nextSettings = await updateSettings({
        codexExecutable: codexExecutable.trim() || "codex",
        codexModel: codexModel.trim(),
      });
      setCodexExecutable(nextSettings.codexExecutable || "codex");
      setCodexModel(nextSettings.codexModel ?? "");
      setSettingsMessage("Codex 配置已保存");
      await refreshUnifiedAiHealth();
    } catch (error) {
      setSettingsMessage(`Codex 配置保存失败：${String(error)}`);
    }
  };

  const chooseAiExecutionMode = async (next: AiExecutionMode) => {
    const hasSelectedProvider = providers.some((provider) => (
      provider.id === selectedApiProviderId && provider.enabled && provider.hasCredential
    ));
    if (next === "api-key" && !hasSelectedProvider) {
      setSettingsMessage("请先选择一个已保存凭据的 API 主 Provider");
      return;
    }
    try {
      if (isDesktopRuntime()) {
        const nextSettings = await updateSettings({ aiExecutionMode: next });
        setAiExecutionMode(nextSettings.aiExecutionMode);
        setSelectedApiProviderId(nextSettings.selectedApiProviderId ?? null);
        await refreshUnifiedAiHealth();
      } else {
        setAiExecutionMode(next);
      }
      setSettingsMessage(next === "codex"
        ? "AI 执行方式已切换为本机 Codex；失败时不会回退 API。"
        : "AI 执行方式已切换为所选 API 主 Provider；失败时不会切换其他 Provider 或 Codex。");
    } catch (error) {
      setSettingsMessage(`切换失败：${String(error)}`);
    }
  };

  const choosePrimaryApiProvider = async (provider: AiProvider) => {
    if (!provider.enabled || !provider.hasCredential) return;
    try {
      if (isDesktopRuntime()) {
        const nextSettings = await updateSettings({ selectedApiProviderId: provider.id });
        setSelectedApiProviderId(nextSettings.selectedApiProviderId ?? null);
        if (nextSettings.aiExecutionMode === "api-key") await refreshUnifiedAiHealth();
      } else {
        setSelectedApiProviderId(provider.id);
      }
      setSettingsMessage(`${provider.name} 已设为 API 主 Provider`);
    } catch (error) {
      setSettingsMessage(`主 Provider 保存失败：${String(error)}`);
    }
  };

  const toggleAiAutomation = async (key: "trend" | "classification" | "workflow") => {
    const next = { ...aiAutomation, [key]: !aiAutomation[key] };
    setAiAutomation(next);
    if (isDesktopRuntime()) await updateAiAutomation({
      aiAutoResearchAnalysisEnabled: next.trend,
      aiAutoClassificationEnabled: next.classification,
      aiAutoWorkflowAssignmentEnabled: next.workflow,
    });
  };

  const savePrivacyExclusions = async () => {
    const lines = (value: string) => value.split(/[\n,]/).map((item) => item.trim()).filter(Boolean);
    if (isDesktopRuntime()) {
      await updatePrivacyExclusions(lines(excludedApps), lines(excludedDomains));
      setSettingsMessage("隐私排除列表已保存；这些应用和域名不会发送给云端 AI。");
    } else {
      setSettingsMessage("桌面安装版会把隐私排除保存在本地 SQLite 设置中。");
    }
  };

  const toggleKnowledgeGraphExperiment = async () => {
    const next = !experimentalKnowledgeGraphEnabled;
    setExperimentalKnowledgeGraphEnabled(next);
    window.localStorage.setItem("daily-task-monitor-knowledge-graph", String(next));
    if (!next && uiTheme === "knowledge-space") await chooseTheme("classic-workbench");
    if (isDesktopRuntime()) await updateKnowledgeGraphExperiment(next);
  };

  const openTimelineFromGraph = (node: KnowledgeGraphNode) => {
    setGraphOpen(false);
    setTab("today");
    if (node.kind === "activity") {
      drillToTimeline({ mode: "segment", segmentId: node.id.replace(/^activity:/, "") });
    } else if (node.kind === "app") {
      drillToTimeline({ mode: "app", app: node.label });
    } else if (node.kind === "category" && node.category) {
      drillToTimeline({ mode: "category", category: node.category as ActivityCategory });
    } else {
      drillToTimeline({ mode: "all" });
    }
  };

  const openTimelineFromWorkflow = (evidence: WorkLedgerEvidence) => {
    setTab("today");
    drillToTimeline({ mode: "segment", segmentId: evidence.id });
  };

  const openWorkflowTask = (taskId: string) => {
    setWorkflowTaskId(taskId);
    setWorkflowRefreshKey((value) => value + 1);
    setTab("workflow");
  };

  const openAiReview = (subjectId: string) => {
    setAiReviewSubjectId(subjectId);
    setTab("ai-review");
  };

  const handleAiReviewResolved = (record: AiReviewRecord) => {
    if (record.kind === "classification") void refreshDashboard();
    if (record.kind === "workflow_assignment" || record.kind === "project_draft") {
      setWorkflowRefreshKey((value) => value + 1);
    }
    void refreshAiReviewMarkers(record);
  };

  if (graphOpen) {
    return <Suspense fallback={<div className="demo-loading">正在构建知识空间...</div>}><KnowledgeGraphPage onBack={() => setGraphOpen(false)} onOpenTimeline={openTimelineFromGraph} /></Suspense>;
  }

  return (
    <div ref={setAppFrame} className="app-frame" data-theme={uiTheme} data-header-layout={headerLayout} data-scroll-container="dashboard" style={{ "--font-selected": cssFontFamily(uiFont) } as React.CSSProperties}>
      <header ref={headerRef} className="app-header">
        <div className="brand-lockup">
          <div className="brand-mark"><Gauge size={22} strokeWidth={2.2} /></div>
          <div>
            <span>LOCAL WORK CONSOLE</span>
            <h1>每日任务监测系统</h1>
          </div>
        </div>
        <nav className="main-tabs" aria-label="主导航">
          {tabOptions.map((item) => {
            const TabIcon = item.icon;
            return (
              <button key={item.id} className={tab === item.id ? "active" : ""} aria-current={tab === item.id ? "page" : undefined} onClick={() => { if (item.id === "ai-review") setAiReviewSubjectId(null); setTab(item.id); }}>
                <TabIcon size={15} strokeWidth={2} aria-hidden="true" />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="header-actions">
          <div className={`status-chip ${monitoring ? "online" : "paused"}`}>
            <span />{monitoring ? "监测中" : "已暂停"}
          </div>
          <button
            type="button"
            className={`ai-status-button ${aiStatus.tone}`}
            data-ai-health-status={aiStatus.status}
            aria-label={aiStatus.ariaLabel}
            aria-controls="ai-provider-settings"
            title={aiStatus.title}
            onClick={openAiProviderSettings}
          >
            <Bot size={16} aria-hidden="true" />
            <span className="ai-status-dot" aria-hidden="true" />
            <span className="ai-status-copy">{aiStatus.label}</span>
          </button>
          <input type="date" aria-label="选择日期" value={selectedDate} onChange={(event) => setSelectedDate(event.target.value)} />
          <div className="focus-popover-anchor">
            <button className="icon-button" aria-label="打开专注工具" aria-expanded={focusOpen} onClick={() => setFocusOpen((value) => !value)}><Focus size={18} /></button>
            {focusOpen && <CompactFocusPopover goal={dailyGoal.goals} minutes={focusMinutes} running={focusRunning} taskId={focusTaskId} tasks={dailyLedger?.tasks ?? []} canStart={selectedDate === new Date().toLocaleDateString("sv-SE")} onTaskChange={setFocusTaskId} onMinutesChange={setFocusMinutes} onToggle={() => void toggleFocus()} />}
          </div>
          <button className="icon-button" aria-label="刷新数据" onClick={() => void refreshDashboard()}><RefreshCw size={18} /></button>
          <button ref={settingsButtonRef} className="icon-button" aria-label="打开设置" onClick={() => setSettingsOpen(true)}><Settings size={19} /></button>
        </div>
      </header>

      <main>
        {tab === "today" && (
          <>
            <section className="page-heading">
              <div><span>TODAY</span><h2>今天的时间结构</h2></div>
              <p>总监测 {formatDuration(metrics.monitoredSeconds)} · 分类覆盖率 {classificationCoverage}% · 待复核/补算 {pendingSegments.length} 项{dailyWorkLedgerRollup ? ` · 台账任务 ${dailyWorkLedgerRollup.tasks.length}` : ""} · {desktopMessage}</p>
            </section>

            <section className="metric-grid" aria-label="今日核心指标">
              <MetricCard label="活跃" value={formatDuration(metrics.activeSeconds)} note={`占总监测 ${monitoredShare(metrics.activeSeconds, metrics.monitoredSeconds)}`} accent="#2563eb" onActivate={() => drillToTimeline({ mode: "active" })} />
              <MetricCard label="学习" value={formatDuration(metrics.learningSeconds)} note={`占总监测 ${monitoredShare(metrics.learningSeconds, metrics.monitoredSeconds)}`} accent="#d97706" onActivate={() => drillToTimeline({ mode: "learning" })} />
              <MetricCard label="最长专注" value={formatDuration(longestFocus)} note={`占总监测 ${monitoredShare(longestFocus, metrics.monitoredSeconds)}`} accent="#059669" onActivate={longestFocusSegment ? () => drillToTimeline({ mode: "segment", segmentId: longestFocusSegment.id }) : undefined} />
              <MetricCard label="切换频率" value={switchesPerActiveHour === null ? "—" : `${switchesPerActiveHour.toFixed(1)} 次/小时`} note={`共 ${totalSwitches} 次`} accent="#7c3aed" />
            </section>

            <TodayAnalysisPanels
              metrics={metrics}
              activityCompositions={activityCompositions}
              activityScope={todayActivityScope}
              onActivityScopeChange={setTodayActivityScope}
              identities={appIdentities}
              selectedSeries={selectedSeries}
              onSeriesChange={setSelectedSeries}
              onDrill={drillToTimeline}
              previewResetKey={selectedDate}
            />
            <TimelinePanel
              ref={setTimeline}
              segments={segments}
              identities={appIdentities}
              filter={drilldown}
              onFilterChange={setDrilldown}
              reviewSubjectIds={pendingReviewSubjects.classification}
              onChangeClassification={(segmentId, category, videoPurpose) => (
                void changeClassification(segmentId, category, videoPurpose)
              )}
              onOpenAiReview={openAiReview}
              pulse={timelinePulse}
              onPulseEnd={() => setTimelinePulse(false)}
            />

            <section className="goal-grid"><DailyGoalPanel selectedDate={selectedDate} localPreview={!isDesktopRuntime()} onGoalChange={setDailyGoal} onOpenTask={openWorkflowTask} onLedgerChanged={setDailyLedger} /></section>
            <AiAnalysisPanel analysis={visibleDailyAnalysis} onReanalyze={() => void reanalyzeDaily()} />
          </>
        )}

        {tab === "trends" && <TrendsView
          anchorDate={selectedDate}
          formatDuration={formatDuration}
          autoAnalysisEnabled={aiAutomation.trend}
          onOpenDate={(date) => {
            setSelectedDate(date);
            setTab("today");
            drillToTimeline({ mode: "all" });
          }}
          onOpenTask={openWorkflowTask}
        />}
        {tab === "workflow" && <WorkflowPage
          selectedDate={selectedDate}
          onOpenTimeline={openTimelineFromWorkflow}
          onOpenDate={(date) => {
            setSelectedDate(date);
            setTab("today");
            drillToTimeline({ mode: "all" });
          }}
          reviewSubjectIds={pendingReviewSubjects.workflowAssignment}
          onOpenAiReview={openAiReview}
          requestedTaskId={workflowTaskId}
          refreshKey={workflowRefreshKey}
        />}
        {tab === "ai-review" && <AiReviewPage subjectId={aiReviewSubjectId} onResolved={handleAiReviewResolved} />}
        {tab === "health" && <>
          <section className="page-heading">
            <div><span>COLLECTION HEALTH</span><h2>采集健康诊断</h2></div>
            <p>检查本机活动采集、系统权限、连续性以及浏览器实时数据通道。</p>
          </section>
          <div className="health-page-toolbar"><span>数据每 15 秒自动刷新</span><button className="secondary-action" type="button" aria-label="刷新采集健康诊断" onClick={() => void getCollectionHealth().then(setCollectionHealth).catch((error) => setDesktopMessage(`采集健康检查失败：${String(error)}`))}><RefreshCw size={16} />立即刷新</button></div>
          <CollectionHealthPanel health={collectionHealth} />
        </>}
      </main>

      {aiAutomationNoticeOpen && (
        <div className="drawer-layer ai-notice-layer" role="presentation">
          <aside
            ref={aiNoticeDialogRef}
            className="settings-drawer ai-notice-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="ai-automation-notice-title"
            tabIndex={-1}
          >
            <header>
              <div><span>AI AUTOMATION</span><h2 id="ai-automation-notice-title">AI 自动化首次说明</h2></div>
              <button className="icon-button" aria-label="关闭 AI 自动化说明" onClick={() => void acknowledgeAiAutomationNotice()}><X size={19} /></button>
            </header>
            <section className="settings-section">
              <h3><Sparkles size={18} />当前执行方式</h3>
              <p className="settings-copy">{aiExecutionMode === "codex" ? "本机 Codex" : "API Key 调用"} 将用于自动分析、待复核归类和工作流建议。</p>
            </section>
            <section className="settings-section">
              <h3><ShieldCheck size={18} />数据边界</h3>
              <p className="settings-copy">只提交完成判断所需的最小化统计、应用名、窗口标题、域名和有限摘要；隐私排除列表仍会先在本地生效。</p>
            </section>
            <section className="settings-section">
              <h3><Activity size={18} />资源与失败语义</h3>
              <p className="settings-copy">后台任务会消耗本机或提供商资源；失败会记录原因并保留本地统计，不会自动回退到另一条 AI 通道。</p>
            </section>
            <section className="settings-section">
              <h3><Settings size={18} />默认开启的自动化</h3>
              <ul className="settings-copy">
                <li>深度分析自动化：{aiAutomation.trend ? "开启" : "关闭"}</li>
                <li>任务自动归类：{aiAutomation.classification ? "开启" : "关闭"}</li>
                <li>工作流建议：{aiAutomation.workflow ? "开启" : "关闭"}</li>
              </ul>
            </section>
            <button className="wide-button" aria-label="关闭 AI 自动化说明" onClick={() => void acknowledgeAiAutomationNotice()}>我已了解</button>
          </aside>
        </div>
      )}

      {settingsOpen && (
        <div className="drawer-layer" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && setSettingsOpen(false)}>
          <aside ref={settingsDrawerRef} className="settings-drawer" role="dialog" aria-modal="true" aria-label="设置" tabIndex={-1}>
            <header><div><span>SETTINGS</span><h2>设置</h2></div><button className="icon-button" aria-label="关闭设置" onClick={() => setSettingsOpen(false)}><X size={19} /></button></header>
            <SettingsSection icon={<Sparkles size={18} />} title="界面主题">
              <div className="theme-picker" role="radiogroup" aria-label="选择界面主题" onKeyDown={handleThemePickerKeyDown}>
                {themeOptions.filter((theme) => theme.id !== "knowledge-space" || experimentalKnowledgeGraphEnabled).map((theme) => <button key={theme.id} role="radio" aria-checked={uiTheme === theme.id} tabIndex={uiTheme === theme.id ? 0 : -1} className={uiTheme === theme.id ? "selected" : ""} onClick={() => void chooseTheme(theme.id)}>
                  <span className={`theme-preview ${theme.id}`}><i /><i /><i /></span>
                  <b>{theme.name}</b><small>{theme.note}</small>
                </button>)}
              </div>
            </SettingsSection>
            <SettingsSection icon={<FileText size={18} />} title="界面字体">
              <label className="font-select-row">
                <span><b>字体</b><small>桌面端列出操作系统中已安装的字体；等宽内容仍使用 Ubuntu Mono。</small></span>
                <select aria-label="选择界面字体" value={uiFont} onChange={(event) => void chooseFont(event.target.value)}>
                  {Array.from(new Set([uiFont, ...systemFonts])).map((font) => <option key={font} value={font}>{font}</option>)}
                </select>
              </label>
            </SettingsSection>
            <SettingsSection icon={<Network size={18} />} title="实验功能">
              <div className="setting-row"><span><b>知识空间</b><small>用最近 30 天真实活动构建本地三维关系图谱</small></span><button className={experimentalKnowledgeGraphEnabled ? "toggle on" : "toggle"} aria-label="启用知识空间实验" aria-pressed={experimentalKnowledgeGraphEnabled} onClick={() => void toggleKnowledgeGraphExperiment()}><i /></button></div>
              {experimentalKnowledgeGraphEnabled && <div className="knowledge-experiment-actions"><button className="wide-button" onClick={() => void chooseTheme("knowledge-space")}>启用配套主题</button><button className="wide-button knowledge-entry" onClick={() => { setSettingsOpen(false); setGraphOpen(true); }}><Network size={16} />进入知识图谱</button></div>}
            </SettingsSection>
            <SettingsSection icon={<Activity size={18} />} title="监控与运行状态">
              <div className="setting-row"><span><b>桌面监测</b><small>前台窗口与输入信号</small></span><button className={monitoring ? "toggle on" : "toggle"} aria-pressed={monitoring} onClick={() => void toggleMonitoring()}><i /></button></div>
              <div className="setting-row"><span><b>不活跃阈值</b><small>从最后一次输入开始回填；监控断档另行记录</small></span><select aria-label="不活跃阈值" value={idleThresholdMinutes} onChange={(event) => void chooseIdleThreshold(Number(event.target.value))}><option value="6">6 分钟</option><option value="10">10 分钟</option><option value="15">15 分钟</option></select></div>
            </SettingsSection>
            <SettingsSection
              id="ai-provider-settings"
              headingId="ai-provider-settings-heading"
              sectionRef={aiProviderSettingsRef}
              tabIndex={-1}
              icon={<ShieldCheck size={18} />}
              title="AI 提供商"
            >
              <div className="setting-row"><span><b>联网自动补算</b><small>离线积压，联网后按优先级处理</small></span><button className={aiBackfillEnabled ? "toggle on" : "toggle"} aria-pressed={aiBackfillEnabled} onClick={() => void toggleAiBackfill()}><i /></button></div>
              <div className="setting-row ai-mode-row">
                <span><b>AI 执行方式</b><small>用于 AI 分析、工作流任务匹配和自动归类</small></span>
                <div className="ai-mode-switch" role="group" aria-label="AI 执行方式">
                  <button type="button" disabled={!providers.some((provider) => provider.id === selectedApiProviderId && provider.enabled && provider.hasCredential)} aria-pressed={aiExecutionMode === "api-key"} onClick={() => void chooseAiExecutionMode("api-key")}>API Key</button>
                  <button type="button" aria-pressed={aiExecutionMode === "codex"} onClick={() => void chooseAiExecutionMode("codex")}>本机 Codex</button>
                </div>
              </div>
              {aiExecutionMode === "codex" && <div className="codex-settings">
                <p className="settings-copy">当前仅调用本机 Codex；系统权限或 PATH 不可用时，AI 队列会记录失败原因。</p>
                <div className="codex-fields">
                  <label htmlFor="codex-executable"><span>可执行文件</span><input id="codex-executable" value={codexExecutable} onChange={(event) => setCodexExecutable(event.target.value)} onBlur={() => void saveCodexConfiguration()} /></label>
                  <label htmlFor="codex-model"><span>模型（可选）</span><input id="codex-model" value={codexModel} onChange={(event) => setCodexModel(event.target.value)} onBlur={() => void saveCodexConfiguration()} placeholder="使用 Codex 默认模型" /></label>
                </div>
                <div className="codex-health-head">
                  <b>运行健康</b>
                  <button type="button" className="icon-button codex-refresh" aria-label="刷新并测试 Codex" title="刷新并测试 Codex" disabled={codexHealthPending} onClick={() => void verifyCodexCli()}><RefreshCw size={17} className={codexHealthPending ? "spinning" : ""} /></button>
                </div>
                {codexHealth ? <CodexHealthDetails health={codexHealth} /> : <p className="codex-health-empty">尚未检查</p>}
              </div>}
              <div className="setting-row"><span><b>深度分析自动化</b><small>今日、趋势与工作流的新证据按冷却规则进入后台队列</small></span><button className={aiAutomation.trend ? "toggle on" : "toggle"} aria-pressed={aiAutomation.trend} onClick={() => void toggleAiAutomation("trend")}><i /></button></div>
              <div className="setting-row"><span><b>任务自动归类</b><small>仅处理待复核的新活动</small></span><button className={aiAutomation.classification ? "toggle on" : "toggle"} aria-pressed={aiAutomation.classification} onClick={() => void toggleAiAutomation("classification")}><i /></button></div>
              <div className="setting-row"><span><b>工作流建议</b><small>建议将按 85% 置信度规则处理</small></span><button className={aiAutomation.workflow ? "toggle on" : "toggle"} aria-pressed={aiAutomation.workflow} onClick={() => void toggleAiAutomation("workflow")}><i /></button></div>
              <div className="provider-list" role="radiogroup" aria-label="API 主 Provider">
              {(providers.length ? providers : [
                { id: "openai", name: "OpenAI", baseUrl: "", model: "gpt-4.1-mini", enabled: true, priority: 0, hasCredential: false },
                { id: "zhipu", name: "智谱", baseUrl: "", model: "glm-4-flash", enabled: true, priority: 1, hasCredential: false },
              ]).map((provider) => <div className="provider-row" key={provider.id}>
                <span className="provider-primary"><input type="radio" name="primary-ai-provider" value={provider.id} aria-label={`设为 ${provider.name} 主 Provider`} checked={selectedApiProviderId === provider.id && provider.hasCredential} disabled={!provider.enabled || !provider.hasCredential} onChange={() => void choosePrimaryApiProvider(provider)} /></span>
                <span className={`provider-mark ${provider.id === "openai" ? "blue" : ""}`}>{provider.name.slice(0, 1)}</span>
                <div><b>{provider.name}</b><small>{provider.hasCredential ? "已配置" : "未配置"} · {provider.model}</small></div>
                <span className="provider-actions"><button onClick={() => void configureProvider(provider)}>配置</button><button disabled={!provider.hasCredential} onClick={() => void verifyProvider(provider)}>测试</button></span>
              </div>)}
              </div>
            </SettingsSection>
            <SettingsSection icon={<FileText size={18} />} title="网页上下文记录">
              <p className="settings-copy">{browserSources.length ? `${browserSources.filter((item) => item.available).length} 个 Chrome / Edge Profile 可读取` : "正在检查 Chrome / Edge Profile"}</p>
              <button className="wide-button" onClick={() => void scanBrowsers()}>立即扫描</button>
            </SettingsSection>
            <SettingsSection icon={<ShieldCheck size={18} />} title="隐私排除">
              <label className="settings-field" htmlFor="excluded-apps">不发送给 AI 的应用（每行一个）</label>
              <textarea id="excluded-apps" value={excludedApps} onChange={(event) => setExcludedApps(event.target.value)} placeholder="例如：password-manager" />
              <label className="settings-field" htmlFor="excluded-domains">不采集、不发送的域名（每行一个）</label>
              <textarea id="excluded-domains" value={excludedDomains} onChange={(event) => setExcludedDomains(event.target.value)} placeholder="例如：company.internal" />
              <button className="wide-button" onClick={() => void savePrivacyExclusions()}>保存隐私排除</button>
            </SettingsSection>
            <SettingsSection icon={<Settings size={18} />} title="高级与校准">
              <DailyMarkdownExportButton date={selectedDate} desktopRuntime={isDesktopRuntime()} exporter={exportDailyReport} onMessage={setSettingsMessage} />
              <button className="wide-button" onClick={() => { setSettingsOpen(false); drillToTimeline({ mode: "category", category: "pending" }); }}>查看低置信分类</button>
              <button className="wide-button" onClick={() => void importLegacy()}>导入旧版数据</button>
              <button className="wide-button" onClick={() => void openDataFolder()}>打开数据目录</button>
            </SettingsSection>
            {settingsMessage && <p className="settings-feedback" role="status">{settingsMessage}</p>}
          </aside>
        </div>
      )}
    </div>
  );
}

function SettingsSection({
  children,
  headingId,
  icon,
  id,
  sectionRef,
  tabIndex,
  title,
}: {
  children: React.ReactNode;
  headingId?: string;
  icon: React.ReactNode;
  id?: string;
  sectionRef?: React.Ref<HTMLElement>;
  tabIndex?: number;
  title: string;
}) {
  return <section ref={sectionRef} id={id} className="settings-section" aria-labelledby={headingId} tabIndex={tabIndex}>
    <h3 id={headingId}>{icon}{title}</h3>{children}
  </section>;
}

const codexHealthPresentation: Record<CodexHealthStatus, { label: string; icon: React.ReactNode }> = {
  healthy: { label: "健康", icon: <CircleCheck size={17} /> },
  "permission-denied": { label: "权限被拒绝", icon: <ShieldAlert size={17} /> },
  unavailable: { label: "不可用", icon: <CircleX size={17} /> },
  "timed-out": { label: "检查超时", icon: <Timer size={17} /> },
  error: { label: "检查失败", icon: <TriangleAlert size={17} /> },
};

function CodexHealthDetails({ health }: { health: CodexHealth }) {
  const presentation = codexHealthPresentation[health.status];
  return <div className="codex-health" data-codex-health-status={health.status}>
    <div className="codex-health-status">{presentation.icon}<b>{presentation.label}</b></div>
    <dl>
      <div><dt>配置路径</dt><dd>{health.configuredPath || "未配置"}</dd></div>
      <div><dt>检测路径</dt><dd>{health.detectedPath || "未检测到"}</dd></div>
      <div><dt>版本</dt><dd>{health.version || "未知"}</dd></div>
      <div><dt>检查时间</dt><dd>{new Date(health.checkedAtMs).toLocaleString("zh-CN", { hour12: false })}</dd></div>
    </dl>
    {health.diagnostic && <p>{health.diagnostic}</p>}
  </div>;
}
