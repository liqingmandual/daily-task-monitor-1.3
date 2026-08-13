import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ExternalContextPanel } from "./ExternalContextPanel";

describe("ExternalContextPanel", () => {
  it("keeps imported plans visibly separate from activity facts", () => {
    const html = renderToStaticMarkup(<ExternalContextPanel items={[{
      id: "context-1",
      sourceId: "calendar-work",
      sourceName: "Work Calendar",
      sourceKind: "ics",
      externalId: "event-1",
      kind: "calendar_event",
      title: "Planning",
      startAtMs: Date.parse("2026-08-13T01:00:00Z"),
      endAtMs: Date.parse("2026-08-13T01:30:00Z"),
      projectName: "",
      status: "confirmed",
      importedAtMs: 1,
    }]} />);
    expect(html).toContain("今日上下文");
    expect(html).toContain("仅本地 · 不作为活动事实");
    expect(html).toContain("Planning");
  });

  it("renders nothing when no local context was imported", () => {
    expect(renderToStaticMarkup(<ExternalContextPanel items={[]} />)).toBe("");
  });
});
