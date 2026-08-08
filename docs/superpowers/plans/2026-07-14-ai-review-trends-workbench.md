# AI 双通道、审核中心与趋势工作台实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不恢复已回退私人后端方案的前提下，为每日任务监测系统交付可审计的 API Key / 本机 Codex 双通道、统一 AI 审核中心，以及支持自定义区间、多基准柱状比较、原始下钻和证据化研究分析的趋势工作台。

**Architecture:** SQLite 保存 AI 入队快照、执行审计、审核状态和人工所有权；Rust 负责最小化 AI 输入、通道内执行、85% 阈值、趋势事实聚合、证据校验和兼容导出；React 只消费权威 DTO，提供设置、审核和趋势交互。现有 `desktop.rs` 中的 Codex 半成品迁移到独立执行器，现有趋势聚合保持兼容并由新工作台接口扩展。

**Tech Stack:** Tauri 2、Rust 2024、SQLite/rusqlite、reqwest、Windows Credential Manager、React 19、TypeScript、ECharts 6、Vitest、Lucide React、现有 CSS 设计系统。

## Global Constraints

- 本计划接续当前脏工作区中的 14 个候选改动；先读 diff、补测试、再按任务提交，禁止丢弃用户改动。
- `src-tauri/Cargo.toml` 当前仅有一处来源未确认的 `tauri-build features = []` 变更；本功能不依赖它，各任务不得把该文件加入提交，除非执行者先证明其必要性并单独说明。
- 不读取或恢复 `backup/private-codex-upgrade-20260714` 和提交 `3f13d22` 中的代码。
- 不删除或重置用户数据库、API Key、历史记录、任务账本和快捷方式。
- Rust 产物继续写入 `.cargo/config.toml` 指向的 `C:/Users/26925/AppData/Local/DailyTaskMonitor/build-cache`；不得在项目目录生成 `target`。
- 开发构建不得覆盖 `C:/Users/26925/AppData/Local/DailyTaskMonitor/app/DailyTaskMonitor.exe`；仅在所有测试和人工验收通过后产出候选安装包。
- AI 任务入队时固定执行通道；全局设置切换只影响新任务，失败时不得跨通道回退。
- API 请求不包含完整活动时间线、无关窗口标题或无关浏览记录；诊断信息不包含 API Key、完整提示词或敏感标题。
- 自动归类和工作流归属统一使用 `confidence >= 0.85` 自动写入；`0.849999` 必须进入复核。
- 人工修改优先于 AI；撤销 AI 自动写入后建立人工所有权，后续 AI 不得再次自动覆盖。
- 趋势事实由 Rust 计算；React 不重新汇总权威总量。自定义区间为 1 至 366 个本地日历日。
- 趋势 AI 只读，不创建任务、不修改目标，不推断人格、情绪、疾病、能力或心理状态。
- 旧 `get_trends`、旧趋势 DTO 和 Markdown 导出在迁移期间保持可用；新字段采用 serde/TypeScript 向后兼容默认值。
- 每个任务完成后只暂存列出的文件；提交前运行 `git diff --check` 并确认没有意外包含用户改动。

---

### Task 1: 固化设置迁移与现有半成品基线

**Files:**
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/tests/app_service.rs`
- Modify: `src/App.tsx`
- Modify: `src/lib/desktop.ts`
- Modify: `src/lib/desktop.test.ts`
- Modify: `src/App.test.tsx`

**Interfaces:**
- Produces: `AppSettings.ai_execution_mode`, `codex_executable`, `codex_model`, `ai_auto_trend_analysis_enabled`, `ai_auto_classification_enabled`, `ai_auto_workflow_assignment_enabled`, `ai_automation_notice_version`.
- Produces: `SettingsPatch` 中对应的可选字段，以及 TypeScript `AppSettings` 的精确镜像。
- Migration rule: 缺失三项自动化字段的旧设置按 `true` 读取；已有显式 `false` 不得被覆盖；通知版本缺失按 `0` 读取。

- [ ] **Step 1: 为升级默认值和人工关闭写失败测试**

在 `app_service.rs` 写三个场景：旧 JSON 缺少新字段时三个开关为开；用户保存 `false` 后重启仍为关；未知/缺失 Codex 路径回落到 `codex`。在 `desktop.test.ts` 断言 camelCase 映射和 patch 参数。

```rust
assert!(settings.ai_auto_trend_analysis_enabled);
assert!(settings.ai_auto_classification_enabled);
assert!(settings.ai_auto_workflow_assignment_enabled);
assert_eq!(settings.ai_automation_notice_version, 0);
assert_eq!(settings.codex_executable, "codex");
```

- [ ] **Step 2: 运行测试并确认失败原因**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test app_service settings_ -j 1 -- --nocapture`

