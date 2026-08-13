import { useEffect, useState } from "react";
import { Focus } from "lucide-react";
import type { WorkLedgerTask } from "../../lib/desktop";
import { nextFocusTickDelay, remainingFocusSeconds } from "../../lib/focus-countdown";
import { CompactFocusPopover, formatFocusCountdown } from "./CompactFocusPopover";

export function CompactFocusControl({ goal, minutes, running, paused, endsAtMs, pausedRemainingSeconds, taskId, tasks, canStart, onTaskChange, onMinutesChange, onToggle }: {
  goal: string;
  minutes: number;
  running: boolean;
  paused: boolean;
  endsAtMs: number | null;
  pausedRemainingSeconds: number | null;
  taskId: string;
  tasks: WorkLedgerTask[];
  canStart: boolean;
  onTaskChange: (taskId: string) => void;
  onMinutesChange: (minutes: number) => void;
  onToggle: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [clockMs, setClockMs] = useState(() => Date.now());

  useEffect(() => {
    if (!running || paused || endsAtMs === null) return;
    let timer: number | undefined;
    const updateClock = () => {
      if (timer !== undefined) window.clearTimeout(timer);
      const observedAtMs = Date.now();
      setClockMs(observedAtMs);
      const delay = nextFocusTickDelay(endsAtMs, observedAtMs);
      if (delay !== null) timer = window.setTimeout(updateClock, delay);
    };
    const refreshWhenVisible = () => {
      if (document.visibilityState !== "hidden") updateClock();
    };
    updateClock();
    window.addEventListener("focus", updateClock);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      if (timer !== undefined) window.clearTimeout(timer);
      window.removeEventListener("focus", updateClock);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [endsAtMs, paused, running]);

  const remainingSeconds = running
    ? paused
      ? pausedRemainingSeconds
      : endsAtMs !== null
        ? remainingFocusSeconds(endsAtMs, clockMs)
        : null
    : null;
  const countdown = remainingSeconds === null ? null : formatFocusCountdown(remainingSeconds);

  return <div className="focus-popover-anchor">
    <button className={`icon-button focus-timer-trigger ${running ? "running" : ""} ${paused ? "paused" : ""}`} aria-label={countdown ? `打开专注工具，${paused ? "已暂停，剩余" : "剩余"} ${countdown}` : "打开专注工具"} aria-expanded={open} onClick={() => setOpen((value) => !value)}><Focus size={18} />{countdown && <span>{paused ? "Ⅱ " : ""}{countdown}</span>}</button>
    {open && <CompactFocusPopover goal={goal} minutes={minutes} running={running} paused={paused} remainingSeconds={remainingSeconds} taskId={taskId} tasks={tasks} canStart={canStart} onTaskChange={onTaskChange} onMinutesChange={onMinutesChange} onToggle={onToggle} />}
  </div>;
}
