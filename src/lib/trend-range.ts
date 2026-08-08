export type TrendPreset = "week" | "month" | "custom";
export type TrendGranularity = "day" | "week" | "month";
export type TrendCustomMode = "continuous" | "specific";

export interface TrendRangeSelection {
  startDate: string;
  endDate: string;
}

const DAY_MS = 86_400_000;
const presets: TrendPreset[] = ["week", "month", "custom"];

function parseCalendarDate(value: string): Date {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) throw new RangeError("日期无效，请使用 YYYY-MM-DD 格式");
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(Date.UTC(year, month - 1, day));
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) {
    throw new RangeError("日期无效");
  }
  return date;
}

function formatCalendarDate(date: Date): string {
  return [
    String(date.getUTCFullYear()).padStart(4, "0"),
    String(date.getUTCMonth() + 1).padStart(2, "0"),
    String(date.getUTCDate()).padStart(2, "0"),
  ].join("-");
}

export function addTrendCalendarDays(value: string, days: number): string {
  const date = parseCalendarDate(value);
  date.setUTCDate(date.getUTCDate() + days);
  return formatCalendarDate(date);
}

export function inclusiveTrendDayCount(range: TrendRangeSelection): number {
  const start = parseCalendarDate(range.startDate);
  const end = parseCalendarDate(range.endDate);
  return Math.round((end.getTime() - start.getTime()) / DAY_MS) + 1;
}

export function defaultTrendGranularity(range: TrendRangeSelection): TrendGranularity {
  const dayCount = inclusiveTrendDayCount(range);
  if (dayCount < 1) throw new RangeError("开始日期不能晚于结束日期");
  if (dayCount > 366) throw new RangeError("自定义范围最多选择 366 天");
  if (dayCount <= 31) return "day";
  if (dayCount <= 120) return "week";
  return "month";
}

export function resolveTrendRange(
  preset: TrendPreset,
  anchorDate: string,
  customStart: string,
  customEnd: string,
): TrendRangeSelection {
  if (preset !== "custom") {
    parseCalendarDate(anchorDate);
    const dayCount = preset === "week" ? 7 : 30;
    return { startDate: addTrendCalendarDays(anchorDate, -(dayCount - 1)), endDate: anchorDate };
  }

  if (!customStart || !customEnd) throw new RangeError("请选择开始和结束日期");
  const range = { startDate: customStart, endDate: customEnd };
  const dayCount = inclusiveTrendDayCount(range);
  if (dayCount < 1) throw new RangeError("开始日期不能晚于结束日期");
  if (dayCount > 366) throw new RangeError("自定义范围最多选择 366 天");
  return range;
}

export function shiftTrendRange(range: TrendRangeSelection, direction: -1 | 1): TrendRangeSelection {
  const dayCount = inclusiveTrendDayCount(range);
  if (dayCount < 1) throw new RangeError("开始日期不能晚于结束日期");
  return {
    startDate: addTrendCalendarDays(range.startDate, direction * dayCount),
    endDate: addTrendCalendarDays(range.endDate, direction * dayCount),
  };
}

export function normalizeTrendSelectedDates(values: string[]): string[] {
  const normalized = [...new Set(values)];
  for (const value of normalized) parseCalendarDate(value);
  return normalized.sort();
}

export function resolveTrendSelectedDates(values: string[]): {
  range: TrendRangeSelection;
  selectedDates: string[];
} {
  const selectedDates = normalizeTrendSelectedDates(values);
  if (!selectedDates.length) throw new RangeError("请至少选择 1 天");
  const range = {
    startDate: selectedDates[0],
    endDate: selectedDates[selectedDates.length - 1],
  };
  if (inclusiveTrendDayCount(range) > 366) throw new RangeError("指定日期的日期跨度最多 366 天");
  return { range, selectedDates };
}

export function defaultTrendGranularityForSelectedDates(values: string[]): TrendGranularity {
  const { selectedDates } = resolveTrendSelectedDates(values);
  if (selectedDates.length <= 31) return "day";
  if (selectedDates.length <= 120) return "week";
  return "month";
}

export function shiftTrendSelectedDates(values: string[], direction: -1 | 1): string[] {
  const { range, selectedDates } = resolveTrendSelectedDates(values);
  const offset = direction * inclusiveTrendDayCount(range);
  return selectedDates.map((date) => addTrendCalendarDays(date, offset));
}

export function moveTrendPreset(current: TrendPreset, key: string): TrendPreset | null {
  if (key === "Home") return presets[0];
  if (key === "End") return presets[presets.length - 1];
  if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(key)) return null;
  const direction = key === "ArrowRight" || key === "ArrowDown" ? 1 : -1;
  const index = presets.indexOf(current);
  return presets[(index + direction + presets.length) % presets.length];
}
