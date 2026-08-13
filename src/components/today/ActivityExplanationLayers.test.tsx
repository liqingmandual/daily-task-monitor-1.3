import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Segment } from "../../lib/metrics";
import { ActivityExplanationLayers } from "./ActivityExplanationLayers";

function segment(overrides: Partial<Segment> = {}): Segment {
  return {
    id: "segment-1",
    startMs: 1_000,
    endMs: 63_000,
    app: "Code",
    title: "Orbit",
    category: "creation_development",
    videoPurpose: "unknown",
    confidence: 0.91,
    classificationSource: "behavior",
    classificationReason: "Editing activity in a creation tool",
    classificationModelVersion: "rules-v1",
    needsReview: false,
    ...overrides,
  };
}

describe("ActivityExplanationLayers", () => {
  it("keeps facts, derived values, inference, and suggestions distinct", () => {
    const html = renderToStaticMarkup(<ActivityExplanationLayers segment={segment()} />);
    expect(html).toContain('data-layer="fact"');
    expect(html).toContain("Code · Orbit");
    expect(html).toContain('data-layer="derived"');
    expect(html).toContain("持续 1 分 2 秒");
    expect(html).toContain('data-layer="inference"');
    expect(html).toContain("本地行为推断 · 置信度 91%");
    expect(html).toContain('data-layer="suggestion"');
    expect(html).toContain("当前无需复核");
  });

  it("does not present a pending classification as fact", () => {
    const html = renderToStaticMarkup(<ActivityExplanationLayers segment={segment({ category: "pending", classificationSource: "pending", confidence: 0.2, needsReview: true })} />);
    expect(html).toContain("未分类 · 待分类 · 置信度 20%");
    expect(html).toContain("确认前不会把推断当作事实");
  });
});
