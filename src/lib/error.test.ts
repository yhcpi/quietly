import { describe, it, expect } from "vitest";
import { humanizeError } from "./error";

describe("humanizeError", () => {
  it("returns the message of an Error", () => {
    expect(humanizeError(new Error("boom"))).toBe("boom");
  });

  it("returns a plain string unchanged", () => {
    expect(humanizeError("boom")).toBe("boom");
  });

  it("strips surrounding JSON quotes from a serde_json string", () => {
    expect(humanizeError('"session is locked"')).toBe("session is locked");
  });

  it("unescapes inner escaped quotes", () => {
    expect(humanizeError('"he said \\"hi\\""')).toBe('he said "hi"');
  });

  it("falls back to String() for unknown shapes", () => {
    expect(humanizeError({ code: 1 })).toBe("[object Object]");
  });

  it("leaves a string with one quote alone (not a JSON wrapper)", () => {
    expect(humanizeError('"unbalanced')).toBe('"unbalanced');
  });
});