Expected: FAIL，当前三个自动化字段默认仍为 `false`，且没有路径与通知版本字段。

Run: `pnpm test -- src/lib/desktop.test.ts src/App.test.tsx`

Expected: FAIL，TypeScript 契约和首次说明弹窗尚未完成。

- [ ] **Step 3: 实现向后兼容设置结构**

为三个自动化字段使用 `#[serde(default = "default_true")]`，`Default` 也设为 `true`。增加 `codex_executable: String`、`codex_model: String` 和 `ai_automation_notice_version: u32`；路径只保存可执行文件位置，不保存命令参数。`codex_model` 为空时不向 CLI 传 `--model`，入队快照标记为 `cli-default`。保持 `ai_backfill_enabled` 旧字段兼容，但不再把它作为三个开关的总开关。

- [ ] **Step 4: 实现首次说明弹窗**

在 `App.tsx` 增加一次性模态框，显示当前执行方式、最小化数据边界、资源消耗、失败不回退和三个开关。关闭时调用 `update_settings({ aiAutomationNoticeVersion: 1 })`。使用 `Dialog` 语义、明确关闭按钮和焦点返回；桌面预览模式不写原生设置。

- [ ] **Step 5: 通过聚焦测试并建立基线提交**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test app_service settings_ -j 1 -- --nocapture`

Run: `pnpm test -- src/lib/desktop.test.ts src/components/App.test.tsx`

Expected: PASS。

```bash
git add src-tauri/src/app.rs src-tauri/tests/app_service.rs src/App.tsx src/App.test.tsx src/lib/desktop.ts src/lib/desktop.test.ts
git commit -m "feat: persist AI execution and automation settings"
```

### Task 2: 在 AI 队列中固定执行方式并记录审计元数据

**Files:**
- Modify: `src-tauri/src/ai.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/work_ledger/repository.rs`
- Modify: `src-tauri/src/work_ledger/service.rs`
- Test: `src-tauri/tests/database.rs`
- Test: `src-tauri/tests/ai_queue.rs`
- Test: `src-tauri/tests/work_ledger_service.rs`

**Interfaces:**
- Produces:

```rust
pub struct AiExecutionSnapshot {
    pub execution_mode: AiExecutionMode,
    pub executor_id: String,
    pub model: String,
    pub evidence_hash: String,
    pub created_at_ms: i64,
}

pub struct AiJob {
    // existing fields
    pub execution: AiExecutionSnapshot,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub error_kind: Option<AiExecutionErrorKind>,
}
```

- Database migration: add nullable/backfilled columns to `ai_jobs`; legacy rows use `api-key`, executor `legacy-provider-registry`, empty model/evidence hash, and original queue timing where available.
- Queue identity: content hash includes job kind, payload, generation and execution snapshot so “改用当前方式重新执行” creates a distinct generation without mutating the old job.

- [ ] **Step 1: 写迁移、锁定和不回退测试**

覆盖旧表迁移、API 入队后切换 Codex 仍由 API 领取、Codex 入队后切换 API 仍由 Codex 领取、重试保留相同快照、显式重跑创建新 generation。

- [ ] **Step 2: 运行队列测试并确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_queue -j 1 -- --nocapture`

Expected: FAIL，`AiJob` 没有执行快照，worker 仍会在处理时读取全局模式。

- [ ] **Step 3: 扩展数据库和队列 API**

将 `enqueue_ai_job_hashed`、`enqueue_ai_job_for_subject`、`force_enqueue_ai_job_for_subject` 改为接收 `&AiExecutionSnapshot`。所有调用者在入队前从设置与 Provider Registry/Codex 配置生成快照；领取和重试只读取 job 快照，不再读取当前全局模式。

- [ ] **Step 4: 保存执行审计**

`mark_ai_job_running` 写入开始时间；成功/失败写入结束时间、耗时、来源、模型、退出码和规范化错误类型。错误正文截断到 1,000 字符并经过凭据/提示词清洗。

- [ ] **Step 5: 运行数据库、队列和工作流回归测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test database --test ai_queue --test work_ledger_service -j 1 -- --nocapture`

Expected: PASS，已有去重、指数退避、stale generation 和人工所有权测试保持通过。

```bash
git add src-tauri/src/ai.rs src-tauri/src/db.rs src-tauri/src/app.rs src-tauri/src/work_ledger/repository.rs src-tauri/src/work_ledger/service.rs src-tauri/tests/database.rs src-tauri/tests/ai_queue.rs src-tauri/tests/work_ledger_service.rs
git commit -m "feat: snapshot AI execution mode in queued jobs"
```

### Task 3: 抽取 API / Codex 统一执行器与健康检查

**Files:**
- Create: `src-tauri/src/ai_executor.rs`
- Create: `src-tauri/tests/ai_executor.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src/lib/desktop.ts`
- Modify: `src/App.tsx`
- Test: `src/lib/desktop.test.ts`

**Interfaces:**
- Produces:

```rust
pub struct AiExecutionRequest {
    pub job_id: String,
    pub kind: String,
    pub snapshot: AiExecutionSnapshot,
    pub system_prompt: String,
    pub minimal_payload_json: String,
    pub timeout_ms: u64,
}

