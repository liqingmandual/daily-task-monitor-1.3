import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { addTrendCalendarDays } from "../../lib/trend-range";

interface TrendDateCalendarProps {
  initialMonth: string;
  selectedDates: string[];
  onSelectedDatesChange: (dates: string[]) => void;
}

const weekdayShort = ["一", "二", "三", "四", "五", "六", "日"];
const weekdayLong = ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"];
const singleMonthQuery = "(max-width: 760px)";

function useSingleMonthCalendar(): boolean {
  const [singleMonth, setSingleMonth] = useState(() => typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia(singleMonthQuery).matches);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const media = window.matchMedia(singleMonthQuery);
    const update = (event: MediaQueryListEvent) => setSingleMonth(event.matches);
    setSingleMonth(media.matches);
    if (typeof media.addEventListener === "function") {
      media.addEventListener("change", update);
      return () => media.removeEventListener("change", update);
    }
    media.addListener(update);
    return () => media.removeListener(update);
  }, []);

  return singleMonth;
}

function monthStart(value: string): string {
  return `${value.slice(0, 7)}-01`;
}

function addMonths(value: string, amount: number): string {
  const [year, month] = value.split("-").map(Number);
  const date = new Date(Date.UTC(year, month - 1 + amount, 1));
  return date.toISOString().slice(0, 10);
}

function monthLabel(value: string): string {
  const [year, month] = value.split("-").map(Number);
  return `${year}年${month}月`;
}

function dateLabel(value: string): string {
  const [year, month, day] = value.split("-").map(Number);
  const weekday = new Date(`${value}T00:00:00Z`).getUTCDay();
  return `${year}年${month}月${day}日，${weekdayLong[weekday]}`;
}

function monthCells(value: string): Array<string | null> {
  const firstWeekday = (new Date(`${value}T00:00:00Z`).getUTCDay() + 6) % 7;
  const nextMonth = addMonths(value, 1);
  const lastDate = Number(addTrendCalendarDays(nextMonth, -1).slice(-2));
  return Array.from({ length: 42 }, (_, index) => {
    const day = index - firstWeekday + 1;
    return day >= 1 && day <= lastDate ? `${value.slice(0, 8)}${String(day).padStart(2, "0")}` : null;
  });
}

export function TrendDateCalendar({ initialMonth, selectedDates, onSelectedDatesChange }: TrendDateCalendarProps) {
  const singleMonth = useSingleMonthCalendar();
  const [visibleMonth, setVisibleMonth] = useState(() => monthStart(initialMonth));
  const [focusDate, setFocusDate] = useState(() => selectedDates[0] ?? monthStart(initialMonth));
  const pendingFocus = useRef<string | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const selected = new Set(selectedDates);

  useEffect(() => {
    const nextMonth = monthStart(initialMonth);
    setVisibleMonth(nextMonth);
    setFocusDate(selectedDates[0] ?? nextMonth);
  }, [initialMonth]);

  useEffect(() => {
    const date = pendingFocus.current;
    if (!date) return;
    pendingFocus.current = null;
    rootRef.current?.querySelector<HTMLButtonElement>(`[data-date="${date}"]`)?.focus();
  }, [visibleMonth]);

  useEffect(() => {
    if (!singleMonth || focusDate.slice(0, 7) === visibleMonth.slice(0, 7)) return;
    pendingFocus.current = focusDate;
    setVisibleMonth(monthStart(focusDate));
  }, [focusDate, singleMonth, visibleMonth]);

  const toggleDate = (date: string) => {
    setFocusDate(date);
    onSelectedDatesChange(selected.has(date)
      ? selectedDates.filter((item) => item !== date)
      : [...selectedDates, date].sort());
  };

  const focusTarget = (date: string) => {
    setFocusDate(date);
    const lastVisibleMonth = addMonths(visibleMonth, singleMonth ? 0 : 1).slice(0, 7);
    const targetMonth = date.slice(0, 7);
    if (targetMonth < visibleMonth.slice(0, 7)) {
      pendingFocus.current = date;
      setVisibleMonth(monthStart(date));
      return;
    }
    if (targetMonth > lastVisibleMonth) {
      pendingFocus.current = date;
      setVisibleMonth(singleMonth ? monthStart(date) : addMonths(monthStart(date), -1));
      return;
    }
    rootRef.current?.querySelector<HTMLButtonElement>(`[data-date="${date}"]`)?.focus();
  };

  const handleDayKeyDown = (event: KeyboardEvent<HTMLButtonElement>, date: string) => {
    const offsets: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 };
    if (event.key in offsets) {
      event.preventDefault();
      focusTarget(addTrendCalendarDays(date, offsets[event.key]));
      return;
    }
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      toggleDate(date);
    }
  };

  const navigate = (amount: number) => {
    const next = addMonths(visibleMonth, amount);
    pendingFocus.current = next;
    setVisibleMonth(next);
    setFocusDate(next);
  };

  const renderMonth = (month: string, secondary: boolean) => (
    <section className={`trend-calendar-month${secondary ? " trend-calendar-month-secondary" : ""}`} key={month}>
      <h3>{monthLabel(month)}</h3>
      <div className="trend-calendar-weekdays" aria-hidden="true">
        {weekdayShort.map((day) => <span key={day}>{day}</span>)}
      </div>
      <div className="trend-calendar-grid" role="grid" aria-label={monthLabel(month)}>
        {monthCells(month).map((date, index) => <div role="gridcell" className="trend-calendar-cell" key={date ?? `empty-${index}`}>
          {date && <button
            type="button"
            data-date={date}
            aria-label={dateLabel(date)}
            aria-pressed={selected.has(date)}
            tabIndex={focusDate === date ? 0 : -1}
            onClick={() => toggleDate(date)}
            onKeyDown={(event) => handleDayKeyDown(event, date)}
          >{Number(date.slice(-2))}</button>}
        </div>)}
      </div>
    </section>
  );

  return <div className="trend-date-calendar" ref={rootRef}>
    <div className="trend-calendar-navigation">
      <button type="button" className="icon-button" aria-label="上一个月" title="上一个月" onClick={() => navigate(-1)}><ChevronLeft size={16} /></button>
      <span aria-live="polite">{singleMonth ? monthLabel(visibleMonth) : `${monthLabel(visibleMonth)} - ${monthLabel(addMonths(visibleMonth, 1))}`}</span>
      <button type="button" className="icon-button" aria-label="下一个月" title="下一个月" onClick={() => navigate(1)}><ChevronRight size={16} /></button>
    </div>
    <div className="trend-calendar-months">
      {renderMonth(visibleMonth, false)}
      {!singleMonth && renderMonth(addMonths(visibleMonth, 1), true)}
    </div>
  </div>;
}
