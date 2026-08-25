import type { TimerState } from "../types";

/** Format a non-negative second count as `MM:SS`. Floors partial seconds. */
export function formatTime(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds));
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return `${m.toString().padStart(2, "0")}:${sec.toString().padStart(2, "0")}`;
}

/** Human-readable label for a timer phase. */
export function formatPhase(phase: TimerState["phase"]): string {
  switch (phase) {
    case "Focus":
      return "Focus";
    case "ShortBreak":
      return "Short Break";
    case "LongBreak":
      return "Long Break";
  }
}