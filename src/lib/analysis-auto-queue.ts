export const DAILY_ANALYSIS_COOLDOWN_MS = 30 * 60 * 1_000;
export const DAILY_ANALYSIS_INCREMENT_SECONDS = 15 * 60;

export interface DailyAnalysisQueueCheckpoint {
  evidenceHash: string;
  monitoredSeconds: number;
  goalSignature: string;
  queuedAtMs: number;
}

export function shouldAutoQueueDailyAnalysis(
  previous: DailyAnalysisQueueCheckpoint | undefined,
  current: DailyAnalysisQueueCheckpoint,
): boolean {
  if (!previous) return true;
  if (previous.evidenceHash === current.evidenceHash) return false;
  if (current.queuedAtMs - previous.queuedAtMs < DAILY_ANALYSIS_COOLDOWN_MS) return false;
  if (previous.goalSignature !== current.goalSignature) return true;
  return current.monitoredSeconds - previous.monitoredSeconds >= DAILY_ANALYSIS_INCREMENT_SECONDS;
}
