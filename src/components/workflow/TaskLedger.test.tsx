import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TaskTimeSummary, WorkLedgerTask } from "../../lib/desktop";
import { TaskLedger } from "./TaskLedger";

const task: WorkLedgerTask = {
  id: "task-1",
  projectId: "project-1",
  title: "任务时间洞察",
  status: "in_progress",
  priority: "high",
  expectedOutput: "可解释的质量报告",
  dueDate: null,
  createdAtMs: 1,
  updatedAtMs: 2,
  completedAtMs: null,
};

const summary: TaskTimeSummary = {
  taskId: task.id,
  lifecycleTotalSeconds: 18_000,
  activeDayAverageSeconds: 3_600,
  naturalDayAverageSeconds: 1_800,
  activeDayCount: 5,
  naturalDayCount: 10,
  latestActivityAtMs: 10_000,
  assignmentConfidence: 0.91,
  reviewState: "confirmed",
};

describe("TaskLedger", () => {
  it("shows lifecycle time, active-day average, recent activity, and assignment confidence", () => {
    const html = renderToStaticMarkup(<TaskLedger
      projectName="项目"
      tasks={[task]}
      taskSummaries={[summary]}
      linkedEvidence={[]}
      selectedTaskId={task.id}
      onSelect={() => undefined}
      onCreate={() => undefined}
      onEdit={() => undefined}
      onComplete={() => undefined}
    />);
    expect(html).toContain("累计 5 小时");
    expect(html).toContain("日均 1 小时");
    expect(html).toContain("归属 91%");
    expect(html).toContain("最近活动");
  });
});
