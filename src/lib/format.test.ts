import { describe, it, expect } from "vitest";
import { formatTime, formatPhase } from "./format";

describe("formatTime", () => {
  it("formats 0 as 00:00", () => {
    expect(formatTime(0)).toBe("00:00");
  });

  it("pads single-digit seconds", () => {
    expect(formatTime(5)).toBe("00:05");
  });

  it("formats 65 as 01:05", () => {
    expect(formatTime(65)).toBe("01:05");
  });

  it("formats 1500 (25 min) as 25:00", () => {
    expect(formatTime(1500)).toBe("25:00");
  });

  it("formats 3600 as 60:00", () => {
    expect(formatTime(3600)).toBe("60:00");
  });

  it("clamps negative input to 00:00", () => {
    expect(formatTime(-1)).toBe("00:00");
  });

  it("floors fractional input", () => {
    expect(formatTime(65.9)).toBe("01:05");
  });
});

describe("formatPhase", () => {
  it("Focus → 'Focus'", () => {
    expect(formatPhase("Focus")).toBe("Focus");
  });

  it("ShortBreak → 'Short Break'", () => {
    expect(formatPhase("ShortBreak")).toBe("Short Break");
  });

  it("LongBreak → 'Long Break'", () => {
    expect(formatPhase("LongBreak")).toBe("Long Break");
  });
});