pub struct AiExecutionOutput {
    pub content: String,
    pub executor_id: String,
    pub model: String,
    pub duration_ms: i64,
    pub exit_code: Option<i32>,
}

pub enum AiExecutionErrorKind {
    NotConfigured,
    CliUnavailable,
    PermissionDenied,
    Authentication,
    Network,
    Timeout,
    InvalidResponse,
    EvidenceMismatch,
    StaleEvidence,
}

pub struct CodexHealth {
    pub configured_path: String,
    pub detected_path: Option<String>,
    pub version: Option<String>,
    pub checked_at_ms: i64,
    pub status: CodexHealthStatus,
    pub diagnostic: String,
}
```

- Tauri commands: `get_codex_health() -> CodexHealth`, `test_codex_cli() -> CodexHealth`, `update_settings({ codexExecutable, codexModel })`.
- API executor continues to use Provider Registry priority within API mode; this is provider failover, not cross-mode fallback.

- [ ] **Step 1: 为命令构造、超时、清洗和无回退写失败测试**

使用临时假 CLI 脚本模拟版本、成功 JSON、非零退出、超时和 stderr 中的敏感文本。API 测试用本地 loopback server 验证仅发送 `minimal_payload_json`。

- [ ] **Step 2: 运行执行器测试并确认模块不存在**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_executor -j 1 -- --nocapture`

Expected: FAIL，`ai_executor` 模块尚不存在。

- [ ] **Step 3: 把 `desktop.rs` 的执行细节迁入执行器**

移动 `run_codex_command`、Codex 请求构造、进程等待/超时、API chat 请求和错误归一化。`desktop.rs::process_one_ai_job` 只负责领取、调用、校验、持久化。Codex 使用配置路径直接执行，参数固定，stdin 传最小 payload，stdout 只接受一个 JSON 对象；配置模型时传 `--model`，未配置时审计为 `cli-default`，若结构化输出报告实际模型则以实际值覆盖执行结果中的模型标识。

- [ ] **Step 4: 实现健康检查 UI**

设置抽屉用 `API Key | 本机 Codex` 分段控件。Codex 行显示配置路径、可选模型、检测路径、版本、检测时间和图标+文字状态；提供路径输入、模型输入和刷新图标按钮。API provider 行保留模型、优先级和测试按钮。

- [ ] **Step 5: 验证两种模式和错误分类**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_executor --test ai_queue -j 1 -- --nocapture`

Run: `pnpm test -- src/lib/desktop.test.ts src/App.test.tsx`

Expected: PASS；测试明确断言 Codex 失败没有调用 API mock，API 失败没有启动 Codex mock。

```bash
git add src-tauri/src/ai_executor.rs src-tauri/tests/ai_executor.rs src-tauri/src/lib.rs src-tauri/src/desktop.rs src-tauri/src/app.rs src/lib/desktop.ts src/lib/desktop.test.ts src/App.tsx src/App.test.tsx
git commit -m "feat: add audited API and local Codex executors"
```

### Task 4: 统一自动化入口、手动触发和 85% 应用规则

**Files:**
- Modify: `src-tauri/src/classifier.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/work_ledger/commands.rs`
- Modify: `src-tauri/src/work_ledger/service.rs`
- Modify: `src-tauri/tests/classification.rs`
- Modify: `src-tauri/tests/work_ledger_service.rs`
- Modify: `src/components/workflow/WorkflowPage.tsx`
- Modify: `src/components/workflow/WorkflowPage.test.tsx`

**Interfaces:**
- Background gates: trend queue uses only `ai_auto_trend_analysis_enabled`; segment classification uses only `ai_auto_classification_enabled`; workflow suggestion queue uses only `ai_auto_workflow_assignment_enabled`.
- Manual commands: `queue_segment_classification(segment_id)`, `queue_workflow_ai_suggestions(start_ms, end_ms)`, `queue_trend_analysis(..., force=true)` ignore background gates but still use the current execution mode snapshot.
- Produces: shared `const AI_AUTO_APPLY_THRESHOLD: f64 = 0.85`.

- [ ] **Step 1: 写三开关独立性、手动可用性和阈值边界测试**

测试八种开关组合中的关键边界；特别断言 `0.85` 自动应用、`0.849999` 保留建议、人工字段不被覆盖。

- [ ] **Step 2: 运行聚焦测试并确认现有行为不完整**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test classification --test work_ledger_service -j 1 -- --nocapture`

