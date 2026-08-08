# Today Analysis Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the approved Today page with a three-column analysis row, selectable two-hour comparison chart, consistent timeline drill-down, Windows app identity/icons, compact focus access, and evidence-based AI analysis.

**Architecture:** Keep aggregation deterministic and testable in `src/lib/metrics.ts`, isolate visual panels under `src/components/today`, and make all drill-down actions produce one `TimelineFilter`. Extend the Rust collector and database only where Windows executable identity and persisted daily AI analysis require native access; the React layer consumes DTOs and never guesses executable paths.

**Tech Stack:** Tauri 2, Rust, rusqlite, Windows API, React 19, TypeScript 7, ECharts 6, Lucide React, Vitest.

## Global Constraints

- Windows 10/11 x64 remains the supported desktop platform.
- The Today analysis row is `Task Distribution / App Ranking / Time Distribution` in that order.
- Time distribution uses twelve two-hour buckets and a `0 / 60 / 120 min` scale.
- Default visible series are Active and Learning; Learning remains a subset of Active and is never added to Active totals.
- Action Queue is removed; Focus is an optional compact top-bar tool.
- AI output never evaluates mood, personality, or general ability.
- Offline statistics and reports continue to work; cloud AI requests remain optional and queued.
- No external CDN, network font, wallpaper, sound effect, or new chart library is introduced.
- Existing activity, browser, classification, and knowledge-graph data are not rewritten.

---

## File Structure

- Create `src/components/today/TimeDistributionPanel.tsx`: selectable two-hour comparison chart and hover/click interaction.
- Create `src/components/today/DistributionPanel.tsx`: task donut/list with stable chart lifecycle.
- Create `src/components/today/AppRankingPanel.tsx`: app donut/list using canonical identity.
- Create `src/components/today/TimelinePanel.tsx`: shared filtering and correctly offset scroll target.
- Create `src/components/today/AiAnalysisPanel.tsx`: portrait and recommendation output with offline state.
- Create `src/components/today/CompactFocusPopover.tsx`: optional top-bar focus control.
- Create `src/components/AppIcon.tsx`: local icon, bundled fallback, and generic fallback rendering.
- Create `src/lib/app-identity.ts`: canonical app naming and alias resolution.
- Create `src/lib/daily-analysis.ts`: deterministic evidence and local fallback analysis.
- Modify `src/App.tsx`: compose the extracted Today components and remove obsolete blocks.
- Modify `src/styles.css`: approved three-column layout, chart interaction, popover, and responsive rules.
- Modify `src/lib/metrics.ts`: expose chart series accessors without changing bucket accounting.
- Modify `src/lib/desktop.ts`: app identity and daily analysis DTO/command wrappers.
- Modify `src-tauri/src/windows_collector.rs`: retain executable path with each foreground sample.
- Modify `src-tauri/src/monitor.rs`: carry executable path through segment creation.
- Modify `src-tauri/src/db.rs`: persist `app_path`, icon cache metadata, and daily analysis results.
- Modify `src-tauri/src/app.rs`: build app identities and daily analysis evidence.
- Modify `src-tauri/src/desktop.rs`: expose icon and daily analysis Tauri commands and process the new AI job kind.

---

### Task 1: Time Series Semantics and Timeline Filter

**Files:**
- Modify: `src/lib/metrics.ts`
- Modify: `src/lib/metrics.test.ts`
- Create: `src/lib/timeline-filter.ts`
- Create: `src/lib/timeline-filter.test.ts`

**Interfaces:**
- Produces: `TimeSeriesKey`, `getTimeBucketSeriesSeconds(bucket, key)`, `TimelineFilter`, `matchesTimelineFilter(segment, filter)`.
- Consumes: existing `TimeBucketMetric`, `Segment`, `ActivityCategory`, and `isLearningSegment`.

- [ ] **Step 1: Write failing time-series tests**

