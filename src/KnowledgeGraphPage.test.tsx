import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import KnowledgeGraphPage from "./KnowledgeGraphPage";
import type { KnowledgeGraphPayload } from "./lib/desktop";

const payload: KnowledgeGraphPayload = {
  nodes: [{ id: "category:research", kind: "category", label: "搜索/调研", durationSeconds: 3600, category: "research", confidence: 1, occurredAtMs: null, metadata: {} }],
  links: [],
  counts: { category: 1 },
  totalSeconds: 3600,
  startMs: 0,
  endMs: 1,
};

describe("KnowledgeGraphPage", () => {
  it("renders the experimental graph console controls and summary", () => {
    const html = renderToStaticMarkup(<KnowledgeGraphPage initialPayload={payload} onBack={() => undefined} onOpenTimeline={() => undefined} />);

    expect(html).toContain("知识空间");
    expect(html).toContain("近 30 天");
    expect(html).toContain("近 7 天");
    expect(html).toContain("今日");
    expect(html).toContain("强光");
    expect(html).toContain("可读");
    expect(html).toContain("应用");
    expect(html).toContain("网页域名");
    expect(html).toContain("1 个节点");
    expect(html).toContain('aria-label="返回数据表盘"');
  });
});
