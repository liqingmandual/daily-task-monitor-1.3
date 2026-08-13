# Architecture

## 运行结构

- `src/`：React/TypeScript 桌面界面与 Tauri IPC 桥。
- `src-tauri/src/monitor.rs`：5 秒采样合并、空闲回填和连续片段状态机。
- `src-tauri/src/windows_collector.rs`：Windows 前台窗口、聚合输入与媒体会话。
- `src-tauri/src/browser.rs`：`BrowserAdapter`、Chromium History 副本扫描和 HTML 摘要。
- `src-tauri/src/classifier.rs`：本地规则与行为分类。
- `src-tauri/src/ai.rs`：提供商与离线任务模型。
- `src-tauri/src/db.rs`：SQLite migration 与持久化。
- `src-tauri/src/segment_overlap.rs`：跨平台权威时间线与重叠片段归一化。
- `src-tauri/src/desktop.rs`：Tauri 命令、托盘、后台 worker 与凭据库边界。

## 分类优先级

手动规则 > 空闲 > 高确定性应用/域名 > 媒体与视频 > 行为模型 > AI > 待分类。

正式原子分类为：空闲、搜索/调研、视频信息输入、文字信息输入、游戏、社交软件、创作开发、文件整理。总活跃时长是汇总指标。

## 采集权威与数据完整性

同一 SQLite 数据库允许多个桌面界面实例读取，但只允许持有
`collector_lease` 的实例执行采样。租约跟随数据库而不是操作系统进程锁，
因此 Windows 和 macOS 使用同一套单采集器约束；失去续租后，其他实例可在
租约过期时接管。

原始活动记录不会因为历史重叠而被删除。Today 与趋势统计会先生成
确定性的权威时间线：Active 优先于 Idle，同类冲突按起止时间和 ID 决胜，
没有记录的间隔保持为空。完整口径、日期切换规则和回归约束见
`docs/DATA_INTEGRITY.md`。

## 离线与补算

采集、规则分类、SQLite 聚合、目标和报告不依赖网络。需要 AI 的工作写入 `ai_jobs`，按内容哈希去重。网络恢复后仅使用用户已经配置并通过健康检查的接口。

## Today 分析工作台

Today 页按固定阅读顺序组合：页面标题、核心指标、任务分布/应用排行/时间分布、活动时间线、目标与产出、AI 分析。趋势和工作流保留各自独立的视图与数据加载路径。

### 时间线筛选与定位

`src/lib/timeline-filter.ts` 定义共享的 `TimelineFilter` 联合类型。指标卡、两个分布面板和两小时柱状图都只产生这个筛选值；`TimelinePanel` 以相同的筛选逻辑显示记录。触发钻取时，`App.tsx` 记录一次滚动请求，等时间线与滚动容器都已挂载后，以固定页头高度加 24px 留白定位到时间线标题。

### 应用身份缓存

前端以 `appIdentityKey(app, executablePath)` 作为稳定键。`resolveAppIdentities` 只把缓存未命中的应用提交给 Tauri 的 `resolve_app_identities` 命令，保存解析后的显示名与图标来源；未解析的项目使用本地回退身份。这让排序、筛选和图标显示在同一应用名对应多个可执行路径时仍保持一致。

### 每日分析证据流

前端 `App.tsx` 构造确定性的预览 `DailyAnalysisEvidence`：浏览记录数固定回退为 `0`，分类覆盖率来自 UI 的四舍五入百分比，数值和排序会被规范化，并以 FNV-1a 生成前端 `evidenceHash`。这个预览证据只用于浏览器预览或 Tauri 结果不可用时的界面本地回退，不能作为持久化、排队或替换 AI 结果的依据。

桌面端的权威证据由 Rust `AppService` 从 SQLite 的活动、目标和浏览记录构造。它使用实际浏览记录数与未取整的覆盖率，对规范化后的序列化证据计算 SHA-256；该哈希用于保存结果的有效性判断、`ai_jobs` 去重/排队，以及仅在当前证据哈希匹配时替换 AI 分析。`AiAnalysisPanel` 显示的是 Tauri 返回的权威结果；在离线、失败或等待补算时，Rust 同样基于该权威证据生成本地分析，避免把前端预览与持久化统计混淆。

## Trend intelligence backend

`get_trends` remains the authoritative range boundary contract. Trend analysis commands accept the same inclusive current/comparison dates and explicit local-day boundary arrays, rebuild one authoritative `TrendPayload`, and key every saved result by `(range_start, range_end, evidence_hash)`. Exact-hash lookup means an older provider completion can remain auditable without replacing or being returned for newer evidence from the same range.

Local trend analysis and Markdown export require no provider or network. The first local result is persisted in `trend_analyses` with source `local`, model `deterministic-v1`, and its real generation time; later exact-hash reads reuse it. `queue_trend_analysis` writes to the existing `ai_jobs` table only when AI backfill is enabled and the existing provider registry has an enabled, automatic-safe provider with a configured Credential Manager key. The subject key includes the exact range and evidence hash, so ordinary requests deduplicate; an explicit `force` request atomically increments the job generation, returns a completed or running job to pending, and resets its retry state. Workers atomically claim a generation token. Provider trend completion validates the matching running token, upserts `trend_analyses`, and completes that generation in one SQLite transaction; any mismatch rolls back without an analysis write. Failure/retry remains gated by the same token, so an older worker cannot change the newly queued generation.

The queued job contains the aggregate `TrendPayload` plus the authoritative current/comparison boundary arrays needed to rebuild that same evidence from current SQLite data. Before saving a provider completion, the shared worker calls the service path that rebuilds the range and compares the new hash with the queued hash; stale output is discarded. Only the nested aggregate `TrendPayload` is sent to the provider. Raw files, browser history paths, complete browser bodies, window titles, and source records are outside this boundary.

For each authoritative `TrendPayload`, the service builds finite summary, observation, and suggestion candidate sets from fixed local templates. These templates contain no numeric literals or personal judgments and are also reused by deterministic local analysis. Change wording for scalar metrics requires a non-null authoritative delta whose absolute value is at least eight percent. Category and application change wording requires a stable name-union comparison with exact integer-second equality and a `1e-9` share tolerance; an empty previous breakdown produces only a neutral current-structure statement. The provider receives the aggregate evidence plus those candidates, never source records, and may only select from them. Provider output is accepted only as one JSON object with exactly `summary`, `observations`, `suggestions`, and `confidence`: summary must exactly equal an allowed summary; every non-empty, unique, bounded observation and suggestion must exactly equal an item in its corresponding set; confidence must be finite and between zero and one. Unknown or missing fields, wrappers, duplicates, over-limit arrays, and any rewritten or newly introduced text are rejected.

`export_trend_markdown` writes UTF-8 Obsidian Markdown from the authoritative evidence and the exact-hash analysis result. Its YAML records the range, generation time, evidence hash, and confidence; the body includes summary and quality callouts, KPI and daily evidence tables, category/application changes, observations, and bounded suggestions.
