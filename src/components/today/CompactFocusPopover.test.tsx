import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CompactFocusPopover } from "./CompactFocusPopover";

afterEach(() => vi.unstubAllGlobals());

describe("CompactFocusPopover", () => {
  it("prevents historical focus starts and enables today's task-aware start", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const onToggle = vi.fn();
    const onTaskChange = vi.fn();
    const task = { id: "task-1", projectId: "project-1", title: "Linked task", status: "in_progress" as const, priority: "high" as const, expectedOutput: "", dueDate: null, createdAtMs: 1, updatedAtMs: 1, completedAtMs: null };
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    const render = async (canStart: boolean) => act(async () => root.render(<CompactFocusPopover goal="Goal" minutes={45} running={false} taskId="task-1" tasks={[task]} canStart={canStart} onTaskChange={onTaskChange} onMinutesChange={vi.fn()} onToggle={onToggle} />));
    try {
      await render(false);
      const action = container.querySelector<HTMLButtonElement>(".compact-focus-action")!;
      expect(action.disabled).toBe(true);
      await act(async () => action.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(onToggle).not.toHaveBeenCalled();

      await render(true);
      expect(container.querySelector<HTMLButtonElement>(".compact-focus-action")?.disabled).toBe(false);
      await act(async () => container.querySelector(".compact-focus-action")!.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(onToggle).toHaveBeenCalledTimes(1);
    } finally {
      await act(async () => root.unmount());
    }
  });
});
