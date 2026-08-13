import type {
  EvidenceRelation,
  TrendEvidence,
  TrendMetric,
  TrendResearchAnalysis as TrendResearchAnalysisResult,
} from "../../lib/desktop";
import type { TrendLocalAnalysis } from "../../lib/trend-analysis";
import type { ActivityScope } from "../../lib/activity-composition";

interface TrendResearchAnalysisProps {
  localAnalysis: TrendLocalAnalysis;
  analysis: TrendResearchAnalysisResult | null;
  analysisStatus: "loading" | "ready" | "error";
  evidence: TrendEvidence[];
  evidenceHash: string;
  activityScope?: ActivityScope;
  analysisError?: string;
  formatDuration: (seconds: number) => string;
  onEvidenceActivate?: (evidenceId: string) => void;
}

const metricLabels: Record<TrendMetric, string> = {
  monitoredSeconds: "监测时长",
  activeSeconds: "活跃时长",
  learningSeconds: "学习时长",
  idleSeconds: "不活跃时长",
  switchCount: "切换次数",
  longestFocusSeconds: "最长专注",
  classificationCoverage: "分类覆盖",
  completedTaskCount: "完成任务",
  linkedTaskSeconds: "任务关联时长",
};

const relationLabels: Record<EvidenceRelation, string> = {
  supports: "支持",
  increased: "当前增加",
  decreased: "当前减少",
  stable: "当前稳定",
};

const durationMetrics = new Set<TrendMetric>([
  "monitoredSeconds",
  "activeSeconds",
  "learningSeconds",
  "idleSeconds",
  "longestFocusSeconds",
  "linkedTaskSeconds",
]);

function confidenceLabel(confidence: number): string {
  if (confidence >= 0.8) return "高置信度";
  if (confidence >= 0.5) return "中置信度";
  return "低置信度";
}

