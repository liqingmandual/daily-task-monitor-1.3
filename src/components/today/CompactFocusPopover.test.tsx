import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CompactFocusPopover, formatFocusCountdown } from "./CompactFocusPopover";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

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
    const render = async (canStart: boolean) => act(async () => root.render(<CompactFocusPopover goal="Goal" minutes={45} running={false} paused={false} remainingSeconds={null} taskId="task-1" tasks={[task]} canStart={canStart} onTaskChange={onTaskChange} onMinutesChange={vi.fn()} onToggle={onToggle} />));
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

  it("shows the running countdown and clamps expired sessions", async () => {
    expect(formatFocusCountdown(1_501)).toBe("25:01");
    expect(formatFocusCountdown(-1)).toBe("00:00");

    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    try {
      await act(async () => root.render(<CompactFocusPopover goal="Goal" minutes={25} running paused={false} remainingSeconds={83} taskId="" tasks={[]} canStart onTaskChange={vi.fn()} onMinutesChange={vi.fn()} onToggle={vi.fn()} />));
      expect(container.querySelector(".compact-focus-countdown")?.textContent).toContain("01:23");
      await act(async () => root.render(<CompactFocusPopover goal="Goal" minutes={25} running paused={false} remainingSeconds={82} taskId="" tasks={[]} canStart onTaskChange={vi.fn()} onMinutesChange={vi.fn()} onToggle={vi.fn()} />));
      expect(container.querySelector(".compact-focus-countdown")?.textContent).toContain("01:22");

      await act(async () => root.render(<CompactFocusPopover goal="Goal" minutes={25} running paused remainingSeconds={82} taskId="" tasks={[]} canStart onTaskChange={vi.fn()} onMinutesChange={vi.fn()} onToggle={vi.fn()} />));
      expect(container.querySelector(".compact-focus-countdown")?.textContent).toContain("PAUSED01:22");
    } finally {
      await act(async () => root.unmount());
    }
  });
});