Expected: FAIL，当前分类与工作流尚未共享完整阈值/人工锁定路径。

- [ ] **Step 3: 实现统一决策函数**

```rust
pub enum AiDisposition { AutoApply, Review, ManualLock }

pub fn decide_ai_disposition(confidence: f64, manually_owned: bool) -> AiDisposition {
    if manually_owned { AiDisposition::ManualLock }
    else if confidence >= AI_AUTO_APPLY_THRESHOLD { AiDisposition::AutoApply }
    else { AiDisposition::Review }
}
```

归类和工作流消费结果前都调用该函数；stale evidence 先于阈值判断。手动请求按钮在自动开关关闭时仍可用，并显示排队数量与当前通道。

- [ ] **Step 4: 回归验证**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test classification --test work_ledger_service -j 1 -- --nocapture`

Run: `pnpm test -- src/components/workflow/WorkflowPage.test.tsx`

Expected: PASS。

```bash
git add src-tauri/src/classifier.rs src-tauri/src/db.rs src-tauri/src/desktop.rs src-tauri/src/work_ledger/commands.rs src-tauri/src/work_ledger/service.rs src-tauri/tests/classification.rs src-tauri/tests/work_ledger_service.rs src/components/workflow/WorkflowPage.tsx src/components/workflow/WorkflowPage.test.tsx
git commit -m "feat: enforce AI automation and review policy"
```

### Task 5: 建立统一 AI 审核与人工所有权数据模型

**Files:**
- Create: `src-tauri/src/ai_review.rs`
- Create: `src-tauri/tests/ai_review.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/work_ledger/service.rs`
- Modify: `src-tauri/src/domain.rs`

**Interfaces:**
- Produces:

```rust
pub enum AiReviewKind { Classification, WorkflowAssignment }
pub enum AiReviewState { Pending, AutoApplied, ManualOverride, ExecutionError, Dismissed, Reverted }

pub struct AiReviewRecord {
    pub id: String,
    pub kind: AiReviewKind,
    pub state: AiReviewState,
    pub subject_id: String,
    pub before_json: String,
    pub proposed_json: String,
    pub applied_json: Option<String>,
    pub confidence: Option<f64>,
    pub evidence_summary: String,
    pub evidence_hash: String,
    pub execution: AiExecutionAuditView,
    pub created_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
}
```

- Database tables: `ai_review_records`, `ai_review_events`, `manual_field_ownership`; immutable events cover generated, auto-applied, accepted, changed, ignored, reverted, retried and failed.
- Tauri commands: `list_ai_reviews(filter)`, `resolve_ai_review(request)`, `revert_ai_auto_apply(review_id)`, `retry_ai_review(review_id, use_current_mode)`.

- [ ] **Step 1: 写状态机和审计历史失败测试**

覆盖低置信待复核、接受、改选、忽略、高置信自动应用、撤销恢复 before 值并建立人工所有权、错误重试保留旧记录、非法状态转换拒绝。

- [ ] **Step 2: 运行新测试并确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_review -j 1 -- --nocapture`

Expected: FAIL，统一审核模型尚不存在。

- [ ] **Step 3: 实现事务化状态机**

每个 resolve/revert 操作在一个 SQLite transaction 中完成正式数据写入、review 状态变化和 event 追加。批量接受仅允许同一 `AiReviewKind`，提交前由后端重新验证 evidence hash 和人工所有权。

- [ ] **Step 4: 接入分类与工作流消费路径**

AI 结果先产生 review record，再根据 disposition 自动应用或等待复核。执行失败写 `ExecutionError`，但不创建虚假建议。人工直接修改正式数据时追加 `ManualOverride` 记录。

