import { describe, it, expect } from "vitest";
import {
  blankSchedule,
  buildTimerConfig,
  buildWeeklyConfig,
  validateSchedule,
  SCHEDULE_KINDS,
} from "./schedule";

describe("SCHEDULE_KINDS", () => {
  it("lists timer and weekly", () => {
    expect(SCHEDULE_KINDS).toEqual(["timer", "weekly"]);
  });
});

describe("blankSchedule", () => {
  it("returns a timer/enabled draft with 25-min default", () => {
    const s = blankSchedule();
    expect(s.name).toBe("");
    expect(s.kind).toBe("timer");
    expect(s.enabled).toBe(true);
    expect(s.block_rule_ids).toBe("");
    const parsed = JSON.parse(s.config_json);
    expect(parsed.kind).toBe("Timer");
    expect(parsed.duration_min).toBe(25);
  });
});

describe("buildTimerConfig", () => {
  it("emits the expected snake_case JSON", () => {
    const json = buildTimerConfig({ duration_min: 50 });
    expect(JSON.parse(json)).toEqual({
      kind: "Timer",
      duration_min: 50,
      short_break_min: 5,
      long_break_min: 15,
      cycles_before_long: 4,
    });
  });
});

describe("buildWeeklyConfig", () => {
  it("emits a Weekly config with the given days and a single range", () => {
    const json = buildWeeklyConfig({
      days: [1, 2, 3, 4, 5],
      start: "09:00",
      end: "12:00",
    });
    expect(JSON.parse(json)).toEqual({
      kind: "Weekly",
      days: [1, 2, 3, 4, 5],
      ranges: [{ start: "09:00", end: "12:00" }],
    });
  });
});

describe("validateSchedule", () => {
  it("rejects empty name", () => {
    expect(validateSchedule(blankSchedule())).toBe("Name is required");
  });

  it("accepts a valid timer schedule", () => {
    const s = { ...blankSchedule(), name: "Morning focus" };
    expect(validateSchedule(s)).toBeNull();
  });

  it("rejects weekly schedule with no days", () => {
    const s: ReturnType<typeof blankSchedule> = {
      ...blankSchedule(),
      name: "Test",
      kind: "weekly" as const,
      config_json: buildWeeklyConfig({ days: [], start: "09:00", end: "10:00" }),
    };
    expect(validateSchedule(s)).toBe("Weekly schedule needs at least one day");
  });

  it("rejects weekly schedule with no ranges", () => {
    const s: ReturnType<typeof blankSchedule> = {
      ...blankSchedule(),
      name: "Test",
      kind: "weekly" as const,
      config_json: JSON.stringify({ kind: "Weekly", days: [1], ranges: [] }),
    };
    expect(validateSchedule(s)).toBe("Weekly schedule needs at least one time range");
  });
});