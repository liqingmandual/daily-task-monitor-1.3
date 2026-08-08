import { act } from "react";
import { createRoot } from "react-dom/client";
import { parseHTML } from "linkedom";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  ConfirmedDailyGoalTask,
  DailyGoalRecord,
  DailyGoalTaskLink,
  WorkLedgerProject,
  WorkLedgerSnapshot,
  WorkLedgerTask,
} from "../../lib/desktop";
import { DailyGoalPanel } from "./DailyGoalPanel";

type TestWindow = { Event: typeof Event };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function task(id: string, date = "2026-07-13", title = id): WorkLedgerTask {
  return {
    id,
    projectId: "project-1",
    title,
    status: "in_progress",
    priority: "high",
    expectedOutput: "Verified output",
    dueDate: date,
    createdAtMs: 1_000,
    updatedAtMs: 1_000,
    completedAtMs: null,
  };
}

const project: WorkLedgerProject = {
  id: "project-1",
  name: "Desktop rewrite",
  color: "#2563eb",
  status: "active",
  description: "",
  createdAtMs: 1_000,
  updatedAtMs: 1_000,
  archivedAtMs: null,
};

function ledger(tasks: WorkLedgerTask[] = [task("task-1")]): WorkLedgerSnapshot {
  return {
    projects: [project],
    tasks,
    progress: [],
    linkedEvidence: [],
    unassignedEvidence: [],
    suggestions: [],
    ambiguousEvidenceHashes: [],
    summary: {
      projectCount: 1,
      taskCount: tasks.length,
      progressCount: 0,
      linkedEvidenceCount: 0,
      unassignedEvidenceCount: 0,
      localSuggestionCount: 0,
      ambiguousEvidenceCount: 0,
    },
  };
}

function goal(date: string, goals: string): DailyGoalRecord {
  return { date, goals, expectedOutput: "Expected", actualOutput: "" };
}

function link(date: string, goalRowId: string, goalText: string, taskId: string): DailyGoalTaskLink {
  return { goalRowId, goalDate: date, goalText, taskId, confirmedAtMs: 2_000 };
}

function actions(overrides: Record<string, unknown> = {}) {
  return {
    getGoal: vi.fn().mockImplementation(async (date: string) => goal(date, "Read paper")),
    saveGoal: vi.fn().mockResolvedValue(undefined),
    listLinks: vi.fn().mockResolvedValue([]),
    loadLedger: vi.fn().mockResolvedValue(ledger()),
    confirmLink: vi.fn().mockImplementation(async (request: { goalRowId: string; goalDate: string; goalText: string; taskId: string }): Promise<ConfirmedDailyGoalTask> => ({
      link: link(request.goalDate, request.goalRowId, request.goalText, request.taskId),
      task: task(request.taskId, request.goalDate),
      taskCreated: false,
    })),
    recordActualOutput: vi.fn().mockResolvedValue(null),
    ...overrides,
  };
}

async function mount({
  selectedDate = "2026-07-13",
  localPreview = false,
  actionOverrides = {},
}: {
  selectedDate?: string;
  localPreview?: boolean;
  actionOverrides?: Record<string, unknown>;
} = {}) {
  const { document, window } = parseHTML("<!doctype html><html><body><div id=\"root\"></div></body></html>");
  vi.stubGlobal("window", window);
  vi.stubGlobal("document", document);
  vi.stubGlobal("navigator", window.navigator);
  vi.stubGlobal("HTMLElement", window.HTMLElement);
  vi.stubGlobal("Node", window.Node);
  vi.stubGlobal("Event", window.Event);
  vi.stubGlobal("MouseEvent", window.MouseEvent);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const actionSet = actions(actionOverrides);
  const onGoalChange = vi.fn();
  const onOpenTask = vi.fn();
  const onLedgerChanged = vi.fn();
  const container = document.getElementById("root") as unknown as HTMLDivElement;
  const root = createRoot(container);
  const render = async (date = selectedDate) => {
    await act(async () => {
      root.render(<DailyGoalPanel
        selectedDate={date}
        localPreview={localPreview}
        actions={actionSet}
        onGoalChange={onGoalChange}
        onOpenTask={onOpenTask}
        onLedgerChanged={onLedgerChanged}
      />);
    });
    await act(async () => undefined);
  };
  await render();
  return { actionSet, container, onGoalChange, onLedgerChanged, onOpenTask, render, root, window };
}

