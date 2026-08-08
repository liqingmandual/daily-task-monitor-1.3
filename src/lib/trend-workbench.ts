import type { TrendRawRow } from "./desktop";

export interface TrendWorkbenchSelectionState {
  rangeKey: string;
  generation: number;
  selectedBucketId: string | null;
  selectedRawRowId: string | null;
}

export interface TrendWorkbenchViewIdentity {
  rangeKey: string;
  generation: number;
}

export interface TrendWorkbenchVisibleSelection extends TrendWorkbenchViewIdentity {
  bucketIds: readonly string[];
  visibleRowIds: readonly string[];
}

export function createTrendWorkbenchSelection(
  rangeKey: string,
): TrendWorkbenchSelectionState {
  return {
    rangeKey,
    generation: 0,
    selectedBucketId: null,
    selectedRawRowId: null,
  };
}

export function beginTrendWorkbenchRequest(
  state: TrendWorkbenchSelectionState,
  rangeKey: string,
): TrendWorkbenchSelectionState {
  const changedView = state.rangeKey !== rangeKey;
  return {
    ...state,
    rangeKey,
    generation: state.generation + 1,
    selectedBucketId: changedView ? null : state.selectedBucketId,
    selectedRawRowId: changedView ? null : state.selectedRawRowId,
  };
}

export function canApplyTrendWorkbenchResponse(
  state: TrendWorkbenchSelectionState,
  response: TrendWorkbenchViewIdentity,
): boolean {
  return state.rangeKey === response.rangeKey && state.generation === response.generation;
}

export function selectBucket(
  state: TrendWorkbenchSelectionState,
  bucketId: string,
  bucketIds: readonly string[],
): TrendWorkbenchSelectionState {
  if (!bucketIds.includes(bucketId)) return clearSelection(state);
  return {
    ...state,
    selectedBucketId: bucketId,
    selectedRawRowId: null,
  };
}

export function selectRawRow(
  state: TrendWorkbenchSelectionState,
  row: TrendRawRow,
  visible: Pick<TrendWorkbenchVisibleSelection, "bucketIds" | "visibleRowIds">,
): TrendWorkbenchSelectionState {
  if (!visible.bucketIds.includes(row.bucketId) || !visible.visibleRowIds.includes(row.rowId)) {
    return clearSelection(state);
  }
  return {
    ...state,
    selectedBucketId: row.bucketId,
    selectedRawRowId: row.rowId,
  };
}

export function clearSelection(
  state: TrendWorkbenchSelectionState,
): TrendWorkbenchSelectionState {
  return {
    ...state,
    selectedBucketId: null,
    selectedRawRowId: null,
  };
}

export function reconcileTrendWorkbenchSelection(
  state: TrendWorkbenchSelectionState,
  view: TrendWorkbenchVisibleSelection,
): TrendWorkbenchSelectionState {
  if (!canApplyTrendWorkbenchResponse(state, view)) return state;
  if (state.selectedBucketId && !view.bucketIds.includes(state.selectedBucketId)) {
    return clearSelection(state);
  }
  if (state.selectedRawRowId && !view.visibleRowIds.includes(state.selectedRawRowId)) {
    return { ...state, selectedRawRowId: null };
  }
  return state;
}

export function rowsForSelectedBucket(
  state: TrendWorkbenchSelectionState,
  rows: readonly TrendRawRow[],
): TrendRawRow[] {
  if (!state.selectedBucketId) return [...rows];
  return rows.filter((row) => row.bucketId === state.selectedBucketId);
}