- [ ] **Step 5: 运行审核与账本回归**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test ai_review --test work_ledger_service --test database -j 1 -- --nocapture`

Expected: PASS。

```bash
git add src-tauri/src/ai_review.rs src-tauri/tests/ai_review.rs src-tauri/src/lib.rs src-tauri/src/db.rs src-tauri/src/desktop.rs src-tauri/src/work_ledger/service.rs src-tauri/src/domain.rs
git commit -m "feat: persist AI review and manual ownership history"
```

### Task 6: 实现 AI 审核中心页面

**Files:**
- Create: `src/components/ai-review/AiReviewPage.tsx`
- Create: `src/components/ai-review/AiReviewPage.test.tsx`
- Create: `src/components/ai-review/AiReviewTable.tsx`
- Create: `src/components/ai-review/AiReviewInspector.tsx`
- Create: `src/lib/ai-review.ts`
- Create: `src/lib/ai-review.test.ts`
- Modify: `src/lib/desktop.ts`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Navigation: `type Tab = "today" | "trends" | "workflow" | "ai-review"`.
- Tabs: `pending`, `auto-applied`, `manual-override`, `execution-error`.
- Filters: kind, execution mode, executor/provider, model, date range and confidence range.
- Commands: accept, choose alternate, ignore, batch accept same kind, revert auto-apply, retry same mode, retry current mode.

- [ ] **Step 1: 写筛选、状态和操作测试**

使用 linkedom 渲染四个页签，断言键盘可达、筛选组合、详情证据、错误诊断、批量异类拒绝、撤销确认和 retry 两种语义。

- [ ] **Step 2: 运行前端测试并确认失败**

Run: `pnpm test -- src/lib/ai-review.test.ts src/components/ai-review/AiReviewPage.test.tsx`

Expected: FAIL，页面和桥接不存在。

- [ ] **Step 3: 实现高密度审核工作台**

页面使用顶部页签+筛选条、左侧稳定宽度表格、右侧 inspector；不使用卡片嵌套。置信度、状态和来源同时用文字与图标表达。低置信记录展开后显示候选及分数；执行异常显示清洗后的诊断和重试按钮。

- [ ] **Step 4: 接入 Today/Workflow 跳转**

Today 时间线和 Workflow 只显示轻量状态标记；点击后打开 AI 审核页并带 `subjectId` 过滤。处理成功后刷新相关页面，但不改变用户当前日期。

- [ ] **Step 5: 验证页面和构建**

Run: `pnpm test -- src/lib/ai-review.test.ts src/components/ai-review/AiReviewPage.test.tsx src/components/workflow/WorkflowPage.test.tsx`

Run: `pnpm run build`

Expected: PASS，TypeScript 无错误。

```bash
git add src/components/ai-review/AiReviewPage.tsx src/components/ai-review/AiReviewPage.test.tsx src/components/ai-review/AiReviewTable.tsx src/components/ai-review/AiReviewInspector.tsx src/lib/ai-review.ts src/lib/ai-review.test.ts src/lib/desktop.ts src/App.tsx src/styles.css
git commit -m "feat: add central AI review workbench"
```

### Task 7: 扩展趋势事实契约、分桶和多基准比较

**Files:**
- Create: `src-tauri/src/trends.rs`
- Create: `src-tauri/tests/trends_workbench.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src/lib/desktop.ts`
- Modify: `src/lib/trend-range.ts`
- Modify: `src/lib/trend-range.test.ts`

**Interfaces:**
- Produces:

```rust
pub enum TrendGranularity { Day, Week, Month }
pub enum TrendMetric {
    MonitoredSeconds,
    ActiveSeconds,
    LearningSeconds,
    IdleSeconds,
    SwitchCount,
    LongestFocusSeconds,
    ClassificationCoverage,
    CompletedTaskCount,
    LinkedTaskSeconds,
}

pub struct TrendWorkbenchRequest {
    pub start_date: String,
    pub end_date: String,
    pub timezone_offset_minutes: i32,
    pub granularity: Option<TrendGranularity>,
    pub metric: TrendMetric,
    pub custom_baseline: Option<TrendDateRange>,
}

pub struct TrendBucket {
    pub id: String,
    pub start_date: String,
    pub end_date: String,
    pub values: TrendMetricValues,
    pub recorded_day_count: usize,
    pub missing_day_count: usize,
    pub evidence_ids: Vec<String>,
}

pub struct TrendBaselineSeries {
    pub kind: TrendBaselineKind,
    pub range: TrendDateRange,
    pub value: Option<f64>,
    pub absolute_delta: Option<f64>,
    pub percent_delta: Option<f64>,
}
```

- Tauri command: `get_trend_workbench(request) -> TrendWorkbenchPayload`.
- Compatibility: `get_trends` remains and can adapt the new day-granularity result into the legacy `TrendPayload`.
- Default granularity: 1-31 days => day, 32-120 => week, 121-366 => month; a user override remains active for the current React session.

- [ ] **Step 1: 写日期边界、分桶和基准失败测试**

覆盖闰日、跨年周、自然月、1/31 向前平移一个月、上一等长区间、上月同期、自定义第四基准、零基准、缺失日和 366 天上限。

- [ ] **Step 2: 运行新聚合测试并确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test trends_workbench -j 1 -- --nocapture`

Expected: FAIL，新请求和分桶类型不存在。

- [ ] **Step 3: 实现纯事实趋势模块**