```ts
it("maps active, learning, idle and atomic categories without double counting", () => {
  const bucket = metrics.timeBuckets[4];
  expect(getTimeBucketSeriesSeconds(bucket, "active")).toBe(bucket.activeSeconds);
  expect(getTimeBucketSeriesSeconds(bucket, "learning")).toBe(bucket.learningSeconds);
  expect(getTimeBucketSeriesSeconds(bucket, "idle")).toBe(bucket.idleSeconds);
  expect(getTimeBucketSeriesSeconds(bucket, "creation_development"))
    .toBe(bucket.categorySeconds.creation_development ?? 0);
});

it("uses a two-hour chart maximum", () => {
  expect(TWO_HOUR_BUCKET_MAX_SECONDS).toBe(7_200);
});
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `npm test -- src/lib/metrics.test.ts`  
Expected: FAIL because `getTimeBucketSeriesSeconds` and `TWO_HOUR_BUCKET_MAX_SECONDS` do not exist.

- [ ] **Step 3: Add explicit chart-series accessors**

```ts
export const TWO_HOUR_BUCKET_MAX_SECONDS = 2 * 60 * 60;

export type TimeSeriesKey = "active" | "learning" | ActivityCategory;

export function getTimeBucketSeriesSeconds(bucket: TimeBucketMetric, key: TimeSeriesKey): number {
  if (key === "active") return bucket.activeSeconds;
  if (key === "learning") return bucket.learningSeconds;
  if (key === "idle") return bucket.idleSeconds;
  return bucket.categorySeconds[key] ?? 0;
}
```

- [ ] **Step 4: Write failing unified timeline-filter tests**

```ts
it("filters a two-hour learning bucket by time and learning semantics", () => {
  const filter: TimelineFilter = { mode: "timeBucket", startHour: 8, endHour: 10, series: "learning" };
  expect(matchesTimelineFilter(researchSegment, filter)).toBe(true);
  expect(matchesTimelineFilter(gameSegment, filter)).toBe(false);
  expect(matchesTimelineFilter(lateResearchSegment, filter)).toBe(false);
});
```

- [ ] **Step 5: Implement `TimelineFilter` as the single drill-down contract**

```ts
export type TimelineFilter =
  | { mode: "all" }
  | { mode: "active" | "idle" | "learning" }
  | { mode: "category"; category: ActivityCategory }
  | { mode: "app"; app: string }
  | { mode: "segment"; segmentId: string }
  | { mode: "timeBucket"; startHour: number; endHour: number; series: TimeSeriesKey };

export function matchesTimelineFilter(segment: Segment, filter: TimelineFilter): boolean {
  if (filter.mode === "all") return true;
  if (filter.mode === "active") return segment.category !== "idle";
  if (filter.mode === "idle") return segment.category === "idle";
  if (filter.mode === "learning") return isLearningSegment(segment);
  if (filter.mode === "category") return segment.category === filter.category;
  if (filter.mode === "app") return segment.app === filter.app;
  if (filter.mode === "segment") return segment.id === filter.segmentId;
  const bucketStart = filter.startHour * 3_600_000;
  const bucketEnd = filter.endHour * 3_600_000;
  const overlaps = segment.endMs > bucketStart && segment.startMs < bucketEnd;
  if (!overlaps) return false;
  if (filter.series === "active") return segment.category !== "idle";
  if (filter.series === "learning") return isLearningSegment(segment);
  return segment.category === filter.series;
}
```

- [ ] **Step 6: Run focused tests and commit**

Run: `npm test -- src/lib/metrics.test.ts src/lib/timeline-filter.test.ts`  
Expected: PASS.  
Commit: `git add src/lib/metrics.ts src/lib/metrics.test.ts src/lib/timeline-filter.ts src/lib/timeline-filter.test.ts && git commit -m "feat: define today chart and timeline filter semantics"`

---

### Task 2: Three-Column Analysis Components

**Files:**
- Create: `src/components/today/TimeDistributionPanel.tsx`
- Create: `src/components/today/DistributionPanel.tsx`
- Create: `src/components/today/AppRankingPanel.tsx`
- Create: `src/components/today/AnalysisPanels.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `DashboardMetrics`, `TimeSeriesKey`, `TimelineFilter`, existing ECharts `DonutChart` behavior.
- Produces: three accessible panel components whose clicks call `onDrill(filter)` without owning timeline state.

