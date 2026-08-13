import { CirclePause, Focus, Play } from "lucide-react";
import type { WorkLedgerTask } from "../../lib/desktop";

export function formatFocusCountdown(remainingSeconds: number): string {
  const safeSeconds = Math.max(0, Math.ceil(remainingSeconds));
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export function CompactFocusPopover({ goal, minutes, running, paused, remainingSeconds, taskId, tasks, canStart, onTaskChange, onMinutesChange, onToggle }: {
  goal: string;
  minutes: number;
  running: boolean;
  paused: boolean;
  remainingSeconds: number | null;
  taskId: string;
  tasks: WorkLedgerTask[];
  canStart: boolean;
  onTaskChange: (taskId: string) => void;
  onMinutesChange: (minutes: number) => void;
  onToggle: () => void;
}) {
  const countdown = remainingSeconds === null ? null : formatFocusCountdown(remainingSeconds);

  return <section className="compact-focus-popover" role="dialog" aria-label="专注工具">
    <div className="compact-focus-heading"><span>FOCUS</span><Focus size={16} /></div>
    <strong>{goal}</strong>
    {running && remainingSeconds !== null && countdown !== null && <div className="compact-focus-countdown" role="timer" aria-label="番茄钟剩余时间" aria-live="off">
      <span>{remainingSeconds <= 0 ? "TIME UP" : paused ? "PAUSED" : "REMAINING"}</span>
      <strong key={countdown} data-remaining-seconds={remainingSeconds}>{countdown}</strong>
    </div>}
    <label className="compact-focus-task"><span>Task</span><select aria-label="Focus task" value={taskId} onChange={(event) => onTaskChange(event.target.value)} disabled={running}><option value="">No linked task</option>{tasks.map((task) => <option key={task.id} value={task.id}>{task.title}</option>)}</select></label>
    <div className="compact-duration-switch" aria-label="专注时长">
      {[25, 45, 60].map((option) => (
        <button key={option} className={minutes === option ? "active" : ""} onClick={() => onMinutesChange(option)}>{option} 分钟</button>
      ))}
    </div>
    <button className={`compact-focus-action ${running ? "running" : ""}`} onClick={onToggle} disabled={!running && !canStart}>
      {running ? <CirclePause size={16} /> : <Play size={16} />}
      {running ? "结束并记录产出" : `开始 ${minutes} 分钟`}
    </button>
  </section>;
}
