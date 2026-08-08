import { describe, expect, it } from "vitest";
import type { KnowledgeGraphPayload } from "./desktop";
import { prepareKnowledgeSpaceLayout } from "./knowledge-graph-layout";

const payload: KnowledgeGraphPayload = {
  startMs: 0,
  endMs: 100_000,
  totalSeconds: 90,
  counts: { category: 1, app: 1, activity: 2 },
  nodes: [
    { id: "category:research", kind: "category", label: "Research", durationSeconds: 90, category: "research", confidence: 1, occurredAtMs: null, metadata: {} },
    { id: "app:chrome", kind: "app", label: "Chrome", durationSeconds: 90, category: "", confidence: .9, occurredAtMs: null, metadata: {} },
    { id: "activity:a", kind: "activity", label: "Article A", durationSeconds: 60, category: "research", confidence: .9, occurredAtMs: 10_000, metadata: { app: "Chrome" } },
    { id: "activity:b", kind: "activity", label: "Article B", durationSeconds: 30, category: "research", confidence: .8, occurredAtMs: 40_000, metadata: { app: "Chrome" } },
  ],
  links: [
    { source: "activity:a", target: "category:research", kind: "classified-as", weightSeconds: 60 },
    { source: "activity:a", target: "app:chrome", kind: "used-app", weightSeconds: 60 },
    { source: "activity:b", target: "category:research", kind: "classified-as", weightSeconds: 30 },
    { source: "activity:b", target: "app:chrome", kind: "used-app", weightSeconds: 30 },
  ],
};

describe("prepareKnowledgeSpaceLayout", () => {
  it("keeps every node and link while assigning finite 3d positions", () => {
    const layout = prepareKnowledgeSpaceLayout(payload);

    expect(layout.nodes).toHaveLength(payload.nodes.length);
    expect(layout.links).toHaveLength(payload.links.length);
    expect(layout.nodes.every((node) => [node.x, node.y, node.z].every(Number.isFinite))).toBe(true);
  });

  it("is deterministic so the graph does not jump between refreshes", () => {
    expect(prepareKnowledgeSpaceLayout(payload)).toEqual(prepareKnowledgeSpaceLayout(payload));
  });

  it("renders semantic hubs larger than raw activity points", () => {
    const layout = prepareKnowledgeSpaceLayout(payload);
    const category = layout.nodes.find((node) => node.id === "category:research")!;
    const activity = layout.nodes.find((node) => node.id === "activity:a")!;

    expect(category.size).toBeGreaterThan(activity.size);
  });
});