- [ ] **Step 1: Write the failing static-render contract**

```tsx
const html = renderToStaticMarkup(
  <TodayAnalysisPanels metrics={metrics} selectedSeries={["active", "learning"]} onSeriesChange={() => {}} onDrill={() => {}} />,
);
expect(html).toContain('data-analysis-layout="three-column"');
expect(html).toContain("任务分布");
expect(html).toContain("应用排行");
expect(html).toContain("时间分布");
expect((html.match(/data-time-bucket=/g) ?? []).length).toBe(12);
expect(html).toContain("120 min");
```

- [ ] **Step 2: Run the test and verify failure**

Run: `npm test -- src/components/today/AnalysisPanels.test.tsx`  
Expected: FAIL because the new panel components do not exist.

- [ ] **Step 3: Implement the selectable two-hour chart**

```tsx
const DEFAULT_SERIES: TimeSeriesKey[] = ["active", "learning"];

export function TimeDistributionPanel({ buckets, selectedSeries, onSeriesChange, onDrill }: Props) {
  return <article className="panel time-panel">
    <PanelHeading eyebrow="HOURLY" title="时间分布" icon={<Clock3 size={18} />} />
    <SeriesPicker value={selectedSeries} defaultValue={DEFAULT_SERIES} onChange={onSeriesChange} />
    <div className="two-hour-chart" style={{ "--series-count": selectedSeries.length } as React.CSSProperties}>
      <ChartAxis ticks={[120, 60, 0]} />
      {buckets.map((bucket) => <div key={bucket.key} data-time-bucket={bucket.key} className="time-bucket-group">
        {selectedSeries.map((series) => {
          const seconds = getTimeBucketSeriesSeconds(bucket, series);
          return <button
            key={series}
            className={`time-series-bar series-${series}`}
            style={{ height: `${seconds / TWO_HOUR_BUCKET_MAX_SECONDS * 100}%` }}
            aria-label={`${bucket.range} ${timeSeriesLabel(series)} ${formatChartDuration(seconds)}`}
            onClick={() => onDrill({ mode: "timeBucket", startHour: bucket.startHour, endHour: bucket.endHour, series })}
          />;
        })}
        <span>{bucket.range}</span>
      </div>)}
    </div>
  </article>;
}
```

- [ ] **Step 4: Extract task and app panels without recreating charts on selection**

Keep ECharts input arrays memoized by metric values and render floating selection details through the existing portal tooltip. List-row hover and donut hover share the same preview state; click only updates `TimelineFilter`.

```tsx
const donutItems = useMemo(() => metrics.categories.map(toCategoryDonutItem), [metrics.categories]);
return <article className="panel distribution-panel">
  <PanelHeading eyebrow="DISTRIBUTION" title="任务分布" icon={<Activity size={18} />} />
  <div className="split-visual compact"><DonutChart items={donutItems} {...chartProps} /><CategoryList onDrill={onDrill} /></div>
</article>;
```

- [ ] **Step 5: Apply the approved responsive layout**

```css
.analysis-grid { display:grid; grid-template-columns:minmax(0,1fr) minmax(0,1fr) minmax(0,1.12fr); gap:12px; align-items:stretch; }
.analysis-grid .split-visual { grid-template-columns:clamp(118px,10vw,154px) minmax(0,1fr); gap:12px; }
.two-hour-chart { min-height:280px; display:grid; grid-template-columns:32px repeat(12,minmax(12px,1fr)); gap:3px; align-items:end; }
@media (max-width:1100px) { .analysis-grid { grid-template-columns:repeat(2,minmax(0,1fr)); } .time-panel { grid-column:1 / -1; } }
@media (max-width:760px) { .analysis-grid { grid-template-columns:1fr; } .time-panel { grid-column:auto; } }
```

- [ ] **Step 6: Run component and app tests, then commit**

Run: `npm test -- src/components/today/AnalysisPanels.test.tsx src/App.test.tsx`  
Expected: PASS and analysis panel order remains Distribution, Apps, Time.  
Commit: `git add src/components/today src/App.tsx src/styles.css && git commit -m "feat: rebuild today analysis row"`

