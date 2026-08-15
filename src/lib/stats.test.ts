import { describe, it, expect } from "vitest";
import { hitsByHour, formatHourLabel } from "./stats";
import type { BlockHit } from "./../types";

function hitAtUnix(unixSeconds: number): BlockHit {
  return {
    id: 1,
    ts: unixSeconds,
    kind: "url",
    target: "x",
    rule_id: null,
    schedule_id: null,
  };
}

describe("hitsByHour", () => {
  it("returns 24 zero buckets for empty input", () => {
    const buckets = hitsByHour([]);
    expect(buckets).toHaveLength(24);
    for (let h = 0; h < 24; h++) {
      expect(buckets[h]).toEqual({ hour: h, count: 0 });
    }
  });

  it("buckets hits into the local-time hour", () => {
    // Use a known local-time instant: 2024-01-15 14:30:00 local.
    // Build it from a Date constructed in local time so the test is
    // timezone-stable.
    const d = new Date(2024, 0, 15, 14, 30, 0);
    const hit = hitAtUnix(Math.floor(d.getTime() / 1000));
    const buckets = hitsByHour([hit]);
    expect(buckets[14].count).toBe(1);
    expect(buckets[13].count).toBe(0);
  });

  it("counts multiple hits in the same hour", () => {
    const a = new Date(2024, 0, 15, 9, 0, 0);
    const b = new Date(2024, 0, 15, 9, 45, 0);
    const c = new Date(2024, 0, 15, 10, 0, 0);
    const hits = [a, b, c].map((d) => hitAtUnix(Math.floor(d.getTime() / 1000)));
    const buckets = hitsByHour(hits);
    expect(buckets[9].count).toBe(2);
    expect(buckets[10].count).toBe(1);
  });
});

describe("formatHourLabel", () => {
  it("pads single-digit hours", () => {
    expect(formatHourLabel(0)).toBe("00:00");
    expect(formatHourLabel(5)).toBe("05:00");
  });

  it("formats double-digit hours as-is", () => {
    expect(formatHourLabel(13)).toBe("13:00");
    expect(formatHourLabel(23)).toBe("23:00");
  });
});
