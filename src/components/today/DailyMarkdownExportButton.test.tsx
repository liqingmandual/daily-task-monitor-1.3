import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DailyMarkdownExportButton } from "./DailyMarkdownExportButton";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => { resolve = nextResolve; reject = nextReject; });
  return { promise, resolve, reject };
}

afterEach(() => vi.unstubAllGlobals());

describe("DailyMarkdownExportButton", () => {
  it("surfaces pending and failure states from a mounted export action", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const pending = deferred<string | null>();
    const exporter = vi.fn().mockReturnValue(pending.promise);
    const onMessage = vi.fn();
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    try {
      await act(async () => root.render(<DailyMarkdownExportButton date="2026-07-13" desktopRuntime exporter={exporter} onMessage={onMessage} />));
      const button = container.querySelector<HTMLButtonElement>("button[aria-label='导出今日日报 Markdown']")!;
      await act(async () => button.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(exporter).toHaveBeenCalledWith("2026-07-13", "markdown");
      expect(button.disabled).toBe(true);
      expect(button.textContent).toContain("正在导出");
      expect(onMessage).toHaveBeenLastCalledWith("正在导出今日日报（Markdown）…");

      await act(async () => pending.reject(new Error("disk full")));
      expect(button.disabled).toBe(false);
      expect(onMessage).toHaveBeenLastCalledWith(expect.stringContaining("disk full"));
    } finally {
      await act(async () => root.unmount());
    }
  });

  it("offers both Markdown and Word formats", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const exporter = vi.fn().mockResolvedValue("report.docx");
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    try {
      await act(async () => root.render(<DailyMarkdownExportButton date="2026-07-13" desktopRuntime exporter={exporter} onMessage={() => undefined} />));
      const word = container.querySelector<HTMLButtonElement>("button[aria-label='导出今日日报 Word']")!;
      expect(word).not.toBeNull();
      await act(async () => word.dispatchEvent(new window.Event("click", { bubbles: true })));
      expect(exporter).toHaveBeenCalledWith("2026-07-13", "docx");
    } finally {
      await act(async () => root.unmount());
    }
  });
});
