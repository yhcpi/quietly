import type { BlockHit } from "../types";

export type HourBucket = { hour: number; count: number };

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

export function formatHourLabel(hour: number): string {
  return `${hour.toString().padStart(2, "0")}:00`;
}
