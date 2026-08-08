# Trends Range Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder Trends tab with a weekly, monthly, and custom-range decision dashboard that compares periods, explains statistical evidence, and supports offline-first AI evaluation.

**Architecture:** Rust owns clipping, daily aggregation, comparison periods, evidence hashing, persistence, and AI queueing. React receives one `TrendPayload` for the selected range and renders a stable desktop layout with range controls, KPI deltas, daily charts, composition changes, quality context, and an evidence-bounded evaluation. The UI never recomputes authoritative totals from raw timeline records.

**Tech Stack:** Tauri 2, Rust, SQLite, React 19, TypeScript, Vite, Vitest, existing CSS and Lucide icon system.

## Global Constraints

- Support Windows 10/11 x64 and remain fully usable offline.
- Range presets are exactly `7 days`, `30 days`, and `custom`; custom ranges are inclusive by local calendar date and limited to 366 days.
- Comparison always uses the immediately preceding range with the same number of local calendar days.
- Learning time remains `search/research + text input + creation/development + learning-purpose video`.
- AI analysis may only cite values present in `TrendEvidence`; it must not infer mood, personality, health, or ability.
- When AI is unavailable, deterministic local analysis and Markdown export remain available.
- AI jobs are deduplicated by SHA-256 evidence hash; stale results must never replace a newer range result.
- Do not modify original activity, browser, classification, goal, or daily-analysis records.
- Do not introduce a cloud account, external CDN, online font, or a second chart library.

---

### Task 1: Authoritative Range Aggregation Contract

**Files:**
- Modify: `src-tauri/src/domain.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src/lib/desktop.ts`
- Test: `src-tauri/tests/database.rs`
- Test: `src-tauri/tests/app_service.rs`
- Test: `src/lib/desktop.test.ts`

**Interfaces:**
- Consumes: `ActivitySegmentRecord`, `DashboardTotals`, `AppService::get_dashboard(start_ms, end_ms)`.
- Produces: `TrendRange`, `TrendDay`, `TrendBreakdownItem`, `TrendSummary`, `TrendComparison`, `TrendDataQuality`, and `TrendPayload`; Tauri command `get_trends(start_ms, end_ms, timezone_offset_minutes) -> TrendPayload`; TypeScript `loadTrendRange(startDate, endDate) -> Promise<TrendPayload>`.

- [ ] **Step 1: Write failing Rust aggregation tests**

Add fixtures crossing midnight, an idle segment, a learning video, and two apps. Assert clipped per-day totals, category/app rollups, coverage, immediately preceding equal-length comparison, and `sum(days.active_seconds) == summary.active_seconds`.

```rust
assert_eq!(payload.days.len(), 2);
assert_eq!(payload.summary.active_seconds, 5_400);
assert_eq!(payload.summary.learning_seconds, 3_600);
assert_eq!(payload.comparison.day_count, 2);
assert_eq!(payload.days.iter().map(|day| day.active_seconds).sum::<i64>(), payload.summary.active_seconds);
```

- [ ] **Step 2: Run focused Rust tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml trend_range -- --nocapture`

Expected: FAIL because `TrendPayload` and `AppService::get_trends` do not exist.

- [ ] **Step 3: Add the shared DTOs and aggregation service**

Define serialized camelCase DTOs. `TrendDay` contains date, monitored/active/idle/learning seconds, switches, longest-focus seconds, coverage, top category, and top app. `TrendSummary` contains totals, daily averages, productive-day count, focus-day count, and ranked category/app breakdowns. `TrendComparison` contains previous range plus signed percentage deltas represented as `Option<f64>` when the previous denominator is zero.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendRange {
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_date: String,
    pub end_date: String,
    pub day_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendPayload {
    pub range: TrendRange,
    pub days: Vec<TrendDay>,
    pub summary: TrendSummary,
    pub comparison: TrendComparison,
    pub quality: TrendDataQuality,
    pub evidence_hash: String,
}
```

Use one clipped segment query for the selected range and one for the comparison range. Aggregate in Rust by local date using the supplied timezone offset; never send the entire raw timeline to React.

- [ ] **Step 4: Replace the command and TypeScript bridge**

Change `get_trends` to return `TrendPayload`, validate `start_ms < end_ms` and `day_count <= 366`, and add exact TypeScript mirrors in `src/lib/desktop.ts`.

```ts
export async function loadTrendRange(startDate: string, endDate: string): Promise<TrendPayload> {
  const start = dayBounds(startDate).startMs;
  const end = dayBounds(endDate).endMs;
  return invoke<TrendPayload>("get_trends", {
    startMs: start,
    endMs: end,
    timezoneOffsetMinutes: new Date().getTimezoneOffset(),
  });
}
```

- [ ] **Step 5: Run contract tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml trend_range -- --nocapture`

Expected: PASS.

Run: `pnpm test -- --run src/lib/desktop.test.ts`

Expected: PASS with exact command argument and DTO parsing assertions.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/domain.rs src-tauri/src/db.rs src-tauri/src/app.rs src-tauri/src/desktop.rs src-tauri/tests/database.rs src-tauri/tests/app_service.rs src/lib/desktop.ts src/lib/desktop.test.ts
git commit -m "feat: add authoritative trend range aggregation"
```

