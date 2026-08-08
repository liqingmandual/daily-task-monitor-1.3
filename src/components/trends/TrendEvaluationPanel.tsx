import { RefreshCcw } from "lucide-react";
import type { TrendAnalysisResult } from "../../lib/desktop";
import type { TrendLocalAnalysis } from "../../lib/trend-analysis";

interface TrendEvaluationPanelProps {
  analysis: TrendLocalAnalysis;
  persistedAnalysis?: TrendAnalysisResult | null;
  analysisStatus?: "loading" | "ready" | "error";
  analysisError?: string;
  evidenceHash: string;
  localConfidence: number;
  nativeActionsAvailable?: boolean;
  reanalysisPending?: boolean;
  reanalysisFeedback?: string;
  reanalysisFeedbackStatus?: "idle" | "pending" | "success" | "error";
  onReanalyze?: () => void;
  formatDuration: (seconds: number) => string;
}

function formatOptionalDuration(value: number | null, formatDuration: (seconds: number) => string): string {
  return value === null ? "样本不足" : formatDuration(value);
}

function formatDelta(value: number | null): string {
  if (value === null) return "无可比基数";
  return `${value > 0 ? "+" : ""}${value.toFixed(1)}%`;
}

export function TrendEvaluationPanel({
  analysis,
  persistedAnalysis = null,
  analysisStatus = "loading",
  analysisError = "",
  evidenceHash,
  localConfidence,
  nativeActionsAvailable = true,
  reanalysisPending = false,
  reanalysisFeedback = "",
  reanalysisFeedbackStatus = "idle",
  onReanalyze = () => undefined,
  formatDuration,
}: TrendEvaluationPanelProps) {
  const stats = analysis.statistics;
  const matchingAnalysis = persistedAnalysis?.evidenceHash === evidenceHash ? persistedAnalysis : null;
  const mismatchedAnalysis = Boolean(persistedAnalysis && !matchingAnalysis);
  const supplementalFeedback = mismatchedAnalysis || (analysisStatus === "ready" && !matchingAnalysis && analysisError)
    ? "补充分析与当前证据不匹配，继续使用本地统计"
    : analysisStatus === "error"
      ? `补充分析读取失败：${analysisError || "未知错误"}`
      : "";
  const localObservationText = new Set(analysis.observations.map((item) => item.text));
  const localSuggestionText = new Set(analysis.suggestions);
  const supplementalObservations = matchingAnalysis?.observations.filter((item) => !localObservationText.has(item)) ?? [];
  const supplementalSuggestions = matchingAnalysis?.suggestions.filter((item) => !localSuggestionText.has(item)) ?? [];
  return (
    <section className="trend-evaluation-panel" aria-labelledby="trend-evaluation-heading">
      <header className="trend-section-heading">
        <div><span>EVALUATION</span><h2 id="trend-evaluation-heading">趋势评价</h2></div>
        <div className="trend-evaluation-actions">
          <dl className="trend-analysis-provenance" aria-label="趋势评价来源">
            <div><dt>来源</dt><dd>本地统计</dd></div>
            <div><dt>模型</dt><dd>deterministic-v1</dd></div>
            <div><dt>置信度</dt><dd>{Math.round(localConfidence * 100)}%</dd></div>
            <div><dt>证据</dt><dd title={evidenceHash}>{evidenceHash.slice(0, 8)}</dd></div>
          </dl>
          <button
            type="button"
            className="icon-button"
            aria-label="重新分析"
            title="重新分析"
            disabled={!nativeActionsAvailable || reanalysisPending}
            onClick={onReanalyze}
          ><RefreshCcw size={17} /></button>
        </div>
      </header>

      {reanalysisFeedback && <p
        className={`trend-action-feedback ${reanalysisFeedbackStatus}`}
        role={reanalysisFeedbackStatus === "error" ? "alert" : "status"}
      >{reanalysisFeedback}</p>}

      {supplementalFeedback && <p className="trend-analysis-feedback" role={analysisStatus === "error" ? "alert" : "status"}>{supplementalFeedback}</p>}

      <section aria-labelledby="trend-observations-heading">
        <h3 id="trend-observations-heading">观察到的变化</h3>
        <ul>{analysis.observations.map((item) => (
          <li key={item.text} data-evidence-keys={item.evidenceKeys.join(" ")}>{item.text}</li>
        ))}</ul>
      </section>

      <section aria-labelledby="trend-statistics-heading">
        <h3 id="trend-statistics-heading">统计依据</h3>
        <dl className="trend-evaluation-statistics">
          <div data-evidence-key="effectiveActivityDayCount"><dt>有效活动日</dt><dd>{stats.effectiveActivityDayCount} 天</dd></div>
          <div data-evidence-key="minimumEffectiveActivityDayCount"><dt>最小判断样本</dt><dd>{stats.minimumEffectiveActivityDayCount} 天</dd></div>
          <div><dt>活跃均值</dt><dd>{formatDuration(stats.activeMeanSeconds)}</dd></div>
          <div><dt>活跃中位数</dt><dd>{formatDuration(stats.activeMedianSeconds)}</dd></div>
          <div><dt>活跃样本标准差</dt><dd>{formatOptionalDuration(stats.activeSampleStandardDeviationSeconds, formatDuration)}</dd></div>
          <div data-evidence-key="activeCoefficientOfVariation"><dt>活跃变异系数</dt><dd>{stats.activeCoefficientOfVariation?.toFixed(2) ?? "样本不足"}</dd></div>
          <div><dt>学习均值</dt><dd>{formatDuration(stats.learningMeanSeconds)}</dd></div>
          <div><dt>学习中位数</dt><dd>{formatDuration(stats.learningMedianSeconds)}</dd></div>
          <div><dt>学习样本标准差</dt><dd>{formatOptionalDuration(stats.learningSampleStandardDeviationSeconds, formatDuration)}</dd></div>
          <div><dt>学习变异系数</dt><dd>{stats.learningCoefficientOfVariation?.toFixed(2) ?? "样本不足"}</dd></div>
          <div><dt>专注样本标准差</dt><dd>{formatOptionalDuration(stats.focusSampleStandardDeviationSeconds, formatDuration)}</dd></div>
          <div data-evidence-key="focusCoefficientOfVariation"><dt>专注变异系数</dt><dd>{stats.focusCoefficientOfVariation?.toFixed(2) ?? "样本不足"}</dd></div>
          <div data-evidence-key="learningDeltaPercent"><dt>学习变化</dt><dd>{formatDelta(stats.learningDeltaPercent)}</dd></div>
          <div data-evidence-key="activeDeltaPercent"><dt>活跃变化</dt><dd>{formatDelta(stats.activeDeltaPercent)}</dd></div>
          <div data-evidence-key="switchingDeltaPercent"><dt>切换变化</dt><dd>{formatDelta(stats.switchingDeltaPercent)}</dd></div>
          <div data-evidence-key="directionThresholdPercent"><dt>方向判断阈值</dt><dd>{stats.directionThresholdPercent}%</dd></div>
          <div data-evidence-key="classificationCoverage"><dt>分类覆盖</dt><dd>{(stats.classificationCoverage * 100).toFixed(0)}%</dd></div>
          <div><dt>最佳有效活动日</dt><dd>{stats.bestEffectiveActivityDay ? `${stats.bestEffectiveActivityDay.date} · ${formatDuration(stats.bestEffectiveActivityDay.activeSeconds)}` : "暂无"}</dd></div>
          <div><dt>最弱有效活动日</dt><dd>{stats.weakestEffectiveActivityDay ? `${stats.weakestEffectiveActivityDay.date} · ${formatDuration(stats.weakestEffectiveActivityDay.activeSeconds)}` : "暂无"}</dd></div>
        </dl>
      </section>

      <section aria-labelledby="trend-suggestions-heading">
        <h3 id="trend-suggestions-heading">下阶段建议</h3>
        <ul>{analysis.suggestions.map((item) => <li key={item}>{item}</li>)}</ul>
      </section>

      {matchingAnalysis && <section className="trend-supplemental-analysis" aria-labelledby="trend-supplemental-heading">
        <header className="trend-section-heading">
          <div><span>SUPPLEMENT</span><h3 id="trend-supplemental-heading">补充分析</h3></div>
          <dl className="trend-analysis-provenance" aria-label="补充分析来源">
            <div><dt>来源</dt><dd>{matchingAnalysis.source === "local" ? "本地统计" : matchingAnalysis.source}</dd></div>
            <div><dt>模型</dt><dd>{matchingAnalysis.model}</dd></div>
            <div><dt>置信度</dt><dd>{Math.round(matchingAnalysis.confidence * 100)}%</dd></div>
            <div><dt>证据</dt><dd title={matchingAnalysis.evidenceHash}>{matchingAnalysis.evidenceHash.slice(0, 8)}</dd></div>
          </dl>
        </header>
        <p className="trend-analysis-summary">{matchingAnalysis.summary}</p>
        {supplementalObservations.length > 0 && <section aria-labelledby="trend-supplemental-observations-heading">
          <h4 id="trend-supplemental-observations-heading">补充观察</h4>
          <ul>{supplementalObservations.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>}
        {supplementalSuggestions.length > 0 && <section aria-labelledby="trend-supplemental-suggestions-heading">
          <h4 id="trend-supplemental-suggestions-heading">补充建议</h4>
          <ul>{supplementalSuggestions.map((item) => <li key={item}>{item}</li>)}</ul>
        </section>}
      </section>}
    </section>
  );
}