---

### Task 3: Correct Timeline Drill-Down and Compact Focus

**Files:**
- Create: `src/components/today/TimelinePanel.tsx`
- Create: `src/components/today/CompactFocusPopover.tsx`
- Create: `src/components/today/TimelinePanel.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `TimelineFilter`, `matchesTimelineFilter`, existing `beginFocus` and `finishFocus` commands.
- Produces: `TimelinePanel` with `scrollIntoViewWithHeaderOffset()` and `CompactFocusPopover` mounted from the header.

- [ ] **Step 1: Write failing UI structure tests**

```tsx
expect(html).toContain('id="activity-timeline-heading"');
expect(html).toContain('data-scroll-offset="header"');
expect(html).not.toContain("今日行动队列");
expect(html).not.toContain('class="panel focus-card"');
expect(html).toContain('aria-label="打开专注工具"');
```

- [ ] **Step 2: Implement header-aware scrolling**

```ts
export function scrollIntoViewWithHeaderOffset(element: HTMLElement, headerHeight = 72): void {
  const top = element.getBoundingClientRect().top + window.scrollY - headerHeight - 24;
  window.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
}
```

Call this with the timeline heading wrapper, not the first matching row, so the heading and first result remain visible.

- [ ] **Step 3: Extract the timeline and use the shared filter**

```tsx
const visibleSegments = useMemo(
  () => segments.filter((segment) => matchesTimelineFilter(segment, filter)),
  [segments, filter],
);
```

The active filter chip uses `timelineFilterLabel(filter)` and clearing it restores `{ mode: "all" }`.

- [ ] **Step 4: Remove Action Queue and move Focus to a header popover**

```tsx
<button className="icon-button" aria-label="打开专注工具" onClick={() => setFocusOpen((value) => !value)}>
  <Focus size={18} />
</button>
{focusOpen && <CompactFocusPopover goal={goal} minutes={focusMinutes} running={focusRunning} onMinutesChange={setFocusMinutes} onToggle={toggleFocus} />}
```

- [ ] **Step 5: Run tests and commit**

Run: `npm test -- src/components/today/TimelinePanel.test.tsx src/App.test.tsx`  
Expected: PASS; no Action Queue or full Focus card appears.  
Commit: `git add src/components/today src/App.tsx src/styles.css && git commit -m "feat: streamline today actions and drilldown"`

---

### Task 4: Windows App Identity and Local Icon Cache

**Files:**
- Modify: `src-tauri/src/windows_collector.rs`
- Modify: `src-tauri/src/monitor.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/app-identity.ts`
- Create: `src/lib/app-identity.test.ts`
- Create: `src/components/AppIcon.tsx`
- Modify: `src/lib/desktop.ts`
- Modify: `src/App.tsx`
- Test: `src-tauri/tests/database.rs`
- Test: `src-tauri/tests/monitor_engine.rs`

**Interfaces:**
- Produces Rust DTO `AppIdentityDto { raw_name, display_name, executable_path, product_name, icon_data_url }`.
- Produces Tauri command `resolve_app_identities(apps: Vec<String>) -> Vec<AppIdentityDto>`.
- Produces TypeScript `resolveDisplayName(rawName, productName)` and `AppIcon`.

- [ ] **Step 1: Write failing Rust persistence tests**

```rust
let record = fixture_segment_with_path("ChatGPT", r"C:\\Program Files\\OpenAI\\ChatGPT.exe");
database.insert_segment(&record)?;
let loaded = database.list_segments(0, 10_000)?;
assert_eq!(loaded[0].app_path.as_deref(), Some(record.app_path.as_deref().unwrap()));
```

- [ ] **Step 2: Carry executable path through collection and storage**

Change `process_name(window)` to return both stem and full path:

```rust
fn process_identity(window: WindowHandle) -> Option<(String, String)> {
    let path = query_process_image_path(window)?;
    let name = Path::new(&path).file_stem()?.to_string_lossy().into_owned();
    Some((name, path))
}
```

Add `app_path: String` to `MonitorSample`, segment state, and `ActivitySegmentRecord`. Add a nullable `app_path TEXT NOT NULL DEFAULT ''` migration and keep legacy imports valid.

- [ ] **Step 3: Write failing alias tests**

```ts
expect(resolveDisplayName("Codex", "ChatGPT")).toBe("ChatGPT");
expect(resolveDisplayName("Code", "Visual Studio Code")).toBe("Visual Studio Code");
expect(resolveDisplayName("Codex", "")).toBe("Codex");
```

- [ ] **Step 4: Implement conservative canonical naming**

```ts
export function resolveDisplayName(rawName: string, productName = ""): string {
  const product = productName.trim();
  if (product) return product;
  return BUILTIN_ALIASES[rawName.toLowerCase()] ?? rawName;
}
```

Do not add `codex -> ChatGPT` to `BUILTIN_ALIASES`; the rename only occurs when Windows product metadata confirms it.

- [ ] **Step 5: Implement native icon extraction and cache**

Use the executable path as the cache key, extract the small icon through Windows Shell APIs, encode one PNG data URL, and cache it below `%LOCALAPPDATA%\DailyTaskMonitor\icon-cache`. The Tauri command returns cached data when present and a null icon when Windows extraction fails.

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIdentityDto {
    raw_name: String,
    display_name: String,
    executable_path: String,
    product_name: String,
    icon_data_url: Option<String>,
}
```

