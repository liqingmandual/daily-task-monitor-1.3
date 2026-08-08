export interface DailyAnalysisEvidence {
  date: string;
  goals: string;
  expectedOutput: string;
  actualOutput: string;
  monitoredSeconds: number;
  activeSeconds: number;
  learningSeconds: number;
  idleSeconds: number;
  switchCount: number;
  longestFocusSeconds: number;
  categorySeconds: Record<string, number>;
  topApps: Array<{ name: string; seconds: number }>;
  browserVisitCount: number;
  classificationCoverage: number;
  evidenceHash: string;
}

export interface DailyAnalysisResult {
  portrait: string;
  recommendation: string;
  findings?: EvidenceBasedFinding[];
  protocolVersion?: number;
  source: "local" | "ai" | "queued";
  evidenceHash: string;
  generatedAtMs: number;
}

export interface EvidenceBasedFinding {
  observation: string;
  hypothesis: string;
  validation: string;
  action: string;
  evidenceIds: string[];
  limitations: string[];
  confidence: number;
}

type EvidenceInput = Omit<DailyAnalysisEvidence, "evidenceHash"> & { evidenceHash?: string };

function stableEvidenceJson(evidence: EvidenceInput): string {
  const categories = Object.fromEntries(
    Object.entries(evidence.categorySeconds)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => [key, Math.max(0, Math.round(value))]),
  );
  return JSON.stringify({
    date: evidence.date,
    goals: evidence.goals.trim(),
    expectedOutput: evidence.expectedOutput.trim(),
    actualOutput: evidence.actualOutput.trim(),
    monitoredSeconds: Math.max(0, Math.round(evidence.monitoredSeconds)),
    activeSeconds: Math.max(0, Math.round(evidence.activeSeconds)),
    learningSeconds: Math.max(0, Math.round(evidence.learningSeconds)),
    idleSeconds: Math.max(0, Math.round(evidence.idleSeconds)),
    switchCount: Math.max(0, Math.round(evidence.switchCount)),
    longestFocusSeconds: Math.max(0, Math.round(evidence.longestFocusSeconds)),
    categorySeconds: categories,
    topApps: evidence.topApps
      .map((app) => ({ name: app.name.trim(), seconds: Math.max(0, Math.round(app.seconds)) }))
      .sort((left, right) => right.seconds - left.seconds || left.name.localeCompare(right.name)),
    browserVisitCount: Math.max(0, Math.round(evidence.browserVisitCount)),
    classificationCoverage: Math.max(0, Math.min(1, evidence.classificationCoverage)),
  });
}

function fnv1a(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function buildDailyAnalysisEvidence(input: EvidenceInput): DailyAnalysisEvidence {
  const normalized = JSON.parse(stableEvidenceJson(input)) as Omit<DailyAnalysisEvidence, "evidenceHash">;
  return { ...normalized, evidenceHash: input.evidenceHash || fnv1a(stableEvidenceJson(input)) };
}

function percent(part: number, whole: number): number {
  return Math.round((Math.max(0, part) / Math.max(1, whole)) * 100);
}

function duration(seconds: number): string {
  const minutes = Math.max(0, Math.round(seconds / 60));
  const hours = Math.floor(minutes / 60);
  return hours ? `${hours} 小时 ${minutes % 60} 分钟` : `${minutes} 分钟`;
}

export function buildLocalDailyAnalysis(evidence: DailyAnalysisEvidence): DailyAnalysisResult {
  const learningShare = percent(evidence.learningSeconds, evidence.activeSeconds);
  const idleShare = percent(evidence.idleSeconds, evidence.monitoredSeconds);
  const creationSeconds = evidence.categorySeconds.creation_development ?? 0;
  const creationShare = percent(creationSeconds, evidence.activeSeconds);
  const topApp = evidence.topApps[0];
  const switchesPerHour = evidence.activeSeconds
    ? evidence.switchCount / (evidence.activeSeconds / 3_600)
    : 0;
  const goalContext = evidence.goals.trim() ? `今日目标是“${evidence.goals.trim()}”。` : "今日尚未填写明确目标。";
  const appContext = topApp ? `主要应用为 ${topApp.name}（${duration(topApp.seconds)}）。` : "应用记录仍不足。";
  const portrait = `${goalContext}共记录 ${duration(evidence.activeSeconds)} 活跃时间，其中学习活动占 ${learningShare}%，创作开发占 ${creationShare}%。${appContext}`;

  const recommendations: string[] = [];
  if (switchesPerHour >= 12) recommendations.push(`当前每小时约切换 ${switchesPerHour.toFixed(1)} 次，可安排一段连续任务以降低切换负荷`);
  else recommendations.push(`当前每小时约切换 ${switchesPerHour.toFixed(1)} 次，继续保留较长的连续任务段`);
  if (idleShare >= 30) recommendations.push(`不活跃时间占总监测 ${idleShare}%，复盘时应区分无输入、监控断档和历史推断`);
  if (evidence.classificationCoverage < 0.8) recommendations.push(`分类覆盖率仅 ${percent(evidence.classificationCoverage, 1)}%，建议优先复核低置信记录`);
  if (!evidence.actualOutput.trim()) recommendations.push("补写一条可核验的实际产出，使目标与时间投入可以对照");
  else recommendations.push(`已记录实际产出“${evidence.actualOutput.trim()}”，可继续核对它与目标的完成程度`);

  return {
    portrait,
    recommendation: `${recommendations.join("；")}。`,
    findings: [
      {
        observation: `今日记录 ${duration(evidence.activeSeconds)} 活跃时间，学习活动占 ${learningShare}%，创作开发占 ${creationShare}%。`,
        hypothesis: evidence.activeSeconds
          ? "这些构成可能反映今天在输入学习与产出活动之间的实际分配。"
          : "可能是当天尚未形成可分析的活跃记录，而不是没有投入。",
        validation: "与前 7 天及同星期的活动构成比较，并核对当天目标与实际产出。",
        action: "明天保留一个目标明确的连续任务段，并在结束时记录可核验产出。",
        evidenceIds: ["metric:active_seconds", "metric:learning_seconds", "metric:creation_seconds"],
        limitations: ["本地统计只能说明时间构成，不能单独证明原因或产出质量。"],
        confidence: evidence.activeSeconds ? 0.78 : 0.35,
      },
      {
        observation: `活跃期间每小时约切换 ${switchesPerHour.toFixed(1)} 次，分类覆盖率为 ${percent(evidence.classificationCoverage, 1)}%。`,
        hypothesis: switchesPerHour >= 12
          ? "较高的切换频率可能与任务被打断或并行查找资料有关。"
          : "当前切换频率可能允许形成较稳定的连续工作片段。",
        validation: "选择一个相似任务做 45 分钟单任务实验，比较切换次数、完成产出和主观阻力。",
        action: switchesPerHour >= 12
          ? "下一次工作前关闭无关窗口，并把临时查找项记录到待办而非立即切换。"
          : "继续保留较长任务段，并记录任务结束时是否产生预期产出。",
        evidenceIds: ["metric:switch_count", "metric:classification_coverage"],
        limitations: ["窗口切换可能是任务本身需要，不能直接等同于注意力分散。"],
        confidence: evidence.classificationCoverage >= 0.8 ? 0.72 : 0.52,
      },
    ],
    protocolVersion: 2,
    source: "local",
    evidenceHash: evidence.evidenceHash,
    generatedAtMs: Date.now(),
  };
}
