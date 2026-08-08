import { ArrowLeft, ArrowRight, Download, RefreshCw, X } from "lucide-react";
import type { ReportFormat, TrendGranularity, TrendMetric, TrendMetricAvailability } from "../../lib/desktop";
import type { TrendCustomMode, TrendPreset, TrendRangeSelection } from "../../lib/trend-range";
import { TrendDateCalendar } from "./TrendDateCalendar";

export const trendMetricOptions: Array<{ value: TrendMetric; label: string }> = [
  { value: "monitoredSeconds", label: "监控时长" },
  { value: "activeSeconds", label: "活跃时长" },
  { value: "learningSeconds", label: "学习时长" },
  { value: "idleSeconds", label: "不活跃时长" },
  { value: "switchCount", label: "切换次数" },
  { value: "longestFocusSeconds", label: "最长专注块" },
  { value: "classificationCoverage", label: "分类覆盖率" },
  { value: "completedTaskCount", label: "任务完成数" },
  { value: "linkedTaskSeconds", label: "已关联任务时长" },
];

interface TrendRangeToolbarProps {
  preset: TrendPreset;
  range: TrendRangeSelection | null;
  customStart: string;
  customEnd: string;
  customMode: TrendCustomMode;
  specificDraftDates: string[];
  specificAppliedDates: string[];
  specificSelectionError: string;
  granularity: TrendGranularity;
  metric: TrendMetric;
  metricAvailability: TrendMetricAvailability[];
  customBaselineEnabled: boolean;
  customBaselineStart: string;
  customBaselineEnd: string;
  loading: boolean;
  exportDisabled: boolean;
  onPresetChange: (preset: TrendPreset) => void;
  onCustomStartChange: (value: string) => void;
  onCustomEndChange: (value: string) => void;
  onCustomModeChange: (value: TrendCustomMode) => void;
  onSpecificDraftDatesChange: (dates: string[]) => void;
  onClearSpecificDates: () => void;
  onApplySpecificDates: () => void;
  onRemoveSpecificDate: (date: string) => void;
  onGranularityChange: (value: TrendGranularity) => void;
  onMetricChange: (value: TrendMetric) => void;
  onCustomBaselineEnabledChange: (value: boolean) => void;
  onCustomBaselineStartChange: (value: string) => void;
  onCustomBaselineEndChange: (value: string) => void;
  onShift: (direction: -1 | 1) => void;
  onRefresh: () => void;
  onExport: (format: ReportFormat) => void;
}

const presetLabels: Record<TrendPreset, string> = { week: "近 7 天", month: "近 30 天", custom: "自定义" };

