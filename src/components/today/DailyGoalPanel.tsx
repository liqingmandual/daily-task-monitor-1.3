import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Link2, Plus } from "lucide-react";
import {
  buildDailyGoalRows,
  confirmDailyGoalTask,
  dayBounds,
  getDailyGoal,
  listDailyGoalTaskLinks,
  loadWorkLedger,
  recordDailyActualOutputProgress,
  saveDailyGoal,
  type ConfirmedDailyGoalTask,
  type DailyGoalRecord,
  type DailyGoalRow,
  type DailyGoalTaskLink,
  type WorkLedgerProject,
  type WorkLedgerSnapshot,
  type WorkLedgerTask,
  type WorkLedgerTaskPriority,
} from "../../lib/desktop";
import { PanelHeading } from "./analysis-shared";

const previewGoal: DailyGoalRecord = {
  date: "",
  goals: "Review the work ledger\nWrite a focused implementation note",
  expectedOutput: "A verified daily task slice",
  actualOutput: "",
};

export type GoalActions = {
  getGoal: typeof getDailyGoal;
  saveGoal: typeof saveDailyGoal;
  listLinks: typeof listDailyGoalTaskLinks;
  loadLedger: typeof loadWorkLedger;
  confirmLink: typeof confirmDailyGoalTask;
  recordActualOutput: typeof recordDailyActualOutputProgress;
};

const defaultActions: GoalActions = {
  getGoal: getDailyGoal,
  saveGoal: saveDailyGoal,
  listLinks: listDailyGoalTaskLinks,
  loadLedger: loadWorkLedger,
  confirmLink: confirmDailyGoalTask,
  recordActualOutput: recordDailyActualOutputProgress,
};

type NewTaskDraft = {
  id: string;
  projectId: string;
  title: string;
  priority: WorkLedgerTaskPriority;
};

type DateViewState = {
  goal: DailyGoalRecord;
  persistedGoal: DailyGoalRecord | null;
  dirty: boolean;
  links: DailyGoalTaskLink[];
  ledger: WorkLedgerSnapshot | null;
  actualTaskId: string;
  existingTaskIds: Record<string, string>;
  newTaskRows: Record<string, NewTaskDraft>;
  pending: Set<string>;
  message: string;
};

type ViewOwner = { date: string; version: number };

function emptyGoal(date: string): DailyGoalRecord {
  return { date, goals: "", expectedOutput: "", actualOutput: "" };
}

function previewLedger(date: string): WorkLedgerSnapshot {
  const project: WorkLedgerProject = { id: "preview-project", name: "Desktop rewrite", color: "#2563eb", status: "active", description: "Local preview", createdAtMs: 0, updatedAtMs: 0, archivedAtMs: null };
  const task: WorkLedgerTask = { id: "preview-task", projectId: project.id, title: "Verify daily goal bridge", status: "in_progress", priority: "high", expectedOutput: "A linked and reviewable goal", dueDate: date, createdAtMs: 0, updatedAtMs: 0, completedAtMs: null };
  return { projects: [project], tasks: [task], progress: [], linkedEvidence: [], unassignedEvidence: [], suggestions: [], ambiguousEvidenceHashes: [], summary: { projectCount: 1, taskCount: 1, progressCount: 0, linkedEvidenceCount: 0, unassignedEvidenceCount: 0, localSuggestionCount: 0, ambiguousEvidenceCount: 0 } };
}

function initialDateState(date: string, localPreview: boolean): DateViewState {
  const initialGoal = localPreview ? { ...previewGoal, date } : emptyGoal(date);
  return {
    goal: initialGoal,
    persistedGoal: localPreview ? initialGoal : null,
    dirty: false,
    links: [],
    ledger: localPreview ? previewLedger(date) : null,
    actualTaskId: "",
    existingTaskIds: {},
    newTaskRows: {},
    pending: new Set(),
    message: "",
  };
}

function deterministicPreviewTaskId(rowId: string): string {
  return `preview-task-${rowId}`;
}

function desktopTaskId(): string {
  return `goal-task-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`;
}

