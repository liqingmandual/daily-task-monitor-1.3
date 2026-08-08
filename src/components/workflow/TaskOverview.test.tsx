import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TaskTimeInsight } from "../../lib/desktop";
import { TaskOverview } from "./TaskOverview";

const insight: TaskTimeInsight = {
  summary: {
    taskId: "task-1",
    lifecycleTotalSeconds: 18_000,
    activeDayAverageSeconds: 3_600,
    naturalDayAverageSeconds: 1_800,
    activeDayCount: 5,
    naturalDayCount: 10,
    latestActivityAtMs: 10_000,
    assignmentConfidence: 0.91,
    reviewState: "confirmed",
  },
  dailyPoints: [{ date: "2026-07-25", investedSeconds: 3_600, activitySeconds: 3_000, focusSeconds: 1_800, switchCount: 3 }],
  medianDailySeconds: 3_300,
  longestContinuousSeconds: 2_400,
  focusSeconds: 9_000,
  focusShare: 0.5,
  switchesPerHour: 1.8,
  regularity: 0.8,
  manualCorrectionRate: 0.1,
  pendingReviewSeconds: 600,
  browserEvidenceCount: 2,
  progressCount: 3,
  expectedOutput: "可解释的任务质量报告",
  assessment: {
    dimensions: [
      { key: "time_investment", label: "时间投入", conclusion: "累计 5 小时", value: 18_000, unit: "seconds" },
      { key: "continuity", label: "连续性", conclusion: "最长 40 分钟", value: 2_400, unit: "seconds" },
      { key: "switching_cost", label: "切换成本", conclusion: "每小时 1.8 次", value: 1.8, unit: "switches_per_hour" },
      { key: "regularity", label: "规律性", conclusion: "规律性 80%", value: 0.8, unit: "ratio" },
      { key: "evidence_confidence", label: "证据可信度", conclusion: "归属 91%", value: 0.91, unit: "ratio" },
    ],
    dataLimitations: ["活跃日少于 7 天"],
  },
};

describe("TaskOverview", () => {
  it("presents transparent quality dimensions and a table alternative for the daily series", () => {
    const html = renderToStaticMarkup(<TaskOverview
      insight={insight}
      loading={false}
      mergeTargets={[]}
      onMerge={() => undefined}
      onExport={() => undefined}
    />);
    expect(html).toContain("生命周期总投入");
    expect(html).toContain("活跃日均");
    expect(html).toContain("时间投入");
    expect(html).toContain("证据可信度");
    expect(html).toContain("<table");
    expect(html).toContain("2026-07-25");
    expect(html).toContain("活跃日少于 7 天");
  });
});