function evidenceValue(
  evidence: TrendEvidence,
  formatDuration: (seconds: number) => string,
): string {
  if (evidence.scope === "rate") return `${evidence.value.toLocaleString("zh-CN", { maximumFractionDigits: 2 })} 次/活跃小时`;
  if (evidence.metric) {
    return statisticValue(evidence.metric, evidence.value, formatDuration);
  }
  if (evidence.id.endsWith("classificationCoverage")) {
    return `${(evidence.value * 100).toFixed(0)}%`;
  }
  if (/dataQuality\.(classifiedSeconds|lowConfidenceSeconds|pendingSeconds)$/.test(evidence.id)) {
    return formatDuration(evidence.value);
  }
  return evidence.value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function statisticValue(
  metric: TrendMetric,
  value: number,
  formatDuration: (seconds: number) => string,
): string {
  if (durationMetrics.has(metric)) return formatDuration(value);
  if (metric === "classificationCoverage") return `${(value * 100).toFixed(0)}%`;
  return value.toLocaleString("zh-CN", { maximumFractionDigits: 2 });
}

function optionalStatistic(
  value: number | null,
  formatter: (value: number) => string,
): string {
  return value === null ? "样本不足" : formatter(value);
}

export function TrendResearchAnalysis({
  localAnalysis,
  analysis,
  analysisStatus,
  evidence,
  evidenceHash,
  activityScope = "all",
  analysisError = "",
  formatDuration,
  onEvidenceActivate,
}: TrendResearchAnalysisProps) {
  const statistics = localAnalysis.statistics;
  const formatSelectedStatistic = (value: number) => statisticValue(
    statistics.selectedMetric,
    value,
    formatDuration,
  );
  const resultScope = analysis
    ? ((analysis as TrendResearchAnalysisResult & { activityScope?: ActivityScope }).activityScope ?? "all")
    : null;
  const matchingAnalysis = analysis?.evidenceHash === evidenceHash && resultScope === activityScope
    ? analysis
    : null;
  const staleFeedback = analysisStatus === "ready" && !matchingAnalysis
    ? analysisError || (analysis ? "研究分析与当前证据不匹配，仅显示事实统计" : "")
    : "";
  const evidenceById = new Map(evidence.map((item) => [item.id, item]));
  const localObservation = statistics.effectiveActivityDayCount > 0
    ? `${metricLabels[statistics.selectedMetric]}的区间均值为 ${formatSelectedStatistic(statistics.selectedMean)}，中位数为 ${formatSelectedStatistic(statistics.selectedMedian)}；结论基于 ${statistics.effectiveActivityDayCount} 个有效活动日。`
    : "当前区间没有足够的有效活动日，暂不判断变化方向。";
  const primaryFinding = matchingAnalysis?.findings[0] ?? null;
  const fact = primaryFinding?.observation
    ? `${localObservation} ${primaryFinding.observation}`
    : localObservation;
  const hypothesis = primaryFinding?.possibleExplanation
    ?? "当前只有本地事实，没有足以支持因果解释的 AI 证据；建议继续记录完整周期。";
  const validation = primaryFinding?.validationMethod
    ?? "在下一个完整周期保持相同统计口径，再比较活跃时长、学习时长与切换次数。";
  const action = primaryFinding
    ? `先按“${primaryFinding.validationMethod}”执行一次小范围验证，并在周期结束后复盘同一组指标。`
    : "保持当前监控设置，补齐至少一个完整周期后重新分析。";

  return (
    <section className="trend-evaluation-panel trend-research-analysis" aria-labelledby="trend-research-heading">
      <header className="trend-section-heading">
        <div><h2 id="trend-research-heading">AI 趋势洞察 <small>（{activityScope === "all" ? "全部活动" : activityScope === "active" ? "活跃" : "学习"} · 基于事实）</small></h2></div>
      </header>

      {analysisStatus === "loading" && <p className="trend-analysis-feedback" role="status">正在读取研究分析...</p>}
      {analysisStatus === "error" && <p className="trend-analysis-feedback" role="status">AI 解释暂不可用，以下仍保留完整本地统计：{analysisError || "未知错误"}</p>}
      {staleFeedback && <p className="trend-analysis-feedback" role="status">{staleFeedback}</p>}

      <div className="trend-insight-grid" role="list" aria-label="证据化趋势分析">
        <article role="listitem" data-kind="fact"><strong><i>●</i>事实</strong><p>{fact}</p></article>
        <article role="listitem" data-kind="hypothesis"><strong><i>●</i>假设</strong><p>{hypothesis}</p></article>
        <article role="listitem" data-kind="validation"><strong><i>●</i>验证</strong><p>{validation}</p></article>
        <article role="listitem" data-kind="action"><strong><i>●</i>建议</strong><p>{action}</p></article>
      </div>

      {primaryFinding && (
        <div className="trend-analysis-provenance">
          <span>{confidenceLabel(primaryFinding.confidence)}</span>
          {primaryFinding.evidenceIds.map((evidenceId) => {
              const item = evidenceById.get(evidenceId);
              if (!item) return null;
              const relation = primaryFinding.claims.find((claim) => claim.evidenceId === evidenceId)?.relation ?? "supports";
              return <button type="button" className="trend-evidence-chip" data-evidence-id={evidenceId} key={evidenceId} title="定位到对应时间桶或任务" onClick={() => onEvidenceActivate?.(evidenceId)}>
                {relationLabels[relation]} · {evidenceValue(item, formatDuration)}
              </button>;
          })}
        </div>
      )}

      <details className="trend-method-details">
        <summary>方法与数据质量</summary>
        <dl className="trend-evaluation-statistics">
          <div><dt>指标</dt><dd>{metricLabels[statistics.selectedMetric]}</dd></div>
          <div><dt>均值</dt><dd>{formatSelectedStatistic(statistics.selectedMean)}</dd></div>
          <div><dt>中位数</dt><dd>{formatSelectedStatistic(statistics.selectedMedian)}</dd></div>
          <div><dt>样本标准差</dt><dd>{optionalStatistic(statistics.selectedSampleStandardDeviation, formatSelectedStatistic)}</dd></div>
          <div><dt>变异系数</dt><dd>{optionalStatistic(statistics.selectedCoefficientOfVariation, (value) => value.toFixed(2))}</dd></div>
          <div><dt>有效活动日</dt><dd>{statistics.effectiveActivityDayCount}</dd></div>
        </dl>
        {primaryFinding && primaryFinding.limitations.length > 0 && <section aria-labelledby="trend-finding-limitations-heading">
          <h3 id="trend-finding-limitations-heading">本条发现的限制</h3>
          <ul>{primaryFinding.limitations.map((limitation) => <li key={limitation}>{limitation}</li>)}</ul>
        </section>}
        {matchingAnalysis && matchingAnalysis.limitations.length > 0 && <section aria-labelledby="trend-research-limitations-heading">
          <h3 id="trend-research-limitations-heading">整体限制</h3>
          <ul>{matchingAnalysis.limitations.map((limitation) => <li key={limitation}>{limitation}</li>)}</ul>
        </section>}
        {matchingAnalysis && <dl className="trend-analysis-provenance" aria-label="研究分析来源">
          <div><dt>来源</dt><dd>{matchingAnalysis.source}</dd></div>
          <div><dt>模型</dt><dd>{matchingAnalysis.model}</dd></div>
          <div><dt>证据</dt><dd title={matchingAnalysis.evidenceHash}>{matchingAnalysis.evidenceHash.slice(0, 8)}</dd></div>
        </dl>}
      </details>
    </section>
  );
}