export function DailyGoalPanel({
  selectedDate,
  localPreview,
  actions = defaultActions,
  onGoalChange,
  onOpenTask,
  onLedgerChanged,
}: {
  selectedDate: string;
  localPreview: boolean;
  actions?: GoalActions;
  onGoalChange: (goal: DailyGoalRecord) => void;
  onOpenTask: (taskId: string) => void;
  onLedgerChanged: (snapshot: WorkLedgerSnapshot) => void;
}) {
  const cacheRef = useRef(new Map<string, DateViewState>());
  const ownerRef = useRef<ViewOwner>({ date: selectedDate, version: 0 });
  if (ownerRef.current.date !== selectedDate) {
    ownerRef.current = { date: selectedDate, version: ownerRef.current.version + 1 };
  }
  const currentOwner = ownerRef.current;
  const getDateState = (date: string) => {
    const existing = cacheRef.current.get(date);
    if (existing) return existing;
    const created = initialDateState(date, localPreview);
    cacheRef.current.set(date, created);
    return created;
  };
  const [view, setView] = useState<DateViewState>(() => getDateState(selectedDate));
  const goalRequestRef = useRef(0);
  const ledgerRequestRef = useRef(0);
  const linksRequestRef = useRef(0);
  const pendingRef = useRef(new Map<string, Set<string>>());
  const callbacksRef = useRef({ onGoalChange, onOpenTask, onLedgerChanged });
  callbacksRef.current = { onGoalChange, onOpenTask, onLedgerChanged };

  const owns = (owner: ViewOwner) => owner.date === ownerRef.current.date && owner.version === ownerRef.current.version;

  const updateDateState = (date: string, update: (current: DateViewState) => DateViewState, owner?: ViewOwner) => {
    const next = update(getDateState(date));
    cacheRef.current.set(date, next);
    if ((!owner && ownerRef.current.date === date) || (owner && owns(owner))) setView(next);
    return next;
  };

  const reconcileMutationState = (owner: ViewOwner, update: (current: DateViewState) => DateViewState) => {
    const next = update(getDateState(owner.date));
    cacheRef.current.set(owner.date, next);
    if (ownerRef.current.date === owner.date) setView(next);
    return next;
  };

  const setMessage = (owner: ViewOwner, message: string) => {
    updateDateState(owner.date, (current) => ({ ...current, message }), owner);
  };

  const setMutationMessage = (owner: ViewOwner, message: string) => {
    reconcileMutationState(owner, (current) => ({ ...current, message }));
  };

  const beginOperation = (owner: ViewOwner, operation: string) => {
    const datePending = pendingRef.current.get(owner.date) ?? new Set<string>();
    if (datePending.has(operation)) return false;
    datePending.add(operation);
    pendingRef.current.set(owner.date, datePending);
    updateDateState(owner.date, (current) => ({ ...current, pending: new Set(datePending), message: "" }), owner);
    return true;
  };

  const endOperation = (owner: ViewOwner, operation: string) => {
    const datePending = pendingRef.current.get(owner.date) ?? new Set<string>();
    datePending.delete(operation);
    reconcileMutationState(owner, (current) => ({ ...current, pending: new Set(datePending) }));
  };

  const loadGoalFor = async (owner: ViewOwner) => {
    const request = ++goalRequestRef.current;
    const next = await actions.getGoal(owner.date);
    if (request !== goalRequestRef.current || !owns(owner)) return false;
    const current = getDateState(owner.date);
    if (current.dirty) return false;
    updateDateState(owner.date, (state) => ({ ...state, goal: next, persistedGoal: next, dirty: false }), owner);
    callbacksRef.current.onGoalChange(next);
    return true;
  };

  const loadLedgerFor = async (owner: ViewOwner) => {
    const request = ++ledgerRequestRef.current;
    const { startMs, endMs } = dayBounds(owner.date);
    const next = await actions.loadLedger(startMs, endMs);
    if (request !== ledgerRequestRef.current || !owns(owner)) return false;
    updateDateState(owner.date, (state) => ({ ...state, ledger: next }), owner);
    callbacksRef.current.onLedgerChanged(next);
    return true;
  };

  const loadLinksFor = async (owner: ViewOwner) => {
    const request = ++linksRequestRef.current;
    const next = await actions.listLinks(owner.date);
    if (request !== linksRequestRef.current || !owns(owner)) return false;
    updateDateState(owner.date, (state) => ({ ...state, links: next }), owner);
    return true;
  };

  useEffect(() => {
    const owner = { ...currentOwner };
    const cached = getDateState(owner.date);
    setView(cached);
    callbacksRef.current.onGoalChange(cached.goal);
    if (localPreview) {
      if (cached.ledger) callbacksRef.current.onLedgerChanged(cached.ledger);
      return;
    }
    void loadGoalFor(owner).catch((error: unknown) => owns(owner) && setMessage(owner, String(error)));
    void loadLedgerFor(owner).catch((error: unknown) => owns(owner) && setMessage(owner, String(error)));
    void loadLinksFor(owner).catch((error: unknown) => owns(owner) && setMessage(owner, String(error)));
  }, [actions, currentOwner.date, currentOwner.version, localPreview]);

  const rows = useMemo(
    () => buildDailyGoalRows(selectedDate, view.persistedGoal?.goals ?? ""),
    [selectedDate, view.persistedGoal?.goals],
  );
  const linkedTaskIds = new Set(view.links.map((link) => link.taskId));
  const tasks = view.ledger?.tasks ?? [];
  const projects = view.ledger?.projects.filter((project) => project.status === "active") ?? [];
  const anyPending = view.pending.size > 0;

  const updateGoal = (patch: Partial<DailyGoalRecord>) => {
    const nextGoal = { ...getDateState(selectedDate).goal, ...patch, date: selectedDate };
    updateDateState(selectedDate, (current) => ({ ...current, goal: nextGoal, dirty: true }));
    callbacksRef.current.onGoalChange(nextGoal);
  };

  const saveFor = async (owner: ViewOwner, record: DailyGoalRecord) => {
    if (!beginOperation(owner, "save")) return false;
    try {
      if (!localPreview) await actions.saveGoal(record);
      reconcileMutationState(owner, (current) => ({ ...current, goal: record, persistedGoal: record, dirty: false }));
      if (ownerRef.current.date === owner.date) {
        callbacksRef.current.onGoalChange(record);
        if (!localPreview && owns(owner)) await Promise.all([loadLedgerFor(owner), loadLinksFor(owner)]);
        setMutationMessage(owner, "Saved");
      }
      return true;
    } catch (error) {
      if (ownerRef.current.date === owner.date) setMutationMessage(owner, String(error));
      return false;
    } finally {
      endOperation(owner, "save");
    }
  };

  const save = () => {
    const owner = { ...ownerRef.current };
    return saveFor(owner, getDateState(owner.date).goal);
  };

  const previewConfirmation = (
    owner: ViewOwner,
    row: DailyGoalRow,
    taskId: string,
    create?: Omit<NewTaskDraft, "id">,
  ): ConfirmedDailyGoalTask => {
    const current = getDateState(owner.date);
    const existingLink = current.links.find((item) => item.goalRowId === row.id);
    const existingTask = current.ledger?.tasks.find((item) => item.id === (existingLink?.taskId ?? taskId));
    if (existingLink && existingTask) return { link: existingLink, task: existingTask, taskCreated: false };
    let confirmedTask = existingTask;
    let taskCreated = false;
    if (!confirmedTask && create) {
      const timestamp = dayBounds(owner.date).startMs;
      confirmedTask = {
        id: taskId,
        projectId: create.projectId,
        title: create.title,
        status: "todo",
        priority: create.priority,
        expectedOutput: current.goal.expectedOutput,
        dueDate: owner.date,
        createdAtMs: timestamp,
        updatedAtMs: timestamp,
        completedAtMs: null,
      };
      taskCreated = true;
    }
    if (!confirmedTask) throw new Error("Preview task does not exist");
    return {
      link: { goalRowId: row.id, goalDate: owner.date, goalText: row.goalText, taskId: confirmedTask.id, confirmedAtMs: dayBounds(owner.date).startMs },
      task: confirmedTask,
      taskCreated,
    };
  };

  const confirm = async (row: DailyGoalRow, taskId: string, create?: Omit<NewTaskDraft, "id">) => {
    const owner = { ...ownerRef.current };
    const operation = `link:${row.id}`;
    if (!taskId || !beginOperation(owner, operation)) return;
    try {
      const confirmed = localPreview
        ? previewConfirmation(owner, row, taskId, create)
        : await actions.confirmLink({
          goalRowId: row.id,
          goalDate: owner.date,
          goalText: row.goalText,
          taskId,
          newTask: create ? { ...create, expectedOutput: getDateState(owner.date).goal.expectedOutput, dueDate: owner.date } : undefined,
        });
      const next = reconcileMutationState(owner, (current) => {
        const tasks = current.ledger?.tasks ?? [];
        const nextTasks = confirmed.task && !tasks.some((item) => item.id === confirmed.task.id)
          ? [...tasks, confirmed.task]
          : tasks;
        const nextLedger = current.ledger ? {
          ...current.ledger,
          tasks: nextTasks,
          summary: { ...current.ledger.summary, taskCount: nextTasks.length },
        } : current.ledger;
        const newTaskRows = { ...current.newTaskRows };
        delete newTaskRows[row.id];
        return {
          ...current,
          ledger: nextLedger,
          links: [...current.links.filter((item) => item.goalRowId !== row.id), confirmed.link],
          newTaskRows,
        };
      });
      if (ownerRef.current.date === owner.date) {
        if ((localPreview || !owns(owner)) && next.ledger) callbacksRef.current.onLedgerChanged(next.ledger);
        else if (!localPreview) await loadLedgerFor(owner);
        setMutationMessage(owner, confirmed.taskCreated ? "Task created and linked" : "Task linked");
        callbacksRef.current.onOpenTask(confirmed.task.id);
      }
    } catch (error) {
      if (ownerRef.current.date === owner.date) setMutationMessage(owner, String(error));
    } finally {
      endOperation(owner, operation);
    }
  };

  const recordActualOutput = async () => {
    const owner = { ...ownerRef.current };
    const current = getDateState(owner.date);
    const taskId = current.actualTaskId;
    if (!taskId || !current.goal.actualOutput.trim() || !beginOperation(owner, "actual-output")) return;
    try {
      if (!localPreview) await actions.saveGoal(current.goal);
      reconcileMutationState(owner, (state) => ({ ...state, persistedGoal: current.goal, dirty: false }));
      if (!localPreview) await actions.recordActualOutput(taskId, owner.date);
      if (ownerRef.current.date === owner.date) {
        if (!localPreview && owns(owner)) await loadLedgerFor(owner);
        setMutationMessage(owner, "Actual output recorded");
        callbacksRef.current.onOpenTask(taskId);
      }
    } catch (error) {
      if (ownerRef.current.date === owner.date) setMutationMessage(owner, String(error));
    } finally {
      endOperation(owner, "actual-output");
    }
  };

  return <article className="panel goal-card daily-goal-panel">
    <PanelHeading eyebrow="TODAY GOAL" title="今日目标" icon={<Check size={18} />} />
    <label htmlFor="goal">今日目标</label>
    <textarea id="goal" value={view.goal.goals} onInput={(event) => updateGoal({ goals: event.currentTarget.value })} />
    <label htmlFor="expected-output">预期产出</label>
    <textarea id="expected-output" value={view.goal.expectedOutput} onInput={(event) => updateGoal({ expectedOutput: event.currentTarget.value })} />
    <label htmlFor="output">实际产出</label>
    <textarea id="output" value={view.goal.actualOutput} onInput={(event) => updateGoal({ actualOutput: event.currentTarget.value })} />
    <button className="secondary-action" onClick={() => void save()} disabled={anyPending}>保存目标</button>
    {view.message && <p className="goal-feedback" role={/error|fail/i.test(view.message) ? "alert" : "status"}>{view.message}</p>}
    {rows.length > 0 && <section className="daily-goal-links" aria-label="Saved goal task links">
      {rows.map((row) => {
        const linked = view.links.find((item) => item.goalRowId === row.id);
        const draft = view.newTaskRows[row.id];
        return <div className="daily-goal-row" data-goal-row-id={row.id} key={row.id}>
          <strong>{row.text}</strong>
          {linked ? <button type="button" className="goal-linked-task" onClick={() => callbacksRef.current.onOpenTask(linked.taskId)}><Link2 size={14} />{tasks.find((task) => task.id === linked.taskId)?.title ?? linked.taskId}</button> : <>
            <div className="daily-goal-link-controls">
              <select aria-label="Choose an existing task" value={view.existingTaskIds[row.id] ?? ""} disabled={anyPending} onChange={(event) => updateDateState(selectedDate, (current) => ({ ...current, existingTaskIds: { ...current.existingTaskIds, [row.id]: event.target.value } }))}>
                <option value="">Link existing task</option>
                {tasks.map((task) => <option key={task.id} value={task.id}>{linkedTaskIds.has(task.id) ? "Linked: " : ""}{task.title}</option>)}
              </select>
              <button type="button" aria-label="Link goal to an existing task" title="Link existing task" disabled={anyPending || !view.existingTaskIds[row.id]} onClick={() => void confirm(row, view.existingTaskIds[row.id])}><Link2 size={15} /></button>
              <button type="button" aria-label="Create and link a task" title="Create and link task" disabled={anyPending || !projects.length} onClick={() => updateDateState(selectedDate, (current) => current.newTaskRows[row.id] ? current : { ...current, newTaskRows: { ...current.newTaskRows, [row.id]: { id: localPreview ? deterministicPreviewTaskId(row.id) : desktopTaskId(), projectId: projects[0]?.id ?? "", title: row.goalText, priority: "medium" } } })}><Plus size={15} /></button>
            </div>
            {draft && <form className="daily-goal-create-form" onSubmit={(event) => { event.preventDefault(); void confirm(row, draft.id, { projectId: draft.projectId, title: draft.title, priority: draft.priority }); }}>
              <select aria-label="Project for new linked task" value={draft.projectId} onChange={(event) => updateDateState(selectedDate, (current) => ({ ...current, newTaskRows: { ...current.newTaskRows, [row.id]: { ...draft, projectId: event.target.value } } }))}>{projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}</select>
              <input aria-label="New linked task title" value={draft.title} onInput={(event) => updateDateState(selectedDate, (current) => ({ ...current, newTaskRows: { ...current.newTaskRows, [row.id]: { ...draft, title: event.currentTarget.value } } }))} required />
              <select aria-label="New linked task priority" value={draft.priority} onChange={(event) => updateDateState(selectedDate, (current) => ({ ...current, newTaskRows: { ...current.newTaskRows, [row.id]: { ...draft, priority: event.target.value as WorkLedgerTaskPriority } } }))}><option value="low">Low</option><option value="medium">Medium</option><option value="high">High</option><option value="urgent">Urgent</option></select>
              <button type="submit" disabled={anyPending || !draft.projectId || !draft.title.trim()}>Create and link</button>
            </form>}
          </>}
        </div>;
      })}
    </section>}
    <section className="daily-actual-output" aria-label="Actual output progress">
      <select aria-label="Choose a task for actual output" value={view.actualTaskId} onChange={(event) => updateDateState(selectedDate, (current) => ({ ...current, actualTaskId: event.target.value }))} disabled={!tasks.length || anyPending}>
        <option value="">Choose task</option>
        {tasks.map((task) => <option key={task.id} value={task.id}>{task.title}</option>)}
      </select>
      <button type="button" aria-label="Record actual output for task" disabled={!view.actualTaskId || !view.goal.actualOutput.trim() || anyPending} onClick={() => void recordActualOutput()}>Record output</button>
    </section>
  </article>;
}
