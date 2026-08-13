export function remainingFocusSeconds(endsAtMs: number, observedAtMs: number): number {
  return Math.max(0, Math.ceil((endsAtMs - observedAtMs) / 1_000));
}

export function nextFocusTickDelay(endsAtMs: number, observedAtMs: number): number | null {
  const remainingMs = endsAtMs - observedAtMs;
  if (remainingMs <= 0) return null;
  const untilNextSecondBoundary = remainingMs % 1_000;
  return untilNextSecondBoundary === 0 ? 1_000 : untilNextSecondBoundary;
}
