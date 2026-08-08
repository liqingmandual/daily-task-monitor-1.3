import { describe, expect, it } from "vitest";
import { buildDailyAnalysisEvidence, buildLocalDailyAnalysis } from "./daily-analysis";

const evidence = buildDailyAnalysisEvidence({
  date: "2026-07-12",
  goals: "完成桌面版任务监测系统的 AI 分析模块",
  expectedOutput: "通过测试并形成可见的分析面板",
  actualOutput: "完成本地分析算法",
  monitoredSeconds: 8 * 3_600,
  activeSeconds: 6 * 3_600,
  learningSeconds: 4 * 3_600,
  idleSeconds: 2 * 3_600,
  switchCount: 48,
  longestFocusSeconds: 52 * 60,
  categorySeconds: { creation_development: 3 * 3_600, research: 3_600 },
  topApps: [{ name: "ChatGPT", seconds: 3 * 3_600 }],
  browserVisitCount: 24,
  classificationCoverage: 0.91,
});

describe("daily analysis", () => {
  it("builds stable evidence hashes independent of object key order", () => {
    const { evidenceHash: _ignoredHash, ...baseEvidence } = evidence;
    const reordered = buildDailyAnalysisEvidence({
      ...baseEvidence,
      categorySeconds: { research: 3_600, creation_development: 3 * 3_600 },
      topApps: [
        { name: "Obsidian", seconds: 1_800 },
        { name: "ChatGPT", seconds: 3 * 3_600 },
      ],
    });
    const original = buildDailyAnalysisEvidence({
      ...baseEvidence,
      topApps: [
        { name: "ChatGPT", seconds: 3 * 3_600 },
        { name: "Obsidian", seconds: 1_800 },
      ],
    });

    expect(reordered.evidenceHash).toBe(original.evidenceHash);
  });

  it("describes work structure without mood, personality, or ability claims", () => {
    const result = buildLocalDailyAnalysis(evidence);

    expect(result.portrait).toContain("学习");
    expect(result.recommendation).toContain("切换");
    expect(result.source).toBe("local");
    expect(result.evidenceHash).toBe(evidence.evidenceHash);
    expect(`${result.portrait}${result.recommendation}`).not.toMatch(/心情|人格|性格|能力不足|聪明|焦虑|抑郁/);
  });
});
