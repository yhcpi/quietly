import type { Schedule } from "../types";

export type ScheduleDraft = Omit<Schedule, "id" | "created_at">;

export const SCHEDULE_KINDS: ReadonlyArray<Schedule["kind"]> = [
  "timer",
  "weekly",
];

export function blankSchedule(): ScheduleDraft {
  return {
    name: "",
    kind: "timer",
    enabled: true,
    config_json: buildTimerConfig({ duration_min: 25 }),
    block_rule_ids: "",
  };
}

export function buildTimerConfig(opts: { duration_min: number }): string {
  return JSON.stringify({
    kind: "Timer",
    duration_min: opts.duration_min,
    short_break_min: 5,
    long_break_min: 15,
    cycles_before_long: 4,
  });
}

export function buildWeeklyConfig(opts: {
  days: number[];
  start: string;
  end: string;
}): string {
  return JSON.stringify({
    kind: "Weekly",
    days: opts.days,
    ranges: [{ start: opts.start, end: opts.end }],
  });
}

export function validateSchedule(draft: ScheduleDraft): string | null {
  if (!draft.name.trim()) {
    return "Name is required";
  }
  if (draft.kind === "timer") {
    let parsed: unknown;
    try {
      parsed = JSON.parse(draft.config_json);
    } catch {
      return "Invalid timer config JSON";
    }
    if (
      !parsed ||
      typeof parsed !== "object" ||
      typeof (parsed as { duration_min?: unknown }).duration_min !== "number" ||
      (parsed as { duration_min: number }).duration_min < 1
    ) {
      return "Timer duration must be at least 1 minute";
    }
  } else if (draft.kind === "weekly") {
    let parsed: unknown;
    try {
      parsed = JSON.parse(draft.config_json);
    } catch {
      return "Invalid weekly config JSON";
    }
    if (!parsed || typeof parsed !== "object") {
      return "Invalid weekly config";
    }
    const obj = parsed as { days?: unknown; ranges?: unknown };
    if (!Array.isArray(obj.days) || obj.days.length === 0) {
      return "Weekly schedule needs at least one day";
    }
    if (!Array.isArray(obj.ranges) || obj.ranges.length === 0) {
      return "Weekly schedule needs at least one time range";
    }
  }
  return null;
}