function dispatch(window: Window, element: Element | null, type: string) {
  if (!element) throw new Error(`Expected element for ${type}`);
  element.dispatchEvent(new (window as unknown as TestWindow).Event(type, { bubbles: true, cancelable: true }));
}

function input(window: Window, element: Element | null, value: string) {
  if (!element) throw new Error("Expected input");
  if (element.tagName === "SELECT") {
    const option = element.querySelector<HTMLOptionElement>(`option[value="${value}"]`);
    if (!option) throw new Error(`Expected option ${value}`);
    option.selected = true;
  } else {
    (element as HTMLInputElement).value = value;
  }
  dispatch(window, element, "input");
  dispatch(window, element, "change");
}

afterEach(() => vi.unstubAllGlobals());

describe("DailyGoalPanel", () => {
  it("confirms the saved goal text with original internal whitespace", async () => {
    const actionSet = actions({ getGoal: vi.fn().mockResolvedValue(goal("2026-07-13", "Read  paper")) });
    const mounted = await mount({ actionOverrides: actionSet });
    try {
      const select = mounted.container.querySelector('select[aria-label="Choose an existing task"]');
      await act(async () => input(mounted.window as unknown as Window, select, "task-1"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector('button[aria-label="Link goal to an existing task"]'), "click"));

      expect(mounted.actionSet.confirmLink).toHaveBeenCalledWith(expect.objectContaining({
        goalText: "Read  paper",
      }));
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("keeps the newest date ledger and links when old requests resolve last", async () => {
    const oldLedger = deferred<WorkLedgerSnapshot>();
    const newLedger = deferred<WorkLedgerSnapshot>();
    const oldLinks = deferred<DailyGoalTaskLink[]>();
    const newLinks = deferred<DailyGoalTaskLink[]>();
    const loadLedger = vi.fn()
      .mockReturnValueOnce(oldLedger.promise)
      .mockReturnValueOnce(newLedger.promise);
    const listLinks = vi.fn((date: string) => date === "2026-07-12" ? oldLinks.promise : newLinks.promise);
    const mounted = await mount({
      selectedDate: "2026-07-12",
      actionOverrides: {
        getGoal: vi.fn((date: string) => Promise.resolve(goal(date, "Read paper"))),
        loadLedger,
        listLinks,
      },
    });
    try {
      await mounted.render("2026-07-13");
      const currentRowId = "goal-2026-07-13-Read%20paper-1";
      await act(async () => {
        newLedger.resolve(ledger([task("task-new", "2026-07-13", "New date task")]));
        newLinks.resolve([link("2026-07-13", currentRowId, "Read paper", "task-new")]);
      });
      expect(mounted.container.textContent).toContain("New date task");

      await act(async () => {
        oldLedger.resolve(ledger([task("task-old", "2026-07-12", "Old date task")]));
        oldLinks.resolve([link("2026-07-12", "goal-old", "Read paper", "task-old")]);
      });
      expect(mounted.container.textContent).toContain("New date task");
      expect(mounted.container.textContent).not.toContain("Old date task");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("restores an unsaved A draft after A-B-A and ignores delayed server values", async () => {
    const firstA = deferred<DailyGoalRecord>();
    const secondA = deferred<DailyGoalRecord>();
    const getGoal = vi.fn()
      .mockReturnValueOnce(firstA.promise)
      .mockResolvedValueOnce(goal("2026-07-13", "Server B"))
      .mockReturnValueOnce(secondA.promise);
    const mounted = await mount({ selectedDate: "2026-07-12", actionOverrides: { getGoal } });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#goal"), "Draft A"));
      await mounted.render("2026-07-13");
      await mounted.render("2026-07-12");
      expect(mounted.container.querySelector<HTMLTextAreaElement>("#goal")?.value).toBe("Draft A");

      await act(async () => {
        firstA.resolve(goal("2026-07-12", "Old server A"));
        secondA.resolve(goal("2026-07-12", "New server A"));
      });
      expect(mounted.container.querySelector<HTMLTextAreaElement>("#goal")?.value).toBe("Draft A");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("saves, links an existing task, and retries create-and-link with one stable task identity", async () => {
    let authoritativeLedger = ledger([task("task-existing", "2026-07-13", "Existing task")]);
    let authoritativeLinks: DailyGoalTaskLink[] = [];
    const confirmLink = vi.fn()
      .mockImplementationOnce(async (request) => ({ link: link(request.goalDate, request.goalRowId, request.goalText, request.taskId), task: task(request.taskId), taskCreated: false }))
      .mockRejectedValueOnce(new Error("transient"))
      .mockImplementationOnce(async (request) => {
        const created = task(request.taskId, request.goalDate, request.newTask.title);
        authoritativeLedger = { ...authoritativeLedger, tasks: [...authoritativeLedger.tasks, created] };
        return { link: link(request.goalDate, request.goalRowId, request.goalText, request.taskId), task: created, taskCreated: true };
      });
    const mounted = await mount({
      actionOverrides: {
        getGoal: vi.fn().mockResolvedValue(goal("2026-07-13", "Link existing\nCreate task")),
        loadLedger: vi.fn(() => Promise.resolve(structuredClone(authoritativeLedger))),
        listLinks: vi.fn(() => Promise.resolve(structuredClone(authoritativeLinks))),
        confirmLink,
      },
    });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#expected-output"), "Saved expectation"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector(".secondary-action"), "click"));
      expect(mounted.actionSet.saveGoal).toHaveBeenCalledWith(expect.objectContaining({ expectedOutput: "Saved expectation" }));

      const rows = mounted.container.querySelectorAll(".daily-goal-row");
      await act(async () => input(mounted.window as unknown as Window, rows[0].querySelector("select"), "task-existing"));
      await act(async () => dispatch(mounted.window as unknown as Window, rows[0].querySelector('button[aria-label="Link goal to an existing task"]'), "click"));
      expect(confirmLink).toHaveBeenNthCalledWith(1, expect.objectContaining({ taskId: "task-existing", newTask: undefined }));

      await act(async () => dispatch(mounted.window as unknown as Window, rows[1].querySelector('button[aria-label="Create and link a task"]'), "click"));
      const form = mounted.container.querySelector(".daily-goal-create-form")!;
      await act(async () => dispatch(mounted.window as unknown as Window, form, "submit"));
      await act(async () => dispatch(mounted.window as unknown as Window, form, "submit"));
      const firstCreate = confirmLink.mock.calls[1][0];
      const retryCreate = confirmLink.mock.calls[2][0];
      expect(retryCreate.taskId).toBe(firstCreate.taskId);
      expect(retryCreate.newTask).toEqual(firstCreate.newTask);
      expect(authoritativeLedger.tasks.filter((item) => item.id === retryCreate.taskId)).toHaveLength(1);
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("creates a deterministic real preview task and preserves its link across A-B-A", async () => {
    const mounted = await mount({ localPreview: true });
    try {
      const firstRow = mounted.container.querySelector(".daily-goal-row")!;
      const rowId = firstRow.getAttribute("data-goal-row-id")!;
      await act(async () => dispatch(mounted.window as unknown as Window, firstRow.querySelector('button[aria-label="Create and link a task"]'), "click"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector(".daily-goal-create-form"), "submit"));
      const expectedTaskId = `preview-task-${rowId}`;
      expect(mounted.onLedgerChanged.mock.calls.at(-1)?.[0].tasks).toContainEqual(expect.objectContaining({ id: expectedTaskId }));
      expect(mounted.container.querySelector(".goal-linked-task")?.textContent).not.toContain(expectedTaskId);

      await mounted.render("2026-07-14");
      await mounted.render("2026-07-13");
      expect(mounted.container.querySelector(".goal-linked-task")).not.toBeNull();
      expect(mounted.onLedgerChanged.mock.calls.at(-1)?.[0].tasks.filter((item: WorkLedgerTask) => item.id === expectedTaskId)).toHaveLength(1);
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("treats an idempotent null actual-output result as success without optimistic progress", async () => {
    const recordActualOutput = vi.fn().mockResolvedValue(null);
    const mounted = await mount({ actionOverrides: { recordActualOutput } });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#output"), "Completed output"));
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector('select[aria-label="Choose a task for actual output"]'), "task-1"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector('button[aria-label="Record actual output for task"]'), "click"));

      expect(recordActualOutput).toHaveBeenCalledWith("task-1", "2026-07-13");
      expect(mounted.container.textContent).toContain("Actual output recorded");
      expect(mounted.onOpenTask).toHaveBeenCalledWith("task-1");
      expect(mounted.onLedgerChanged.mock.calls.at(-1)?.[0].progress).toEqual([]);
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("clears the saved date draft so a later authoritative response can apply", async () => {
    const returnedA = deferred<DailyGoalRecord>();
    const getGoal = vi.fn()
      .mockResolvedValueOnce(goal("2026-07-12", "Server A"))
      .mockResolvedValueOnce(goal("2026-07-13", "Server B"))
      .mockReturnValueOnce(returnedA.promise);
    const mounted = await mount({ selectedDate: "2026-07-12", actionOverrides: { getGoal } });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#goal"), "Saved A"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector(".secondary-action"), "click"));
      await mounted.render("2026-07-13");
      await mounted.render("2026-07-12");
      expect(mounted.container.querySelector<HTMLTextAreaElement>("#goal")?.value).toBe("Saved A");

      await act(async () => returnedA.resolve(goal("2026-07-12", "Authoritative A")));
      expect(mounted.container.querySelector<HTMLTextAreaElement>("#goal")?.value).toBe("Authoritative A");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("does not run a save refresh against a newer selected date", async () => {
    const saveGoal = deferred<void>();
    const loadLedger = vi.fn()
      .mockResolvedValueOnce(ledger([task("task-a", "2026-07-12", "Date A task")]))
      .mockResolvedValueOnce(ledger([task("task-b", "2026-07-13", "Date B task")]));
    const mounted = await mount({
      selectedDate: "2026-07-12",
      actionOverrides: { saveGoal: vi.fn().mockReturnValue(saveGoal.promise), loadLedger },
    });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#goal"), "Draft A"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector(".secondary-action"), "click"));
      await mounted.render("2026-07-13");
      expect(mounted.container.textContent).toContain("Date B task");
      await act(async () => saveGoal.resolve());
      expect(loadLedger).toHaveBeenCalledTimes(2);
      expect(mounted.container.textContent).toContain("Date B task");
      expect(mounted.container.textContent).not.toContain("Date A task");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("clears an in-flight save after returning to its date before completion", async () => {
    const saveGoal = deferred<void>();
    const mounted = await mount({
      selectedDate: "2026-07-12",
      actionOverrides: { saveGoal: vi.fn().mockReturnValue(saveGoal.promise) },
    });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#goal"), "Draft A"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector(".secondary-action"), "click"));
      expect(mounted.container.querySelector<HTMLButtonElement>(".secondary-action")?.disabled).toBe(true);

      await mounted.render("2026-07-13");
      await mounted.render("2026-07-12");
      expect(mounted.container.querySelector<HTMLButtonElement>(".secondary-action")?.disabled).toBe(true);

      await act(async () => saveGoal.resolve());
      expect(mounted.container.querySelector<HTMLButtonElement>(".secondary-action")?.disabled).toBe(false);
      expect(mounted.container.textContent).toContain("Saved");
      expect(mounted.container.querySelector<HTMLTextAreaElement>("#goal")?.value).toBe("Draft A");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("publishes an in-flight link result after returning to its date before completion", async () => {
    const confirmation = deferred<ConfirmedDailyGoalTask>();
    const mounted = await mount({
      selectedDate: "2026-07-12",
      actionOverrides: {
        getGoal: vi.fn((date: string) => Promise.resolve(goal(date, "Read paper"))),
        loadLedger: vi.fn().mockResolvedValue(ledger([task("task-a", "2026-07-12", "Date A task")])),
        confirmLink: vi.fn().mockReturnValue(confirmation.promise),
      },
    });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector('select[aria-label="Choose an existing task"]'), "task-a"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector('button[aria-label="Link goal to an existing task"]'), "click"));
      await mounted.render("2026-07-13");
      await mounted.render("2026-07-12");
      expect(mounted.container.querySelector<HTMLButtonElement>(".secondary-action")?.disabled).toBe(true);

      const rowId = "goal-2026-07-12-Read%20paper-1";
      await act(async () => confirmation.resolve({
        link: link("2026-07-12", rowId, "Read paper", "task-a"),
        task: task("task-a", "2026-07-12", "Date A task"),
        taskCreated: false,
      }));
      expect(mounted.container.querySelector<HTMLButtonElement>(".secondary-action")?.disabled).toBe(false);
      expect(mounted.container.querySelector(".goal-linked-task")?.textContent).toContain("Date A task");
      expect(mounted.container.textContent).toContain("Task linked");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("does not apply a completed link or its refresh after the date owner changes", async () => {
    const confirmation = deferred<ConfirmedDailyGoalTask>();
    const loadLedger = vi.fn()
      .mockResolvedValueOnce(ledger([task("task-a", "2026-07-12", "Date A task")]))
      .mockResolvedValueOnce(ledger([task("task-b", "2026-07-13", "Date B task")]));
    const mounted = await mount({
      selectedDate: "2026-07-12",
      actionOverrides: {
        getGoal: vi.fn((date: string) => Promise.resolve(goal(date, "Read paper"))),
        loadLedger,
        confirmLink: vi.fn().mockReturnValue(confirmation.promise),
      },
    });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector('select[aria-label="Choose an existing task"]'), "task-a"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector('button[aria-label="Link goal to an existing task"]'), "click"));
      await mounted.render("2026-07-13");
      await act(async () => confirmation.resolve({ link: link("2026-07-12", "goal-a", "Read paper", "task-a"), task: task("task-a"), taskCreated: false }));
      expect(loadLedger).toHaveBeenCalledTimes(2);
      expect(mounted.onOpenTask).not.toHaveBeenCalled();
      expect(mounted.container.textContent).toContain("Date B task");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });

  it("does not apply actual-output completion or refresh after the date owner changes", async () => {
    const progress = deferred<null>();
    const loadLedger = vi.fn()
      .mockResolvedValueOnce(ledger([task("task-a", "2026-07-12", "Date A task")]))
      .mockResolvedValueOnce(ledger([task("task-b", "2026-07-13", "Date B task")]));
    const mounted = await mount({
      selectedDate: "2026-07-12",
      actionOverrides: { loadLedger, recordActualOutput: vi.fn().mockReturnValue(progress.promise) },
    });
    try {
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector("#output"), "Output A"));
      await act(async () => input(mounted.window as unknown as Window, mounted.container.querySelector('select[aria-label="Choose a task for actual output"]'), "task-a"));
      await act(async () => dispatch(mounted.window as unknown as Window, mounted.container.querySelector('button[aria-label="Record actual output for task"]'), "click"));
      await mounted.render("2026-07-13");
      await act(async () => progress.resolve(null));
      expect(loadLedger).toHaveBeenCalledTimes(2);
      expect(mounted.onOpenTask).not.toHaveBeenCalled();
      expect(mounted.container.textContent).toContain("Date B task");
    } finally {
      await act(async () => mounted.root.unmount());
    }
  });
});