- [ ] **Step 6: Render one shared `AppIcon` in rankings and timeline**

```tsx
export function AppIcon({ identity }: { identity: AppIdentity }) {
  if (identity.iconDataUrl) return <img className="app-icon" src={identity.iconDataUrl} alt="" />;
  const fallback = bundledIconFor(identity.displayName);
  return fallback ?? <AppWindow className="app-icon fallback" aria-hidden="true" />;
}
```

- [ ] **Step 7: Run Rust and frontend tests, then commit**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`  
Run: `npm test -- src/lib/app-identity.test.ts src/App.test.tsx`  
Expected: all tests PASS.  
Commit: `git add src-tauri src/lib src/components/AppIcon.tsx src/App.tsx && git commit -m "feat: resolve Windows app identity and icons"`

---

### Task 5: Evidence-Based Daily AI Analysis

**Files:**
- Create: `src/lib/daily-analysis.ts`
- Create: `src/lib/daily-analysis.test.ts`
- Create: `src/components/today/AiAnalysisPanel.tsx`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src/lib/desktop.ts`
- Modify: `src/App.tsx`
- Test: `src-tauri/tests/app_service.rs`

**Interfaces:**
- Produces `DailyAnalysisEvidence`, `DailyAnalysisResult`, `buildLocalDailyAnalysis(evidence)`.
- Produces Tauri commands `get_daily_analysis(date, start_ms, end_ms)` and `queue_daily_analysis(date, start_ms, end_ms)`.
- Consumes goals, dashboard totals/timeline, browser summary counts, confidence coverage, and existing AI provider queue.

- [ ] **Step 1: Write deterministic fallback tests**

```ts
it("describes structure without mood or ability claims", () => {
  const result = buildLocalDailyAnalysis(fixtureEvidence);
  expect(result.portrait).toContain("学习");
  expect(result.recommendation).toContain("切换");
  expect(`${result.portrait}${result.recommendation}`).not.toMatch(/心情|人格|能力不足/);
});
```

- [ ] **Step 2: Define evidence and result DTOs**

```ts
export interface DailyAnalysisEvidence {
  date: string;
  goals: string;
  expectedOutput: string;
  actualOutput: string;
  monitoredSeconds: number;
  activeSeconds: number;
  learningSeconds: number;
  idleSeconds: number;
  switchCount: number;
  longestFocusSeconds: number;
  categorySeconds: Record<string, number>;
  topApps: Array<{ name: string; seconds: number }>;
  classificationCoverage: number;
}

export interface DailyAnalysisResult {
  portrait: string;
  recommendation: string;
  source: "local" | "ai" | "queued";
  evidenceHash: string;
  generatedAtMs: number;
}
```