周桶以用户本地周一为起点并在查询边界裁剪；月桶按本地自然月裁剪。上月同期把起止日期整体减一个日历月，不存在的日期截断至目标月最后一天，并保持起止顺序。百分比基准为零时返回 `None`，UI 不显示无穷大。

- [ ] **Step 4: 计算完整统计带**

返回总量、按选定粒度均值、日值中位数、最大值、样本标准差、变异系数、有效采样天数、缺失天数、分类覆盖率、低置信时长和待处理时长。统计字段附稳定 evidence ID，例如 `current.summary.activeSeconds`。

- [ ] **Step 5: 验证 Rust/TypeScript 契约和旧接口**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test trends_workbench --test app_service -j 1 -- --nocapture`

Run: `pnpm test -- src/lib/trend-range.test.ts src/lib/desktop.test.ts`

Expected: PASS，旧趋势和 Markdown 测试不变。

```bash
git add src-tauri/src/trends.rs src-tauri/tests/trends_workbench.rs src-tauri/src/lib.rs src-tauri/src/app.rs src-tauri/src/db.rs src-tauri/src/desktop.rs src/lib/desktop.ts src/lib/trend-range.ts src/lib/trend-range.test.ts
git commit -m "feat: aggregate multi-baseline trend workbench data"
```

### Task 8: 增加任务归集、原始下钻与双向选择

**Files:**
- Modify: `src-tauri/src/trends.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/work_ledger/repository.rs`
- Modify: `src-tauri/tests/trends_workbench.rs`
- Create: `src/lib/trend-workbench.ts`
- Create: `src/lib/trend-workbench.test.ts`
- Modify: `src/lib/desktop.ts`

**Interfaces:**
- `completedTaskCount`: 按 `tasks.completed_at_ms` 落入查询/桶区间。
- `linkedTaskSeconds`: 对 activity evidence 与桶区间做时间裁剪后按 task/project 汇总；focus evidence 使用实际 focus session 相交时长，禁止重复计时。
- Produces: `TrendBucketDrilldown`，包含原始活动行、应用/分类分布、完成任务、关联任务时长、工作流归属和数据完整度。
- Produces TS selection reducer: `selectBucket`, `selectRawRow`, `clearSelection`; bucket 与 raw row 始终指向同一 `bucketId`。

- [ ] **Step 1: 写任务归集和裁剪测试**

构造跨桶 activity evidence、一个完成任务、一个未完成任务、重复 evidence link 和 focus session；断言只计一次且各桶之和等于区间 rollup。

- [ ] **Step 2: 运行测试并确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test trends_workbench task_ -j 1 -- --nocapture`

Run: `pnpm test -- src/lib/trend-workbench.test.ts`

Expected: FAIL，linked task duration 和选择 reducer 尚不存在。

- [ ] **Step 3: 实现查询和下钻 DTO**

DB 查询只取请求区间相交记录；下钻标题摘要遵循现有隐私处理。原始表行含 `rowId`, `bucketId`, date/time, app, category, task/project, duration, confidence 和 review state，不把完整窗口标题用于 AI payload。

- [ ] **Step 4: 实现纯函数选择状态**

表格选择行时派生 bucket；图表选择 bucket 时过滤并高亮表格。范围或粒度改变时清空不再存在的 selection，避免旧请求回写新视图。

- [ ] **Step 5: 验证一致性**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test trends_workbench -j 1 -- --nocapture`

Run: `pnpm test -- src/lib/trend-workbench.test.ts`

Expected: PASS。

```bash
git add src-tauri/src/trends.rs src-tauri/src/db.rs src-tauri/src/work_ledger/repository.rs src-tauri/tests/trends_workbench.rs src/lib/trend-workbench.ts src/lib/trend-workbench.test.ts src/lib/desktop.ts
git commit -m "feat: add task-aware trend drilldown data"
```

### Task 9: 重构趋势页为完整数据工作台

**Files:**
- Create: `src/components/trends/TrendRangeToolbar.tsx`
- Create: `src/components/trends/TrendStatisticsStrip.tsx`
- Create: `src/components/trends/TrendComparisonChart.tsx`
- Create: `src/components/trends/TrendDrilldown.tsx`
- Create: `src/components/trends/TrendRawTable.tsx`
- Modify: `src/components/trends/TrendTimelineChart.tsx`
- Modify: `src/components/trends/TrendsView.tsx`
- Modify: `src/components/trends/TrendsView.test.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Toolbar: preset/custom range, day/week/month segmented control, one metric menu, custom baseline toggle and dates, previous/next/refresh icon buttons.
- Time chart: one selected metric across buckets; stable chart height and selected bucket.
- Comparison chart: current, previous equal period, previous-month same period and optional custom baseline as grouped bars.
- Layout order: toolbar -> statistics strip -> time bars -> comparison bars -> drilldown/distributions -> raw table -> AI analysis.

