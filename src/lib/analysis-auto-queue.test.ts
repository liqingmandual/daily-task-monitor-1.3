import { describe, expect, it } from "vitest";
import {
  DAILY_ANALYSIS_COOLDOWN_MS,
  shouldAutoQueueDailyAnalysis,
  type DailyAnalysisQueueCheckpoint,
} from "./analysis-auto-queue";

function checkpoint(overrides: Partial<DailyAnalysisQueueCheckpoint> = {}): DailyAnalysisQueueCheckpoint {
  return {
    evidenceHash: "hash-1",
    monitoredSeconds: 3_600,
    goalSignature: "goal-a",
    queuedAtMs: 1_000,
    ...overrides,
  };
}

describe("shouldAutoQueueDailyAnalysis", () => {
  it("queues the first matching scope and does not queue the same evidence twice", () => {
    const current = checkpoint();
    expect(shouldAutoQueueDailyAnalysis(undefined, current)).toBe(true);
    expect(shouldAutoQueueDailyAnalysis(current, current)).toBe(false);
  });

  it("requires both cooldown and a 15-minute evidence increment", () => {
    const previous = checkpoint();
    expect(shouldAutoQueueDailyAnalysis(previous, checkpoint({
      evidenceHash: "hash-2",
      monitoredSeconds: previous.monitoredSeconds + 900,
      queuedAtMs: previous.queuedAtMs + DAILY_ANALYSIS_COOLDOWN_MS - 1,
    }))).toBe(false);
    expect(shouldAutoQueueDailyAnalysis(previous, checkpoint({
      evidenceHash: "hash-2",
      monitoredSeconds: previous.monitoredSeconds + 899,
      queuedAtMs: previous.queuedAtMs + DAILY_ANALYSIS_COOLDOWN_MS,
    }))).toBe(false);
    expect(shouldAutoQueueDailyAnalysis(previous, checkpoint({
      evidenceHash: "hash-2",
      monitoredSeconds: previous.monitoredSeconds + 900,
      queuedAtMs: previous.queuedAtMs + DAILY_ANALYSIS_COOLDOWN_MS,
    }))).toBe(true);
  });

  it("allows a goal/output change after the cooldown without requiring more activity time", () => {
    const previous = checkpoint();
    expect(shouldAutoQueueDailyAnalysis(previous, checkpoint({
      evidenceHash: "hash-2",
      goalSignature: "goal-b",
      queuedAtMs: previous.queuedAtMs + DAILY_ANALYSIS_COOLDOWN_MS,
    }))).toBe(true);
  });
});
