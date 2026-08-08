import { describe, expect, it } from "vitest";

import type { TrendRawRow } from "./desktop";
import {
  beginTrendWorkbenchRequest,
  canApplyTrendWorkbenchResponse,
  clearSelection,
  createTrendWorkbenchSelection,
  reconcileTrendWorkbenchSelection,
  rowsForSelectedBucket,
  selectBucket,
  selectRawRow,
} from "./trend-workbench";

const rows: TrendRawRow[] = [
  {
    rowId: "row-a",
    bucketId: "2024-04-10_2024-04-10",
    evidenceKind: "activity",
    evidenceId: "segment-a",
    date: "2024-04-10",
    startTime: "09:00:00",
    endTime: "09:10:00",
    app: "Codex",
    titleSummary: "Codex / creation_development",
    category: "creation_development",
    taskId: "task-a",
    taskTitle: "Task A",
    projectId: "project-a",
    projectName: "Project A",
    clippedDurationSeconds: 600,
    confidence: 1,
    reviewState: "confirmed",
    shared: false,
  },
  {
    rowId: "row-b",
    bucketId: "2024-04-11_2024-04-11",
    evidenceKind: "focus",
    evidenceId: "focus-b",
    date: "2024-04-11",
    startTime: "10:00:00",
    endTime: "10:05:00",
    app: "Focus",
    titleSummary: "Focus session",
    category: "focus",
    taskId: "task-b",
    taskTitle: "Task B",
    projectId: "project-a",
    projectName: "Project A",
    clippedDurationSeconds: 300,
    confidence: null,
    reviewState: "confirmed",
    shared: false,
  },
];

describe("trend workbench selection", () => {
  it("selects a bucket and filters raw rows to the same bucket", () => {
    const initial = createTrendWorkbenchSelection("2024-04-10:2024-04-11:day");
    const selected = selectBucket(initial, rows[1].bucketId, rows.map((row) => row.bucketId));

    expect(selected.selectedBucketId).toBe(rows[1].bucketId);
    expect(selected.selectedRawRowId).toBeNull();
    expect(rowsForSelectedBucket(selected, rows)).toEqual([rows[1]]);
  });

  it("selects a raw row and derives its bucket", () => {
    const initial = createTrendWorkbenchSelection("2024-04-10:2024-04-11:day");
    const selected = selectRawRow(initial, rows[0], {
      bucketIds: rows.map((row) => row.bucketId),
      visibleRowIds: rows.map((row) => row.rowId),
    });

    expect(selected.selectedRawRowId).toBe("row-a");
    expect(selected.selectedBucketId).toBe(rows[0].bucketId);
  });

  it("clears both sides of the selection", () => {
    const selected = selectRawRow(
      createTrendWorkbenchSelection("2024-04-10:2024-04-11:day"),
      rows[0],
      { bucketIds: [rows[0].bucketId], visibleRowIds: [rows[0].rowId] },
    );

    expect(clearSelection(selected)).toMatchObject({
      selectedBucketId: null,
      selectedRawRowId: null,
    });
  });

  it("changes generation and clears selection when the range or granularity key changes", () => {
    const selected = selectRawRow(
      createTrendWorkbenchSelection("2024-04-10:2024-04-11:day"),
      rows[0],
      { bucketIds: [rows[0].bucketId], visibleRowIds: [rows[0].rowId] },
    );
    const next = beginTrendWorkbenchRequest(selected, "2024-04-10:2024-04-11:week");

    expect(next.generation).toBe(selected.generation + 1);
    expect(next.rangeKey).toBe("2024-04-10:2024-04-11:week");
    expect(next.selectedBucketId).toBeNull();
    expect(next.selectedRawRowId).toBeNull();
  });

  it("rejects stale responses by generation and range key", () => {
    const initial = createTrendWorkbenchSelection("old:day");
    const pending = beginTrendWorkbenchRequest(initial, "new:week");

    expect(canApplyTrendWorkbenchResponse(pending, {
      rangeKey: "old:day",
      generation: initial.generation,
    })).toBe(false);
    expect(canApplyTrendWorkbenchResponse(pending, {
      rangeKey: pending.rangeKey,
      generation: pending.generation,
    })).toBe(true);
  });

  it("drops a hidden raw row while retaining its existing bucket selection", () => {
    const selected = selectRawRow(
      createTrendWorkbenchSelection("2024-04-10:2024-04-11:day"),
      rows[0],
      { bucketIds: [rows[0].bucketId], visibleRowIds: [rows[0].rowId] },
    );
    const reconciled = reconcileTrendWorkbenchSelection(selected, {
      rangeKey: selected.rangeKey,
      generation: selected.generation,
      bucketIds: [rows[0].bucketId],
      visibleRowIds: [],
    });

    expect(reconciled.selectedBucketId).toBe(rows[0].bucketId);
    expect(reconciled.selectedRawRowId).toBeNull();
  });

  it("clears an absent bucket and refuses rows whose bucket is not in the view", () => {
    const initial = createTrendWorkbenchSelection("2024-04-10:2024-04-11:day");
    const selected = selectBucket(initial, rows[0].bucketId, [rows[0].bucketId]);
    const reconciled = reconcileTrendWorkbenchSelection(selected, {
      rangeKey: selected.rangeKey,
      generation: selected.generation,
      bucketIds: [rows[1].bucketId],
      visibleRowIds: [rows[1].rowId],
    });
    const refused = selectRawRow(initial, rows[0], {
      bucketIds: [rows[1].bucketId],
      visibleRowIds: [rows[0].rowId],
    });

    expect(reconciled.selectedBucketId).toBeNull();
    expect(reconciled.selectedRawRowId).toBeNull();
    expect(refused.selectedBucketId).toBeNull();
    expect(refused.selectedRawRowId).toBeNull();
  });
});
