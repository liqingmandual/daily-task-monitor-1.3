import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AiQueueRecord } from "../../lib/desktop";
import { AiExecutionQueue, type AiExecutionQueueActions } from "./AiExecutionQueue";

function record(id: string): AiQueueRecord {
  return {
    id,
    generation: 0,
    kind: "classify_segment",
    status: "awaiting-reassignment",
    attempts: 2,
    nextAttemptAtMs: 2_000,
    lastError: "Authorization: Bearer secret https://api.example.test",
    execution: {
      executionMode: "api-key",
      executorId: "legacy-provider-registry",
      model: "",
      evidenceHash: `hash-${id}`,
      createdAtMs: 1_000,
    },
    startedAtMs: null,
    finishedAtMs: null,
    durationMs: null,
    executorId: null,
    model: null,
    exitCode: null,
    errorKind: "provider",
  };
}

afterEach(() => vi.unstubAllGlobals());

describe("AiExecutionQueue", () => {
  it("lists paused jobs, redacts diagnostics and retries the selected batch", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    Object.assign(window, { confirm: vi.fn().mockReturnValue(true) });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const records = [record("legacy-1"), record("legacy-2")];
    const actions: AiExecutionQueueActions = {
      list: vi.fn().mockResolvedValue(records),
      retry: vi.fn().mockResolvedValue(records.map((item) => ({ ...item, id: `new-${item.id}`, status: "pending" as const }))),
    };
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    await act(async () => root.render(<AiExecutionQueue actions={actions} />));
    await act(async () => undefined);

    expect(actions.list).toHaveBeenCalledWith({ status: "awaiting-reassignment", limit: 500 });
    expect(container.textContent).toContain("旧 Provider 队列");
    expect(container.textContent).toContain("诊断信息已脱敏");
    expect(container.textContent).not.toContain("secret");

    const selectAll = container.querySelector<HTMLInputElement>('[aria-label="选择全部等待改派任务"]');
    await act(async () => selectAll?.dispatchEvent(new window.Event("click", { bubbles: true })));
    const retry = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("按当前方式重新排队"));
    await act(async () => retry?.dispatchEvent(new window.Event("click", { bubbles: true })));
    await act(async () => undefined);

    expect(actions.retry).toHaveBeenCalledWith(["legacy-1", "legacy-2"]);
    await act(async () => root.unmount());
  });

  it("locks the status filter while a reassignment is in flight", async () => {
    const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
    Object.assign(window, { confirm: vi.fn().mockReturnValue(true) });
    vi.stubGlobal("window", window);
    vi.stubGlobal("document", document);
    vi.stubGlobal("navigator", window.navigator);
    vi.stubGlobal("HTMLElement", window.HTMLElement);
    vi.stubGlobal("Node", window.Node);
    vi.stubGlobal("Event", window.Event);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    let finishRetry: ((records: AiQueueRecord[]) => void) | undefined;
    const retryPromise = new Promise<AiQueueRecord[]>((resolve) => { finishRetry = resolve; });
    const actions: AiExecutionQueueActions = {
      list: vi.fn().mockResolvedValue([record("legacy-1")]),
      retry: vi.fn().mockReturnValue(retryPromise),
    };
    const container = document.getElementById("root") as unknown as HTMLDivElement;
    const root = createRoot(container);
    await act(async () => root.render(<AiExecutionQueue actions={actions} />));
    await act(async () => undefined);

    const selectAll = container.querySelector<HTMLInputElement>('[aria-label="选择全部等待改派任务"]');
    await act(async () => selectAll?.dispatchEvent(new window.Event("click", { bubbles: true })));
    const retry = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("按当前方式重新排队"));
    await act(async () => retry?.dispatchEvent(new window.Event("click", { bubbles: true })));
    const statusSelect = container.querySelector<HTMLSelectElement>(".ai-execution-queue-toolbar select");
    expect(statusSelect?.disabled).toBe(true);

    await act(async () => finishRetry?.([]));
    expect(statusSelect?.disabled).toBe(false);
    await act(async () => root.unmount());
  });
});
