import { BrainCircuit, CheckCircle2, FlaskConical, Lightbulb, SearchCheck } from "lucide-react";
import type { ActivityScope } from "../../lib/activity-composition";
import type { ScopedDailyAnalysisResult } from "../../lib/desktop";
import { PanelHeading } from "./analysis-shared";

function activityScopeLabel(scope: ActivityScope): string {
  return scope === "meaningful" ? "学习" : "全部";
}

export function AiAnalysisPanel({
  analysis,
  onReanalyze,
}: {
  analysis: ScopedDailyAnalysisResult;
  onReanalyze?: () => void;
}) {
  const findings = analysis.findings ?? [];
  return (
    <article className="panel ai-analysis-panel">
      <div className="ai-analysis-heading">
        <PanelHeading eyebrow="AI ANALYSIS" title="AI 分析" icon={<BrainCircuit size={18} />} />
        <span className="analysis-scope">当前口径：{activityScopeLabel(analysis.activityScope)}</span>
        {onReanalyze ? <button type="button" onClick={onReanalyze}>重新分析</button> : null}
      </div>
      {findings.length ? (
        <div className="evidence-findings">
          {findings.map((finding, index) => (
            <section className="evidence-finding-card" key={`${finding.observation}-${index}`}>
              <header>
                <span>发现 {index + 1}</span>
                <small>置信度 {Math.round(finding.confidence * 100)}%</small>
              </header>
              <dl>
                <div>
                  <dt><SearchCheck size={15} /> 事实观察</dt>
                  <dd>{finding.observation}</dd>
                </div>
                <div>
                  <dt><BrainCircuit size={15} /> 可能解释</dt>
                  <dd>{finding.hypothesis}</dd>
                </div>
                <div>
                  <dt><FlaskConical size={15} /> 如何验证</dt>
                  <dd>{finding.validation}</dd>
                </div>
                <div>
                  <dt><CheckCircle2 size={15} /> 下一步</dt>
                  <dd>{finding.action}</dd>
                </div>
              </dl>
              {finding.limitations.length ? (
                <p className="finding-limit">数据限制：{finding.limitations.join("；")}</p>
              ) : null}
            </section>
          ))}
        </div>
      ) : (
        <div className="ai-analysis-grid">
          <section className="analysis-block portrait">
            <span><BrainCircuit size={16} /> 今日工作画像</span>
            <p>{analysis.portrait}</p>
          </section>
          <section className="analysis-block recommendation">
            <span><Lightbulb size={16} /> 评价与建议</span>
            <p>{analysis.recommendation}</p>
          </section>
        </div>
      )}
      <footer className={`analysis-status source-${analysis.source}`}>
        {analysis.source === "ai"
          ? "AI 已按当前证据补算；所有解释均为待验证假设"
          : analysis.source === "queued"
            ? "本地分析可用，等待联网后补算"
            : "基于本地统计生成；未生成未经证据支持的因果判断"}
      </footer>
    </article>
  );
}
