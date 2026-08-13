# Activity data integrity

Orbit keeps raw collector records in SQLite and derives one authoritative
timeline for reporting. This separates recoverable source evidence from the
cross-platform rules used by dashboard and trend analysis surfaces.

## One collector per database

Multiple desktop windows may run during local multi-agent development, but
they must not create independent activity streams in the same database. A
singleton row in `collector_lease` assigns collection authority to one process:

- the owner renews the lease during the five-second collection loop;
- the lease expires after 20 seconds without a renewal;
- another process may collect only after acquiring an absent or expired lease;
- disabling monitoring releases an owned lease immediately;
- non-owning processes keep their UI and read paths available but reset their
  in-memory monitor engine and do not sample or write activity.

The lease is stored in the same SQLite database as the activity records, so the
rule is independent of Windows or macOS process APIs and follows the actual
storage boundary. It is not a process-wide single-instance lock.

## Canonical activity timeline

Historical databases may already contain overlapping records, and an expired
lease can briefly overlap with a slow former owner. Raw records are therefore
preserved and normalized at the reporting boundary:

1. Clip every record to the requested range.
2. Split the range at every record start and end boundary.
3. Select exactly one record for every covered interval. Active records take
   precedence over Idle; remaining ties use later start, later end, and ID.
4. Merge adjacent intervals when the same record remains authoritative.
5. Leave intervals with no source record empty.

The deterministic implementation lives in
`src-tauri/src/segment_overlap.rs`. Dashboard totals, dashboard composition,
legacy trends, the trend workbench, and workflow-linked trend facts consume the
same canonical stream. React fallback metrics use the equivalent contract in
`src/lib/segment-overlap.ts`.

Normalization is non-destructive: it does not rewrite or delete raw activity
rows. This keeps later auditing and improved repair policies possible.

## Metric semantics

`monitored time` is the union of covered Active and Idle intervals. It does not
include gaps with no record, but it does include recorded Idle time while Orbit
is running, including long periods away from the computer. Consequently:

- one calendar day cannot contribute more than its actual local-day length;
- overlapping collectors cannot multiply monitored, category, or application
  time;
- Active wins when an active observation overlaps an Idle backfill;
- a missing day contributes zero to a selected-range average;
- sleep, continuity gaps, and input-idle records remain distinguishable data.

## Date rollover and refresh

When the dashboard is following today, a local-date change detected by the
minute timer, window focus, or visibility change advances the selected date and
requests a new snapshot. Loaded segments and authoritative composition carry
their source date, so yesterday's snapshot is never rendered under today's
heading while the refresh is pending. Explicitly selected historical dates do
not auto-advance.

## Regression expectations

Tests cover lease exclusion and expiry takeover, Active-over-Idle selection,
unobserved gaps, dashboard totals and composition, both trend APIs, workflow
drilldown duration, frontend fallback metrics, local-date rollover, and the
neutral Idle timeline color. Any new reporting surface should consume the
canonical timeline rather than sum raw segment durations directly.
