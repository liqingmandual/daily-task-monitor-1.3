import type { Segment } from "./metrics";

export function canonicalizeOverlappingSegments(segments: readonly Segment[]): Segment[] {
  const valid = segments.filter((segment) => segment.endMs > segment.startMs);
  const boundaries = [...new Set(valid.flatMap((segment) => [segment.startMs, segment.endMs]))]
    .sort((left, right) => left - right);
  const result: Segment[] = [];
  for (let index = 0; index + 1 < boundaries.length; index += 1) {
    const startMs = boundaries[index];
    const endMs = boundaries[index + 1];
    const winner = valid
      .filter((segment) => segment.startMs < endMs && segment.endMs > startMs)
      .sort((left, right) => {
        const activeDifference = Number(right.category !== "idle") - Number(left.category !== "idle");
        return activeDifference
          || right.startMs - left.startMs
          || right.endMs - left.endMs
          || right.id.localeCompare(left.id);
      })[0];
    if (!winner) continue;
    const previous = result.at(-1);
    if (previous?.id === winner.id && previous.endMs === startMs) {
      previous.endMs = endMs;
    } else {
      result.push({ ...winner, startMs, endMs });
    }
  }
  return result;
}
