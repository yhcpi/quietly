import { describe, it, expect } from "vitest";
import {
  blankRule,
  validateRule,
  RULE_KINDS,
  RULE_MATCH_TYPES,
} from "./rule";

describe("blankRule", () => {
  it("returns a URL/contains/active draft with empty pattern", () => {
    const r = blankRule();
    expect(r).toEqual({
      kind: "url",
      pattern: "",
      match_type: "contains",
      note: "",
      active: true,
    });
  });
});

describe("RULE_KINDS / RULE_MATCH_TYPES", () => {
  it("RULE_KINDS lists the form-selectable kinds", () => {
    expect(RULE_KINDS).toEqual(["url", "domain", "app_path", "title"]);
  });

  it("RULE_MATCH_TYPES lists the three valid match types", () => {
    expect(RULE_MATCH_TYPES).toEqual(["exact", "contains", "regex"]);
  });
});

describe("validateRule", () => {
  it("rejects empty pattern", () => {
    const r = blankRule();
    expect(validateRule(r)).toBe("Pattern is required");
  });

  it("rejects whitespace-only pattern", () => {
    const r = { ...blankRule(), pattern: "   " };
    expect(validateRule(r)).toBe("Pattern is required");
  });

  it("accepts a non-empty contains pattern", () => {
    const r = { ...blankRule(), pattern: "twitter.com" };
    expect(validateRule(r)).toBeNull();
  });

  it("accepts a valid regex", () => {
    const r = { ...blankRule(), pattern: "^https://.*$", match_type: "regex" as const };
    expect(validateRule(r)).toBeNull();
  });

  it("rejects an invalid regex", () => {
    const r = { ...blankRule(), pattern: "[unclosed", match_type: "regex" as const };
    expect(validateRule(r)).toBe("Invalid regex pattern");
  });
});