- [ ] **Step 3: Persist daily AI results and queue by evidence hash**

Add table `daily_analyses(date TEXT PRIMARY KEY, evidence_hash TEXT, portrait TEXT, recommendation TEXT, source TEXT, generated_at_ms INTEGER)`. Enqueue kind `daily_analysis` with subject key `date:evidence_hash`; a duplicate payload must not create a second pending job.

- [ ] **Step 4: Add a dedicated AI prompt and parser**

The prompt requires strict JSON:

```json
{"portrait":"...","recommendation":"..."}
```

It instructs the provider to cite only supplied statistics, avoid mood/personality/ability judgments, and keep each field under 220 Chinese characters. Invalid JSON marks the job failed and uses existing exponential backoff.

- [ ] **Step 5: Render the two-section AI panel with offline state**

```tsx
<article className="panel ai-analysis-panel">
  <PanelHeading eyebrow="AI ANALYSIS" title="AI 分析" icon={<BrainCircuit size={18} />} />
  <div className="ai-analysis-grid">
    <AnalysisBlock title="今日工作画像" text={analysis.portrait} />
    <AnalysisBlock title="评价与建议" text={analysis.recommendation} />
  </div>
  {analysis.source === "queued" && <p className="analysis-status">等待联网后补算</p>}
</article>
```

- [ ] **Step 6: Run tests and commit**

Run: `npm test -- src/lib/daily-analysis.test.ts src/App.test.tsx`  
Run: `cargo test --manifest-path src-tauri/Cargo.toml app_service`  
Expected: PASS and the offline fallback is always available.  
Commit: `git add src-tauri src/lib src/components/today src/App.tsx && git commit -m "feat: add evidence-based daily analysis"`

---

### Task 6: Today Page Integration and Verification

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/styles.css`
- Modify: `docs/ARCHITECTURE.md`

**Interfaces:**
- Consumes every interface produced in Tasks 1-5.
- Produces the completed Today page; no new cross-module contract is introduced.

- [ ] **Step 1: Update the integration test to the approved structure**

```tsx
expect(html).toContain('data-analysis-layout="three-column"');
expect(html).toContain("120 min");
expect(html).toContain("今日工作画像");
expect(html).toContain("评价与建议");
expect(html).not.toContain("今日行动队列");
expect(html).toContain('aria-label="打开专注工具"');
```

- [ ] **Step 2: Compose the final Today view**

The React order is: heading, metrics, three analysis panels, timeline, goal/output, AI analysis. Keep settings and the knowledge-space experiment unchanged.

- [ ] **Step 3: Verify responsive and keyboard behavior**

At widths 1433, 1180, 980, and 599 pixels verify: no page-level horizontal overflow; task/app lists remain usable; the two-hour chart labels remain legible; each series bar, donut, list row, metric card, filter chip, and focus control is keyboard reachable.

- [ ] **Step 4: Run the full automated suite**

Run: `npm test`  
Expected: all Vitest tests PASS.  
Run: `npm run build`  
Expected: TypeScript and Vite production build PASS.  
Run: `cargo test --manifest-path src-tauri/Cargo.toml`  
Expected: all Rust tests PASS.

- [ ] **Step 5: Run the desktop smoke test**

Run: `npm run tauri dev`  
Verify the Today page loads real data, date changes refresh all panels, app icons resolve without blocking, timeline drill-down lands below the fixed header, offline mode shows local AI analysis, and the focus popover starts/completes a session.

- [ ] **Step 6: Update architecture notes and commit**

Document `TimelineFilter`, app identity caching, and daily analysis evidence flow in `docs/ARCHITECTURE.md`.

```bash
git add src docs/ARCHITECTURE.md
git commit -m "feat: complete today analysis redesign"
```

---

## Follow-On Plans

After this plan passes its smoke test, write and execute two independent plans:

1. `2026-07-11-trends-range-analysis.md` for weekly/monthly/custom range aggregation and AI comparison.
2. `2026-07-11-project-ledger.md` for the Project -> Task -> Evidence -> Review workflow.
