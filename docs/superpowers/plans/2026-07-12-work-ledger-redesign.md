# Work Ledger Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` and implement one task at a time with review checkpoints.

**Goal:** Replace the ambiguous Workflow tab with an offline-first project and task progress ledger grounded in goals, focus sessions, activity segments, browser evidence, and user-confirmed assignments.

**Architecture:** Activity classification remains an independent fact about what kind of activity occurred. A new work-ledger domain models `Project -> Task -> ProgressEntry / EvidenceLink`. Rust and SQLite own identity, assignment, rollups, and provenance. React renders a decision-oriented ledger and never treats an activity segment itself as a task.

**Tech Stack:** Tauri 2, Rust, SQLite, React 19, TypeScript, Vitest, existing Lucide and styling system.

## Global Constraints

- Remain fully usable offline; AI is optional enrichment, not a prerequisite.
- Never rewrite or delete original activity, browser, goal, focus, or classification evidence.
- Manual project/task assignments always outrank AI suggestions.
- Do not infer mood, personality, health, or ability.
- Preserve classification as the activity-type dimension; project/task assignment is a separate dimension.
- A task may have many activity segments, browser visits, focus sessions, and progress entries.
- Imported daily goals are historical text snapshots, not automatically one task per line without user confirmation.

---

### Task 1: Work Ledger Domain And Migration

**Files:**
- Create: `src-tauri/src/work_ledger/domain.rs`
- Create: `src-tauri/src/work_ledger/repository.rs`
- Create: `src-tauri/src/work_ledger/service.rs`
- Create: `src-tauri/src/work_ledger/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/db.rs`
- Test: `src-tauri/tests/work_ledger_database.rs`
- Test: `src-tauri/tests/work_ledger_service.rs`

- [ ] Add versioned migration tables: `projects`, `tasks`, `task_activity_links`, `task_browser_links`, `task_progress_entries`.
- [ ] Add project fields: id, name, color, status, description, created/updated/archive timestamps.
- [ ] Add task fields: id, project id, title, status, priority, expected output, due date, created/updated/completed timestamps.
- [ ] Add evidence-link provenance: `manual | rule | ai`, confidence, reason, created timestamp; enforce uniqueness per evidence/task.
- [ ] Implement CRUD, archive, task status transitions, evidence assignment/removal, and project/task duration rollups.
- [ ] Prove migrations are idempotent and original tables/data remain unchanged.
- [ ] Commit: `feat: add work ledger domain and migration`.

---

### Task 2: Ledger Query Contract And Evidence Suggestions

**Files:**
- Create: `src-tauri/src/work_ledger/commands.rs`
- Modify: `src-tauri/src/work_ledger/service.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src/lib/desktop.ts`
- Test: `src-tauri/tests/work_ledger_service.rs`
- Test: `src/lib/desktop.test.ts`

- [ ] Add `get_work_ledger(start_ms, end_ms, project_id)` returning projects, tasks, progress, linked evidence, unassigned evidence, and summary counts.
- [ ] Add commands for project/task save, status update, progress entry, evidence assignment, and assignment removal.
- [ ] Expose complete evidence metadata including classification source, reason, confidence, application, title, and duration.
- [ ] Build deterministic local suggestions from goal/focus text, application/title keywords, browser domain/title, and prior manual assignments.
- [ ] Queue optional AI suggestions only for ambiguous unassigned evidence; never auto-apply below 70% confidence.
- [ ] Keep manual assignment authoritative and evidence-hash suggestions for deduplication/stale-result protection.
- [ ] Commit: `feat: add work ledger commands and suggestions`.

---

### Task 3: Project And Task Progress Ledger UI

**Files:**
- Create: `src/components/workflow/WorkflowPage.tsx`
- Create: `src/components/workflow/ProjectSidebar.tsx`
- Create: `src/components/workflow/TaskLedger.tsx`
- Create: `src/components/workflow/TaskEditor.tsx`
- Create: `src/components/workflow/ProgressTimeline.tsx`
- Create: `src/components/workflow/EvidenceAssignmentPanel.tsx`
- Create: `src/components/workflow/ReviewQueue.tsx`
- Create: `src/components/workflow/WorkflowPage.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

- [ ] Replace the current eight-row segment list with a three-area desktop workspace: project rail, task ledger, evidence/progress inspector.
- [ ] Show task status, expected output, invested time, last activity, linked evidence count, and classification confidence.
- [ ] Support create/edit/archive project, create/edit/complete task, append progress, and assign/remove evidence.
- [ ] Add `待归属` review queue with local/AI suggestions and batch confirmation.
- [ ] Keep activity evidence inspectable and link back to the exact Today timeline record.
- [ ] At 980px collapse the inspector below; at 599px use project and task drawers without horizontal page overflow.
- [ ] Commit: `feat: build project task progress ledger`.

---

### Task 4: Goal And Focus Integration, Reporting, And Verification

**Files:**
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/desktop.rs`
- Modify: `src/lib/desktop.ts`
- Modify: `src/App.tsx`
- Modify: `src/components/workflow/WorkflowPage.tsx`
- Modify: `src/styles.css`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `README.md`
- Test: `src-tauri/tests/work_ledger_service.rs`
- Test: `src/components/workflow/WorkflowPage.test.tsx`

- [ ] Add nullable task linkage to focus sessions without breaking old sessions.
- [ ] Add `get_daily_goal` so stored goals/outputs restore correctly after date switch or restart.
- [ ] Allow a confirmed daily-goal line to create or link a task, but never silently duplicate tasks.
- [ ] Attach focus outcomes and actual output to progress entries with provenance.
- [ ] Include project/task rollups in daily, weekly, monthly, custom-range, and Obsidian Markdown reports.
- [ ] Verify offline operation, manual precedence, stale AI protection, migration, Today timeline drill-down, responsive layouts, and native Tauri smoke.
- [ ] Commit: `feat: integrate goals focus and work ledger reporting`.

## Acceptance Test

1. Create a project and task offline.
2. Assign two activity segments and one browser visit; rollup duration equals clipped evidence duration with no double counting.
3. Start and complete a focus session linked to the task; its outcome appears in progress history.
4. Confirm a suggestion; refresh; assignment and provenance persist.
5. Manually move evidence to another task; later AI backfill cannot override it.
6. Switch dates and restart; daily goals restore and linked task context remains intact.
7. Open Today from a ledger evidence item; the exact timeline record lands below the fixed header.
8. Export Markdown; project, task, invested time, evidence, and progress are represented without unsupported claims.
