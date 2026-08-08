import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ProgressTimeline } from "./ProgressTimeline";

afterEach(() => vi.unstubAllGlobals());

describe("ProgressTimeline", () => {
  it("mounts compact provenance labels and submits manual progress", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const onAdd = vi.fn().mockResolvedValue(true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    try {
      await act(async () => root.render(<ProgressTimeline
        taskId="task-1"
        pending={false}
        onAdd={onAdd}
        entries={[
          { id: "manual", taskId: "task-1", note: "One", createdAtMs: 3, originKind: "manual", sourceId: null, sourceDate: null },
          { id: "focus", taskId: "task-1", note: "Two", createdAtMs: 2, originKind: "focus_outcome", sourceId: "focus-1", sourceDate: "2026-07-13" },
          { id: "output", taskId: "task-1", note: "Three", createdAtMs: 1, originKind: "daily_actual_output", sourceId: "2026-07-13", sourceDate: "2026-07-13" },
        ]}
      />));
      expect([...container.querySelectorAll(".workflow-progress-origin")].map((item) => item.textContent)).toEqual(["Manual", "Focus", "Daily output"]);

      const textarea = container.querySelector<HTMLTextAreaElement>("#workflow-progress-note")!;
      textarea.value = "Mounted note";
      await act(async () => textarea.dispatchEvent(new window.Event("input", { bubbles: true })));
      await act(async () => container.querySelector("form")!.dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true })));
      expect(onAdd).toHaveBeenCalledWith("task-1", "Mounted note");
      expect(textarea.value).toBe("");
    } finally {
      await act(async () => root.unmount());
    }
  });
});