### Task 2: Weekly, Monthly, and Custom Range Workbench

**Files:**
- Create: `src/components/trends/TrendsView.tsx`
- Create: `src/components/trends/TrendsView.test.tsx`
- Create: `src/components/trends/TrendTimelineChart.tsx`
- Create: `src/components/trends/TrendBreakdownPanel.tsx`
- Create: `src/lib/trend-range.ts`
- Create: `src/lib/trend-range.test.ts`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: `TrendPayload`, `loadTrendRange(startDate, endDate)`, existing `formatDuration`, `categoryMeta`, and `AppIcon`.
- Produces: `TrendRangeSelection`, `resolveTrendRange(preset, anchorDate, customStart, customEnd)`, and the complete Trends workbench.

- [ ] **Step 1: Write failing range and view tests**

Assert 7-day and 30-day inclusive ranges, leap-month custom dates, invalid/reversed ranges, and the 366-day limit. Render the view and assert the order: range toolbar, KPI comparison, daily trend, activity structure, app movement, evidence/quality, evaluation.

```ts
expect(resolveTrendRange("week", "2026-07-12", "", "")).toEqual({ startDate: "2026-07-06", endDate: "2026-07-12" });
expect(resolveTrendRange("month", "2026-07-12", "", "")).toEqual({ startDate: "2026-06-13", endDate: "2026-07-12" });
expect(() => resolveTrendRange("custom", "", "2026-07-12", "2026-07-01")).toThrow("开始日期不能晚于结束日期");
```

- [ ] **Step 2: Run frontend tests and verify failure**

Run: `pnpm test -- --run src/lib/trend-range.test.ts src/components/trends/TrendsView.test.tsx`

Expected: FAIL because the range resolver and Trends components do not exist.

- [ ] **Step 3: Implement the range toolbar and loading flow**

Use a segmented control for `近 7 天 / 近 30 天 / 自定义`, two date inputs only for custom mode, previous/next-range arrow icon buttons, and a refresh icon button. Keep range state inside `TrendsView`; emit no raw API parameters outside `loadTrendRange`.

- [ ] **Step 4: Implement the visual hierarchy**

Build a quiet, data-dense workbench:

1. Six compact KPI cells: active, learning, learning ratio, longest focus, switches/hour, coverage. Each shows current value and signed comparison delta.
2. Full-width daily chart: active duration as the full bar, learning as the inner colored segment, goal markers only when real goal data exists.
3. Two-column analysis row: category composition change and app movement, each with current duration, share, and comparison delta.
4. Evidence strip: recorded days, missing days, low-confidence duration, browser coverage, and a plain-language reliability label.

Do not use a hero, decorative illustration, nested cards, or oversized headings.

- [ ] **Step 5: Add keyboard and responsive behavior**

All range controls and breakdown rows are buttons or native inputs. At 980px the two analysis panels stack; at 599px KPI cells become two columns and the daily chart gains an internal horizontal scroller without page overflow.

- [ ] **Step 6: Run tests and build**

Run: `pnpm test -- --run src/lib/trend-range.test.ts src/components/trends/TrendsView.test.tsx`

Expected: PASS.

Run: `pnpm run build`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/components/trends src/lib/trend-range.ts src/lib/trend-range.test.ts src/App.tsx src/styles.css
git commit -m "feat: build trend range workbench"
```

### Task 3: Deterministic Statistical Evaluation

**Files:**
- Create: `src/lib/trend-analysis.ts`
- Create: `src/lib/trend-analysis.test.ts`
- Create: `src/components/trends/TrendEvaluationPanel.tsx`
- Modify: `src/components/trends/TrendsView.tsx`

**Interfaces:**
- Consumes: `TrendPayload`.
- Produces: `TrendLocalAnalysis` and `buildLocalTrendAnalysis(payload)`.

- [ ] **Step 1: Write failing statistical-analysis tests**

Cover insufficient data (`< 3` recorded days), stable/increasing/decreasing learning, median active duration, coefficient of variation, focus consistency, switching pressure, classification quality, and statements that never mention mood/personality/ability.

```ts
const result = buildLocalTrendAnalysis(payload);
expect(result.observations[0]).toContain("7 个有记录日");
expect(result.statistics.learningDeltaPercent).toBe(12.5);
expect(result.suggestions.every((item) => !/心情|性格|能力/.test(item))).toBe(true);
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `pnpm test -- --run src/lib/trend-analysis.test.ts`

Expected: FAIL because `buildLocalTrendAnalysis` does not exist.

- [ ] **Step 3: Implement bounded statistics**

Calculate mean, median, sample standard deviation, coefficient of variation, best day, weakest recorded day, learning delta, active delta, and coverage. Label a metric as increasing/decreasing only when the absolute delta is at least 8%; otherwise label it stable. When fewer than three days contain activity, return an explicit insufficient-evidence status instead of a directional judgment.

