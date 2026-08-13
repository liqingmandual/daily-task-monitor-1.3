import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CompactFocusControl } from "./CompactFocusControl";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("CompactFocusControl", () => {
  it("updates only its countdown on the exact next second boundary", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_250);
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
      await act(async () => root.render(<CompactFocusControl goal="Goal" minutes={25} running paused={false} endsAtMs={84_000} pausedRemainingSeconds={null} taskId="" tasks={[]} canStart onTaskChange={vi.fn()} onMinutesChange={vi.fn()} onToggle={vi.fn()} />));
      const trigger = container.querySelector(".focus-timer-trigger") as HTMLButtonElement;
      const popoverCountdown = () => container.querySelector(".compact-focus-countdown > strong")?.textContent;
      expect(trigger.textContent).toContain("01:23");
      await act(async () => trigger.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(popoverCountdown()).toBe("01:23");
      const initialPopoverNode = container.querySelector(".compact-focus-countdown > strong");
      await act(async () => vi.advanceTimersByTimeAsync(749));
      expect(trigger.textContent).toContain("01:23");
      expect(popoverCountdown()).toBe("01:23");
      await act(async () => vi.advanceTimersByTimeAsync(1));
      expect(trigger.textContent).toContain("01:22");
      expect(popoverCountdown()).toBe("01:22");
      expect(container.querySelector(".compact-focus-countdown > strong")).not.toBe(initialPopoverNode);
      await act(async () => vi.advanceTimersByTimeAsync(3_000));
      expect(trigger.textContent).toContain("01:19");
      expect(popoverCountdown()).toBe("01:19");
    } finally {
      await act(async () => root.unmount());
    }
  });
});
