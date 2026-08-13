# Product direction and competitor analysis

## Positioning

Orbit should become a local-first, cross-platform and explainable
activity tracker for knowledge work. It should automatically capture apps and
active websites, organize fragmented activity into projects and tasks, and
provide focus, learning, and healthy-break feedback without surveillance.

The differentiator is not collection alone. It is the combination of private
local evidence, explainable classification, work-ledger organization, and
optional AI constrained by locally verified facts.

## Current capabilities

- Five-second foreground observation on Windows and macOS, with change-driven
  raw persistence and a 60-second storage heartbeat.
- Window title, executable identity, idle detection, and continuity repair.
- Read-only Chromium history collection with URL credential/query redaction.
- Authenticated loopback browser watchers that measure active-tab duration for
  Chromium, Firefox, and converted Safari extensions.
- Local SQLite storage, manual exclusions, and manual classification overrides.
- Today, trends, comparison ranges, workflow/task ledger, AI review, knowledge
  graph, and Markdown/DOCX export.
- Optional API-provider or local Codex analysis with validation and audit data.

The macOS collector currently records app identity, window title when macOS
permits it, and idle duration. Media playback parity with Windows remains open.
Browser history is evidence of a visit, not authoritative active-tab duration.

## Competitor map

### ManicTime

ManicTime is the reporting and timesheet benchmark. It automatically records
applications, websites, and documents on Windows, macOS, and Linux, then adds
auto-tagging, billing, team reporting, and cloud or on-premises deployment.

Reference: <https://www.manictime.com/>

### ActivityWatch

ActivityWatch is the open and local-first architecture benchmark. Its watcher
model supports Windows, macOS, Linux, and Android; browser extensions provide
active-tab events. Cross-device sync exists but is still described as beta and
depends on an external file-sync mechanism.

References:

- <https://docs.activitywatch.net/en/latest/watchers.html>
- <https://docs.activitywatch.net/en/latest/syncing.html>

### Rize

Rize is the automatic categorization and behavior-coaching benchmark. It runs
on macOS and Windows, tracks application/window/URL metadata, attributes live
entries to clients and projects, and combines focus metrics, break reminders,
calendar context, and team integrations.

Reference: <https://rize.io/features/automatic-time-tracking>

### Timing

Timing is the macOS depth benchmark. It records active apps, document paths or
URLs, provides mature project rules and reports, and integrates with macOS
applications and automation. It is not a Windows/Linux tracker.

Reference: <https://timingapp.com/help/faq>

## Product gaps

1. Authoritative website duration requires an installed browser watcher;
   browser history remains visit evidence, and browsers without a connected
   watcher have no measured active-tab duration.
2. There is no cross-device event synchronization or conflict model.
3. Classification corrections do not yet offer a complete transparent
   "apply once / create future rule" learning loop.
4. macOS lacks media-session parity, signed/notarized releases, and production
   permission onboarding.
5. There is no activity-aware break-reminder loop.
6. Long-term retention, compaction, export portability, and database health
   controls need product surfaces.

## Delivery plan

### P0: trustworthy capture

- [x] Keep platform collectors behind one `MonitorSample` contract.
- [x] Enforce one collector authority per SQLite database while allowing
  concurrent local UI instances.
- [x] Normalize historical overlap consistently across Today and Trends without
  rewriting raw evidence or filling unobserved gaps.
- [x] Validate macOS app/window/idle collection across permission denial,
  idle, sleep/wake, and offline operation on a real device with the repeatable
  checklist and database smoke check.
- [x] Add browser watcher protocol v1 and local extension sources for Chromium,
  Firefox, and Safari; history visits remain separate from measured duration.
- [x] Add permission diagnostics and an explicit collection-health screen.
- [x] Keep Windows and macOS collector tests green in CI. Run 12 at P0 closure
  commit `19ae652` passed frontend, extension, Rust, and Windows independent
  desktop jobs across the configured Windows/macOS matrix.

Signed, notarized macOS packaging is deliberately deferred until a later
distribution phase.

### Pre-P1: focus visibility

- [x] Restore the running Focus session from SQLite and show one synchronized,
  second-level countdown in the dashboard header and Focus popover.
- [x] Show the same countdown in the macOS menu bar while leaving the Windows
  tray presentation unchanged.
- [x] Control one persisted Focus session from the macOS tray with explicit
  start, pause/continue, and end actions, plus a deduplicated completion notification.

### P1: explainable organization and sync

- Show facts, derived aggregates, classification inference, and suggestions as
  separate layers with evidence and confidence.
- Turn accepted manual corrections into previewable rules.
- Synchronize append-only device events with stable device/event IDs,
  deterministic merging, and optional end-to-end encrypted storage.
- Add calendar and project-system context without silently sharing raw personal
  activity.

### P2: behavior assistance and commercial workflows

- Add activity-aware 25/30/45/50-minute break reminders that defer during
  meetings, video playback, presentation, idle, or lock screen.
- Finish the visual-system validation started by the Orbit UI refresh: audit
  remaining empty, loading, error, hover, and focus states across Today,
  Trends, Workflow, and AI Review.
- Validate the visual refresh at common desktop window sizes and for keyboard
  navigation, contrast, reduced motion, and light/dark appearance before
  treating it as complete.
- Add billing-safe time entries, rounding, client reports, and integrations.
- Add team sharing based on approved project entries, never raw personal logs.

## Success criteria

- Application and active-tab segments have explicit provenance and coverage.
- Sleep/wake, idle, midnight, timezone, and multi-device tests are deterministic.
- A user can inspect, exclude, correct, export, retain, and delete local data.
- Raw personal evidence never leaves the device without a clear opt-in boundary.
- The product remains useful with networking and AI completely disabled.