- [ ] **Step 1: 写完整工作台渲染和交互测试**

断言九个指标、自定义区间、自定义基准默认关闭、零基准空状态、点击柱子下钻、点击原始行反向选柱、日/周/月切换和 366 天错误。

- [ ] **Step 2: 运行趋势组件测试并确认失败**

Run: `pnpm test -- src/components/trends/TrendsView.test.tsx src/lib/trend-workbench.test.ts`

Expected: FAIL，当前页面只有单一上期比较和日表。

- [ ] **Step 3: 实现稳定、紧凑的工作台 UI**

使用 ECharts grouped bar，不新增图表库。统计带使用紧凑行式布局，不做大型卡片墙；下钻和原始表为无嵌套面板。图表容器设稳定高度和最小宽度，长标签换行或截断并提供 tooltip。

- [ ] **Step 4: 实现可访问交互与空状态**

所有图例、指标、粒度和开关可键盘操作；颜色之外同时使用标签/图案区分基准。缺失数据、零基准、无任务、无分类和加载失败分别显示明确原因。

- [ ] **Step 5: 验证响应式布局与构建**

Run: `pnpm test -- src/components/trends/TrendsView.test.tsx`

Run: `pnpm run build`

Expected: PASS。随后在 `1440x900`, `1024x768`, `390x844` 三个视口人工检查：无重叠、表格可滚动、按钮文字不溢出、选中状态一致。

```bash
git add src/components/trends/TrendRangeToolbar.tsx src/components/trends/TrendStatisticsStrip.tsx src/components/trends/TrendComparisonChart.tsx src/components/trends/TrendDrilldown.tsx src/components/trends/TrendRawTable.tsx src/components/trends/TrendTimelineChart.tsx src/components/trends/TrendsView.tsx src/components/trends/TrendsView.test.tsx src/styles.css
git commit -m "feat: build the trend data workbench UI"
```

### Task 10: 实现证据校验的研究型趋势分析

**Files:**
- Create: `src-tauri/src/trend_analysis.rs`
- Create: `src-tauri/tests/trend_analysis.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/ai.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src/lib/desktop.ts`
- Modify: `src/lib/trend-analysis.ts`
- Modify: `src/lib/trend-analysis.test.ts`
- Create: `src/components/trends/TrendResearchAnalysis.tsx`
- Modify: `src/components/trends/TrendsView.tsx`
- Modify: `src/components/trends/TrendsView.test.tsx`

**Interfaces:**
- Produces:

```rust
pub struct TrendResearchFinding {
    pub observation: String,
    pub possible_explanation: String,
    pub validation_method: String,
    pub evidence_ids: Vec<String>,
    pub claims: Vec<TrendEvidenceClaim>,
    pub confidence: f64,
    pub limitations: Vec<String>,
}

pub struct TrendEvidenceClaim {
    pub evidence_id: String,
    pub relation: EvidenceRelation,
}

pub struct TrendResearchAnalysis {
    pub status: ResearchStatus,
    pub findings: Vec<TrendResearchFinding>,
    pub limitations: Vec<String>,
    pub source: String,
    pub model: String,
    pub evidence_hash: String,
}
```

- Evidence IDs resolve only against the selected range, enabled baselines and data-quality map returned by `TrendWorkbenchPayload`.
- AI free text may not contain raw numeric literals; all displayed numbers come from validated evidence chips. Direction words declared by `claims` are accepted only when they match the evidence sign/tolerance.
- Insufficient policy: fewer than 3 effective activity days returns `limitations_only`; missing baseline suppresses only comparative claims; classification coverage below 0.5 prohibits category-causal explanations.

- [ ] **Step 1: 写最小化输入和严格校验失败测试**

覆盖合法 evidence ID、未知 ID、伪造数值、错误方向、过期 hash、未启用基准、部分 finding 无效、全部 finding 无效、样本不足和心理推断禁词。

- [ ] **Step 2: 运行研究分析测试并确认旧 candidate parser 不满足需求**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test trend_analysis -j 1 -- --nocapture`

Expected: FAIL，当前 `parse_trend_analysis_response` 只允许预定义句子，不能表达证据化自由文本。

- [ ] **Step 3: 实现 evidence map、最小 payload 和 validator**

API/Codex 输入只包含聚合指标、已启用基准、质量字段和 evidence ID，不包含原始时间线。validator 逐 finding 校验 hash、ID、关系、置信度、禁用推断和数字字面量；部分有效时只保存通过项，全部无效时把 job 标记为 `InvalidResponse`。

- [ ] **Step 4: 保留确定性本地分析作为非 AI 事实层**

`buildLocalTrendAnalysis` 继续提供均值、中位数、标准差和变异系数，但改为消费新统计 DTO，不冒充 AI。AI 不可用或样本不足时，页面仍展示事实统计和限制说明。

- [ ] **Step 5: 实现“观察 -> 可能解释 -> 验证方法”UI**

每条 finding 显示证据 chip、来源、模型、置信度和限制；“可能解释”明确标为假设。该区域只读，不出现“转为任务”或修改目标按钮。

- [ ] **Step 6: 验证后端、前端和隐私边界**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test trend_analysis --test ai_executor -j 1 -- --nocapture`

