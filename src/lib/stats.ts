import type { BlockHit } from "../types";

/** A single hour-of-day bucket in the block-hit histogram. */
export type HourBucket = { hour: number; count: number };

/**
 * Group block hits into 24 hour-of-day buckets (0..23) using the
 * browser's local timezone. Always returns a 24-element array so the
 * chart has a stable x-axis even on empty days.
 */
export function hitsByHour(hits: BlockHit[]): HourBucket[] {
  const buckets: HourBucket[] = Array.from({ length: 24 }, (_, hour) => ({
    hour,
    count: 0,
  }));
  for (const hit of hits) {
    const d = new Date(hit.ts * 1000);
    const h = d.getHours();
    if (h >= 0 && h < 24) {
      buckets[h].count += 1;
    }
  }
  return buckets;
}

/** Format an hour-of-day bucket label as `HH:00`. */
export function formatHourLabel(hour: number): string {
  return `${hour.toString().padStart(2, "0")}:00`;
}