export function TrendRangeToolbar(props: TrendRangeToolbarProps) {
  const availability = new Map(props.metricAvailability.map((item) => [item.metric, item]));
  const specificStart = props.specificAppliedDates[0];
  const specificEnd = props.specificAppliedDates.at(-1);
  return <section className="trend-workbench-toolbar" aria-labelledby="trend-workbench-range-heading">
    <div className="trend-toolbar-title">
      <h1 id="trend-workbench-range-heading">趋势概览</h1>
      <p>按所选时间范围汇总 · {props.range ? `${props.range.startDate} ~ ${props.range.endDate}` : "请选择有效日期"}</p>
    </div>
    <div className="trend-toolbar-line">
      <div className="trend-preset-control" role="radiogroup" aria-label="趋势范围预设">
        {(Object.keys(presetLabels) as TrendPreset[]).map((item) => <button type="button" role="radio" aria-label={presetLabels[item]} aria-checked={props.preset === item} data-preset={item} className={props.preset === item ? "active" : ""} key={item} onClick={() => props.onPresetChange(item)}>{presetLabels[item]}</button>)}
      </div>
      <label className="trend-select-field"><span>指标</span><select aria-label="趋势指标" value={props.metric} onChange={(event) => props.onMetricChange(event.target.value as TrendMetric)}>
        {trendMetricOptions.map((item) => {
          const unavailable = availability.get(item.value)?.status === "unavailable";
          return <option key={item.value} value={item.value} disabled={unavailable}>{item.label}{unavailable ? "（当前数据不可用）" : ""}</option>;
        })}
      </select></label>
      <div className="trend-granularity" role="radiogroup" aria-label="时间粒度">
        {(["day", "week", "month"] as TrendGranularity[]).map((item) => <button type="button" role="radio" aria-checked={props.granularity === item} className={props.granularity === item ? "active" : ""} key={item} onClick={() => props.onGranularityChange(item)}>{{ day: "日", week: "周", month: "月" }[item]}</button>)}
      </div>
      <button className="trend-toggle" type="button" role="switch" aria-label="启用自定义基准" aria-checked={props.customBaselineEnabled} onClick={() => props.onCustomBaselineEnabledChange(!props.customBaselineEnabled)}><span aria-hidden="true" />自定义基准</button>
      <div className="trend-toolbar-actions">
        <button type="button" className="icon-button" aria-label="上一时间段" title="上一时间段" disabled={!props.range} onClick={() => props.onShift(-1)}><ArrowLeft size={17} /></button>
        <button type="button" className="icon-button" aria-label="下一时间段" title="下一时间段" disabled={!props.range} onClick={() => props.onShift(1)}><ArrowRight size={17} /></button>
        <button type="button" className="icon-button" aria-label="刷新趋势" title="刷新趋势" disabled={!props.range || props.loading} onClick={props.onRefresh}><RefreshCw size={17} /></button>
        <div className="trend-export-actions" aria-label="导出报告">
          <button type="button" className="icon-button" aria-label="导出 Markdown" title="导出 Markdown" disabled={props.exportDisabled} onClick={() => props.onExport("markdown")}><Download size={17} /></button>
          <button type="button" className="trend-compact-button" aria-label="导出 Word" title="导出 Word" disabled={props.exportDisabled} onClick={() => props.onExport("docx")}>Word</button>
        </div>
      </div>
    </div>
    {props.preset === "custom" && <div className="trend-custom-selection">
      <div className="trend-custom-mode" role="radiogroup" aria-label="自定义日期选择方式">
        <button type="button" role="radio" data-custom-mode="continuous" aria-checked={props.customMode === "continuous"} className={props.customMode === "continuous" ? "active" : ""} onClick={() => props.onCustomModeChange("continuous")}>连续区间</button>
        <button type="button" role="radio" data-custom-mode="specific" aria-checked={props.customMode === "specific"} className={props.customMode === "specific" ? "active" : ""} onClick={() => props.onCustomModeChange("specific")}>指定日期</button>
      </div>
      {props.customMode === "continuous" && <div className="trend-date-line"><label>开始日期<input aria-label="开始日期" type="date" value={props.customStart} onChange={(event) => props.onCustomStartChange(event.target.value)} /></label><span>至</span><label>结束日期<input aria-label="结束日期" type="date" value={props.customEnd} onChange={(event) => props.onCustomEndChange(event.target.value)} /></label></div>}
      {props.customMode === "specific" && <div className="trend-specific-selection">
        <TrendDateCalendar
          initialMonth={specificStart ?? props.range?.startDate ?? props.customStart}
          selectedDates={props.specificDraftDates}
          onSelectedDatesChange={props.onSpecificDraftDatesChange}
        />
        <div className="trend-specific-actions">
          <button type="button" className="trend-compact-button" data-action="clear-specific-dates" onClick={props.onClearSpecificDates}>清空</button>
          <button type="button" className="trend-primary-button" data-action="apply-specific-dates" onClick={props.onApplySpecificDates}>应用选择</button>
        </div>
        {props.specificSelectionError && <p className="trend-specific-status error" role="alert">{props.specificSelectionError}</p>}
        <div className="trend-applied-selection" aria-live="polite">
          <div className="trend-range-summary">{props.specificAppliedDates.length
            ? <><strong>已选 {props.specificAppliedDates.length} 天</strong><span>{specificStart} 至 {specificEnd}</span></>
            : "尚未应用指定日期"}</div>
          <div className="trend-date-chips" aria-label="已应用日期">
            {props.specificAppliedDates.map((date) => <button type="button" className="trend-date-chip" aria-label={`移除 ${date}`} key={date} onClick={() => props.onRemoveSpecificDate(date)}><span>{date}</span><X size={13} aria-hidden="true" /></button>)}
          </div>
        </div>
      </div>}
    </div>}
    {props.customBaselineEnabled && <div className="trend-date-line custom-baseline-dates"><label>基准开始<input type="date" value={props.customBaselineStart} onChange={(event) => props.onCustomBaselineStartChange(event.target.value)} /></label><span>至</span><label>基准结束<input type="date" value={props.customBaselineEnd} onChange={(event) => props.onCustomBaselineEndChange(event.target.value)} /></label></div>}
    {!(props.preset === "custom" && props.customMode === "specific") && <div className="trend-range-summary trend-range-summary-accessible" aria-live="polite">{props.range ? `${props.range.startDate} 至 ${props.range.endDate}` : "请选择有效日期"}</div>}
  </section>;
}