```ts
export interface TrendLocalAnalysis {
  status: "ready" | "insufficient";
  observations: string[];
  suggestions: string[];
  statistics: {
    activeMedianSeconds: number;
    learningMedianSeconds: number;
    activeCoefficientOfVariation: number | null;
    learningDeltaPercent: number | null;
    recordedDayCount: number;
  };
}
```

- [ ] **Step 4: Render evidence before advice**

The panel contains `观察到的变化`, `统计依据`, and `下阶段建议`. Each observation references a displayed statistic. Suggestions are operational experiments, not verdicts about the user.

- [ ] **Step 5: Run tests and build**

Run: `pnpm test -- --run src/lib/trend-analysis.test.ts src/components/trends/TrendsView.test.tsx`

Expected: PASS.

Run: `pnpm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/lib/trend-analysis.ts src/lib/trend-analysis.test.ts src/components/trends/TrendEvaluationPanel.tsx src/components/trends/TrendsView.tsx
git commit -m "feat: add evidence-based trend evaluation"
```

### Task 4: Offline-First AI Backfill and Trend Markdown Export

**Files:**
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/ai.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src/lib/desktop.ts`
- Modify: `src/components/trends/TrendEvaluationPanel.tsx`
- Modify: `src/components/trends/TrendsView.tsx`
- Modify: `docs/ARCHITECTURE.md`
- Test: `src-tauri/tests/database.rs`
- Test: `src-tauri/tests/app_service.rs`
- Test: `src/components/trends/TrendsView.test.tsx`

**Interfaces:**
- Consumes: `TrendPayload.evidenceHash`, existing AI provider selection, `ai_jobs`, Credential Manager provider keys, and Markdown export conventions.
- Produces: `TrendAnalysisResult`, Tauri commands `get_trend_analysis`, `queue_trend_analysis`, and `export_trend_markdown`.

- [ ] **Step 1: Write failing persistence and stale-result tests**

Add `trend_analyses` migration tests, evidence-hash lookup tests, queue deduplication, strict JSON parsing, and a stale-result scenario where an older completion cannot replace the newest hash.

```rust
assert_eq!(service.queue_trend_analysis(&evidence)?, first_job_id);
assert_eq!(service.queue_trend_analysis(&evidence)?, first_job_id);
assert_ne!(newer_evidence.evidence_hash, evidence.evidence_hash);
assert_eq!(service.get_trend_analysis(&newer_evidence)?.evidence_hash, newer_evidence.evidence_hash);
```

- [ ] **Step 2: Run focused Rust tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml trend_analysis -- --nocapture`

Expected: FAIL because trend analysis persistence and commands do not exist.

- [ ] **Step 3: Add evidence persistence and AI queueing**

Persist range dates, evidence hash, source (`local` or provider id), generated time, observations, suggestions, and model version. Reuse provider health checks and exponential retry. If no provider is configured or the machine is offline, return deterministic local analysis immediately and queue one deduplicated AI job.

- [ ] **Step 4: Enforce a strict AI response contract**

Require JSON with exactly `summary`, `observations`, `suggestions`, and `confidence`. Reject prose wrappers, unknown fields, missing fields, unsupported numerical claims, and any statement about mood, personality, health, or ability. AI prompt data is limited to `TrendEvidence`; it excludes raw local files and complete browser bodies.

- [ ] **Step 5: Add Markdown export**

Export Obsidian-compatible UTF-8 Markdown with YAML properties (`type`, `range_start`, `range_end`, `generated_at`, `evidence_hash`, `confidence`), callouts for summary/quality, KPI and daily tables, category/app changes, observations, and suggestions. Local export works without AI.

- [ ] **Step 6: Wire the UI**

Show `本地统计` or provider/model provenance, evidence hash prefix, confidence, and a `重新分析` icon button. Ignore any response whose evidence hash differs from the currently displayed payload. Add `导出 Markdown` beside the range controls.

- [ ] **Step 7: Run complete verification**

Run: `pnpm test`

Expected: all frontend tests PASS.

Run: `pnpm run build`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests PASS.

Run: `pnpm run tauri dev`

Verify presets, custom range, comparison deltas, offline local evaluation, queued AI provenance, stale-result guard, and Markdown export in the native window.

- [ ] **Step 8: Document and commit**

Document the authoritative range contract, equal-length comparison rule, statistical thresholds, trend evidence hash, AI privacy boundary, and offline fallback.

```bash
git add src src-tauri docs/ARCHITECTURE.md
git commit -m "feat: add offline-first trend intelligence"
```

## Self-Review

- Spec coverage: weekly, monthly, custom range, comparison, evidence quality, statistical analysis, AI evaluation, offline fallback, and Markdown export each map to a task.
- Placeholder scan: no deferred implementation markers or unspecified error-handling steps remain.
- Type consistency: `TrendPayload` and `evidenceHash` originate in Task 1 and are consumed unchanged by Tasks 2-4; AI results key off the same SHA-256 hash.