Run: `pnpm test -- src/lib/trend-analysis.test.ts src/components/trends/TrendsView.test.tsx`

Expected: PASS；mock server 收到的 JSON 不含 `timeline`, `windowTitle`, `browserVisits` 或原始活动标题。

```bash
git add src-tauri/src/trend_analysis.rs src-tauri/tests/trend_analysis.rs src-tauri/src/lib.rs src-tauri/src/ai.rs src-tauri/src/app.rs src-tauri/src/desktop.rs src-tauri/src/db.rs src/lib/desktop.ts src/lib/trend-analysis.ts src/lib/trend-analysis.test.ts src/components/trends/TrendResearchAnalysis.tsx src/components/trends/TrendsView.tsx src/components/trends/TrendsView.test.tsx
git commit -m "feat: validate evidence-backed trend research analysis"
```

### Task 11: 全链路回归、桌面验收和候选构建

**Files:**
- Modify as required by failures only: files touched in Tasks 1-10
- Modify: `README.md`
- Create: `docs/ai-dual-channel-and-trends.md`
- Test: all Rust and frontend test suites

**Acceptance Matrix:**
- API and Codex each complete classification, workflow and trend jobs with correct source/model/duration audit.
- Switching mode after enqueue does not change existing jobs; retry-same and retry-current have distinct behavior.
- Three automation toggles are independent; manual triggers always work.
- `0.85` auto-applies, `0.849999` reviews, manual override blocks AI, revert creates manual ownership.
- Any valid 1-366 day range supports day/week/month buckets, three default baselines and optional custom baseline.
- Task completed count and linked duration match drilldown/raw rows.
- Every AI finding has valid evidence; insufficient data yields limitations only.

- [ ] **Step 1: 运行格式化和静态检查**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `pnpm run build`

Expected: PASS。

- [ ] **Step 2: 运行全部自动化测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib --tests -j 1 -- --nocapture`

Run: `pnpm test`

Expected: PASS。若 Windows linker 报 `LNK1104`，等待占用释放后以相同命令重试；不得改回 D 盘 target，也不得删除稳定 EXE。

- [ ] **Step 3: 启动开发版并执行人工验收**

Run: `pnpm tauri dev`

在真实桌面窗口验证设置健康检查、首次说明、审核四页签、手动触发、失败重试、趋势自定义区间、九指标、分桶、四基准、下钻、原始表和 AI 证据。确认窄窗口无内容重叠。

- [ ] **Step 4: 验证数据兼容和导出**

复制一份用户数据库到临时目录进行升级测试；确认原数据库、凭据和历史不变。验证旧 Markdown 导出仍可生成，并增加粒度、比较基准、任务统计、数据质量和 evidence ID 段落。

- [ ] **Step 5: 构建候选包但不替换稳定版**

Run: `pnpm tauri build`

Expected: 在 AppData build cache 产出可安装/可执行候选；记录路径、文件大小和版本。先由用户验收候选，再另行决定是否更新 `DailyTaskMonitor.exe` 和桌面快捷方式。

- [ ] **Step 6: 完成文档和最终提交**

文档说明两种通道、无自动回退、最小化数据、85% 规则、审核状态、趋势口径、Codex 路径配置和错误排查。

```bash
git add README.md docs/ai-dual-channel-and-trends.md
git commit -m "docs: document AI review and trend workbench"
```

## Completion Gate

- [ ] `git status --short` 只包含执行前已存在且明确不属于本计划的改动；不得遗留本计划生成文件。
- [ ] `git diff --check` 无空白错误。
- [ ] `rg -n "T[B]D|T[O]DO|FIX[M]E|PLACE[H]OLDER|待[定]|稍后实[现]" docs/superpowers/plans/2026-07-14-ai-review-trends-workbench.md` 无结果。
- [ ] Rust 全测试、Vitest 全测试、TypeScript/Vite build 和 Tauri candidate build 均有成功记录。
- [ ] 真实桌面人工验收覆盖 API、Codex、审核、趋势、导出和升级兼容。
- [ ] 稳定 EXE、用户数据库和凭据未被覆盖或删除。
