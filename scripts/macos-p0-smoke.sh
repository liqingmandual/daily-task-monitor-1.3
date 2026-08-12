#!/bin/sh
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
  echo "FAIL: this smoke check must run on macOS"
  exit 1
fi
command -v sqlite3 >/dev/null 2>&1 || { echo "FAIL: sqlite3 is unavailable"; exit 1; }

database_path="${1:-}"
if [ -z "$database_path" ]; then
  database_path="$(find "${HOME}/Library/Application Support" -path '*DailyTaskMonitor*/data/monitor.db' -type f -print -quit 2>/dev/null || true)"
fi
if [ -z "$database_path" ] || [ ! -f "$database_path" ]; then
  echo "FAIL: monitor.db not found; pass its path as the first argument"
  exit 1
fi

integrity="$(sqlite3 "$database_path" 'PRAGMA quick_check;')"
[ "$integrity" = "ok" ] || { echo "FAIL: SQLite quick_check returned: $integrity"; exit 1; }

now_ms="$(( $(date +%s) * 1000 ))"
recent_cutoff="$(( now_ms - 120000 ))"
recent_samples="$(sqlite3 "$database_path" "SELECT COUNT(*) FROM activity_samples WHERE sampled_at_ms >= $recent_cutoff;")"
gap_count="$(sqlite3 "$database_path" "SELECT COUNT(*) FROM activity_segments WHERE inactivity_reason = 'continuity_gap';")"
watcher_slices="$(sqlite3 "$database_path" "SELECT COUNT(*) FROM browser_activity_segments WHERE provenance = 'watcher-heartbeat-v1';" 2>/dev/null || printf '0')"

echo "PASS: database integrity"
echo "INFO: samples in last 120 seconds: $recent_samples"
echo "INFO: recorded continuity gaps: $gap_count"
echo "INFO: measured browser slices: $watcher_slices"
if [ "$recent_samples" -eq 0 ]; then
  echo "WARN: no recent sample; verify monitoring is enabled and the app is running"
  exit 2
fi
echo "PASS: recent desktop collection"
